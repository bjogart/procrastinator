//! The default [`Storage`](crate::storage::Storage) implementation.
//!
//! See [`Storage`] for details.

use crate::INCONSISTENT_STATE;
use crate::handler::Event;
use crate::handler::EventKind;
use crate::handler::Handler;
use crate::panic_expected_different_type;
use alloc::sync::Arc;
use core::any::Any;
use core::any::TypeId;
use core::hash::BuildHasher as _;
use core::hash::Hash;
use core::hash::Hasher as _;
use hashbrown::HashTable;
use std::hash::RandomState;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;

use crate::storage::Storage as StorageTrait;
use crate::storage::Transfer as TransferTrait;

/// The default storage: deduplicated values behind [`Arc`]s.
///
/// Each distinct value is stored once, in an [`Arc`]. Values are told apart
/// by type and by their [`Hash`] and [`Eq`] implementations, so storing a
/// value equal to one that is already stored returns the existing
/// [`StorageId`]. Query results are returned as <code>[Arc]&lt;T&gt;</code>,
/// so reading a memoized output never clones it.
///
/// Values stay stored until [`Db::gc`](crate::Db::gc) drops the ones that
/// are no longer needed.
///
/// # Examples
///
/// ```
/// use procrastinator::Db;
/// use procrastinator::query::Input;
/// use procrastinator::storage::default;
/// use std::sync::Arc;
///
/// struct Name;
/// impl Input for Name {
///     type Value = String;
/// }
///
/// let mut db: Db<default::Storage> = Db::default();
/// let name = db.new_input::<Name>(&"Ferris".to_owned());
/// let handle: Arc<String> = db.snapshot().query::<Name>(&name);
/// assert_eq!(*handle, "Ferris");
/// ```
///
/// # Panics
///
/// A panic in the [`Clone`], [`Eq`], or [`Hash`] implementation of a value,
/// or in the handler, propagates to the caller and leaves the storage
/// unchanged. User code only runs outside of the write lock, so the lock is
/// never poisoned.
#[derive(Debug, Default)]
pub struct Storage {
    hasher: RandomState,
    inner: RwLock<StorageInner>,
}

/// Copies selected values from a [`Storage`] into a new one.
///
/// Created by [`Storage::transfer`](crate::storage::Storage::transfer). It
/// holds a read lock on the old storage until it is dropped or turned into
/// the new storage with
/// [`Transfer::into_storage`](crate::storage::Transfer::into_storage).
/// See [`Storage::transfer`](crate::storage::Storage::transfer) for an
/// example.
#[derive(Debug)]
pub struct Transfer<'storage> {
    curr: RwLockReadGuard<'storage, StorageInner>,
    hasher: RandomState,
    next: StorageInner,
}

#[derive(Debug, Default)]
struct StorageInner {
    index: HashTable<(u64, StorageId)>,
    values: Vec<Arc<dyn Any + Send + Sync + 'static>>,
}

/// Identifies a value in a [`Storage`].
///
/// An ID is only meaningful for the storage that created it. After
/// [`Db::gc`](crate::Db::gc), values live in a new storage under new IDs.
/// See [`Storage::store`](crate::storage::Storage::store) for an example.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StorageId(usize);

impl StorageTrait for Storage {
    type Id = StorageId;

    type Handle = Arc<dyn Any + Send + Sync + 'static>;

    type Transfer<'storage> = Transfer<'storage>;

    fn store<H, T>(&self, handler: &H, value: &T) -> Self::Id
    where
        H: Handler,
        T: Clone + Eq + Hash + Send + Sync + 'static,
    {
        let Self { inner, hasher } = self;
        // Check if the value exists in storage before attempting to insert it.
        let hash = hash_value::<T>(hasher, value);
        let mut read_len = {
            let inner = inner.read().expect(INCONSISTENT_STATE);
            if let Some(id) = inner.storage_id(hash, value) {
                return id;
            }
            inner.len()
        };
        // Clone before taking a write lock in case `T::clone` panics.
        let owned_value: Arc<dyn Any + Send + Sync + 'static> = Arc::new(value.clone());
        let id = loop {
            {
                let mut inner = inner.write().expect(INCONSISTENT_STATE);
                // Another thread may have stored an equal value since the
                // lookup. Values are only ever appended, so an unchanged length
                // means that storage did not change since the lookup. Calling
                // `T::eq` again could panic and poison the write lock, so abort
                // the write and retry from the beginning.
                if inner.values.len() == read_len {
                    break inner.store_value(hash, owned_value);
                }
            };
            let inner = inner.read().expect(INCONSISTENT_STATE);
            if let Some(id) = inner.storage_id(hash, value) {
                return id;
            }
            read_len = inner.values.len();
        };
        // User-defined handler implementations may panic, so emit the event
        // after the write lock is dropped.
        handler.event(Event::new(EventKind::StoreValue));
        id
    }

    fn get(&self, id: Self::Id) -> Self::Handle {
        let Self { inner, hasher: _ } = self;
        Arc::clone(inner.read().expect(INCONSISTENT_STATE).handle(id))
    }

    fn transfer(&self) -> Self::Transfer<'_> {
        Transfer {
            curr: self.inner.read().expect(INCONSISTENT_STATE),
            hasher: self.hasher.clone(),
            next: StorageInner::default(),
        }
    }
}

impl TransferTrait for Transfer<'_> {
    type Storage = Storage;

    fn transfer<T>(
        &mut self,
        id: <Self::Storage as StorageTrait>::Id,
    ) -> <Self::Storage as StorageTrait>::Id
    where
        T: Clone + Eq + Hash + Send + Sync + 'static,
    {
        let shared = self.curr.handle(id);
        let Some(value) = shared.downcast_ref::<T>() else {
            panic_expected_different_type::<T>()
        };
        let hash = hash_value::<T>(&self.hasher, value);
        if let Some(id) = self.next.storage_id(hash, value) {
            return id;
        }
        self.next.store_value(hash, Arc::clone(shared))
    }

    fn into_storage<H>(self, handler: &H) -> Self::Storage
    where
        H: Handler,
    {
        let freed_values = self
            .curr
            .values
            .len()
            .saturating_sub(self.next.values.len());
        handler.event(Event::new(EventKind::FreeValues(freed_values)));
        Storage {
            hasher: self.hasher,
            inner: RwLock::new(self.next),
        }
    }
}

impl StorageInner {
    fn store_value(&mut self, hash: u64, value: Arc<dyn Any + Send + Sync + 'static>) -> StorageId {
        let id = StorageId::new(self.values.len());
        self.values.push(value);
        self.index.insert_unique(hash, (hash, id), index_entry_hash);
        id
    }

    fn storage_id<T>(&self, hash: u64, value: &T) -> Option<StorageId>
    where
        T: Eq + 'static,
    {
        self.index
            .find(hash, |(entry_hash, entry_id)| {
                hash == *entry_hash && eq_value::<T>(value, &**self.handle(*entry_id))
            })
            .map(|(_, id)| *id)
    }

    fn handle(&self, id: StorageId) -> &Arc<dyn Any + Send + Sync + 'static> {
        self.values
            .get(id.idx())
            .expect("bug: unknown storage ID (was this ID created by another database?)")
    }

    fn len(&self) -> usize {
        self.values.len()
    }
}

fn index_entry_hash((hash, _): &(u64, StorageId)) -> u64 {
    *hash
}

fn eq_value<T>(v1: &T, v2: &dyn Any) -> bool
where
    T: PartialEq + 'static,
{
    v2.downcast_ref() == Some(v1)
}

fn hash_value<T>(build_hasher: &RandomState, value: &T) -> u64
where
    T: Hash + 'static,
{
    let mut hasher = build_hasher.build_hasher();
    TypeId::of::<T>().hash(&mut hasher);
    value.hash(&mut hasher);
    hasher.finish()
}

impl StorageId {
    const fn new(idx: usize) -> Self {
        Self(idx)
    }

    const fn idx(self) -> usize {
        self.0
    }
}
