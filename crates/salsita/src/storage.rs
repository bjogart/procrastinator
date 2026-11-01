use crate::INCONSISTENT_STATE;
use crate::event;
use crate::event::Event;
use crate::event::EventKind;
use crate::panic_expected_different_type;
use alloc::sync::Arc;
use core::any::Any;
use core::any::TypeId;
use core::fmt::Debug;
use core::hash::BuildHasher as _;
use core::hash::Hash;
use core::hash::Hasher as _;
use core::ops::Deref;
use hashbrown::HashTable;
use std::hash::RandomState;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;

pub trait Storage: Default + 'static {
    type Id: Clone + Copy + Eq + Hash + Debug + Send + Sync;
    type Handle: Debug + Handle;
    type Transfer<'storage>: Transfer<Storage = Self>
    where
        Self: 'storage;

    fn store<H, T>(&self, handler: &H, value: &T) -> Self::Id
    where
        H: event::Handler,
        T: Clone + Eq + Hash + Send + Sync + 'static;

    fn get(&self, id: Self::Id) -> Self::Handle;

    fn transfer(&self) -> Self::Transfer<'_>;
}

pub trait Transfer {
    type Storage: Storage;

    fn transfer<T>(&mut self, id: <Self::Storage as Storage>::Id) -> <Self::Storage as Storage>::Id
    where
        T: Clone + Eq + Hash + Send + Sync + 'static;

    fn into_storage<H>(self, handler: &H) -> Self::Storage
    where
        H: event::Handler;
}

pub trait Handle {
    type TypedHandle<T>: Deref<Target = T>;

    fn downcast<T>(self) -> Self::TypedHandle<T>
    where
        T: Send + Sync + 'static;
}

#[derive(Debug, Default)]
pub struct DefaultStorage {
    hasher: RandomState,
    inner: RwLock<DefaultStorageInner>,
}

#[derive(Debug)]
pub struct DefaultTransfer<'storage> {
    curr: RwLockReadGuard<'storage, DefaultStorageInner>,
    hasher: RandomState,
    next: DefaultStorageInner,
}

#[derive(Debug, Default)]
struct DefaultStorageInner {
    index: HashTable<(u64, DefaultStorageId)>,
    values: Vec<Arc<dyn Any + Send + Sync + 'static>>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DefaultStorageId(usize);

impl Storage for DefaultStorage {
    type Id = DefaultStorageId;

    type Handle = Arc<dyn Any + Send + Sync + 'static>;

    type Transfer<'storage> = DefaultTransfer<'storage>;

    fn store<H, T>(&self, handler: &H, value: &T) -> Self::Id
    where
        H: event::Handler,
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
        DefaultTransfer {
            curr: self.inner.read().expect(INCONSISTENT_STATE),
            hasher: self.hasher.clone(),
            next: DefaultStorageInner::default(),
        }
    }
}

impl Transfer for DefaultTransfer<'_> {
    type Storage = DefaultStorage;

    fn transfer<T>(&mut self, id: <Self::Storage as Storage>::Id) -> <Self::Storage as Storage>::Id
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
        H: event::Handler,
    {
        let freed_values = self
            .curr
            .values
            .len()
            .saturating_sub(self.next.values.len());
        handler.event(Event::new(EventKind::FreeValues(freed_values)));
        DefaultStorage {
            hasher: self.hasher,
            inner: RwLock::new(self.next),
        }
    }
}

impl DefaultStorageInner {
    fn store_value(
        &mut self,
        hash: u64,
        value: Arc<dyn Any + Send + Sync + 'static>,
    ) -> DefaultStorageId {
        let id = DefaultStorageId::new(self.values.len());
        self.values.push(value);
        self.index.insert_unique(hash, (hash, id), index_entry_hash);
        id
    }

    fn storage_id<T>(&self, hash: u64, value: &T) -> Option<DefaultStorageId>
    where
        T: Eq + 'static,
    {
        self.index
            .find(hash, |(entry_hash, entry_id)| {
                hash == *entry_hash && eq_value::<T>(value, &**self.handle(*entry_id))
            })
            .map(|(_, id)| *id)
    }

    fn handle(&self, id: DefaultStorageId) -> &Arc<dyn Any + Send + Sync + 'static> {
        self.values
            .get(id.idx())
            .expect("bug: unknown storage ID (was this ID created by another database?)")
    }

    fn len(&self) -> usize {
        self.values.len()
    }
}

fn index_entry_hash((hash, _): &(u64, DefaultStorageId)) -> u64 {
    *hash
}

fn eq_value<T>(a: &T, b: &dyn Any) -> bool
where
    T: PartialEq + 'static,
{
    b.downcast_ref() == Some(a)
}

fn hash_value<T>(build_hasher: &RandomState, v: &T) -> u64
where
    T: Hash + 'static,
{
    let mut hasher = build_hasher.build_hasher();
    TypeId::of::<T>().hash(&mut hasher);
    v.hash(&mut hasher);
    hasher.finish()
}

impl Handle for Arc<dyn Any + Send + Sync + 'static> {
    type TypedHandle<T> = Arc<T>;

    fn downcast<T>(self) -> Self::TypedHandle<T>
    where
        T: Send + Sync + 'static,
    {
        Arc::downcast(self).unwrap_or_else(|_| panic_expected_different_type::<T>())
    }
}

impl DefaultStorageId {
    const fn new(idx: usize) -> Self {
        Self(idx)
    }

    const fn idx(self) -> usize {
        self.0
    }
}
