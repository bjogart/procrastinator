//! Where a database keeps query arguments, query outputs, and input values.
//!
//! A [`Db`](crate::Db) does not hold values directly. It hands them to its
//! [`Storage`], which returns a small [`Storage::Id`] for each value, and
//! works with those IDs from then on. Comparing IDs instead of values makes
//! memo lookups and change detection cheap.
//!
//! The storage is the first type parameter of [`Db`](crate::Db). The default,
//! [`default::Storage`], fits most programs, so most code only meets this
//! module in the bounds of [`Query::eval`](crate::query::Query::eval).
//! Implement [`Storage`] to control how values are kept, for example to
//! return a different kind of handle from queries.
//!
//! # Examples
//!
//! ```
//! use procrastinator::Db;
//! use procrastinator::Snapshot;
//! use procrastinator::handler::Handler;
//! use procrastinator::query::Query;
//! use procrastinator::storage::Storage;
//! use procrastinator::storage::default;
//!
//! struct Double;
//! impl Query for Double {
//!     type Args = u32;
//!     type Out = u32;
//!
//!     // Queries are generic over the storage, so they work with any `Db`.
//!     fn eval<S, H>(_snapshot: &Snapshot<S, H>, n: &Self::Args) -> Self::Out
//!     where
//!         S: Storage,
//!         H: Handler,
//!     {
//!         n * 2
//!     }
//! }
//!
//! // Spelling out the default storage, rather than relying on the default
//! // type parameter.
//! let db: Db<default::Storage> = Db::default();
//! assert_eq!(*db.snapshot().query::<Double>(&21), 42);
//! ```

use crate::handler::Handler;
use crate::panic_expected_different_type;
use alloc::sync::Arc;
use core::any::Any;
use core::fmt::Debug;
use core::hash::Hash;
use core::ops::Deref;

pub mod default;

/// A store of type-erased values, each identified by an ID.
///
/// The database stores every query argument, query output, and input value
/// with [`Storage::store`], and reads values back with [`Storage::get`].
/// During [`Db::gc`](crate::Db::gc), it copies the values it still needs into
/// a new storage with [`Storage::transfer`].
///
/// Most programs use [`default::Storage`]. Its source is a good starting
/// point for a custom implementation.
///
/// # Examples
///
/// ```
/// use procrastinator::storage::Handle;
/// use procrastinator::storage::Storage as _;
/// use procrastinator::storage::default;
///
/// let storage = default::Storage::default();
/// // `()` is a handler that ignores all events.
/// let a = storage.store(&(), &"hello".to_owned());
/// let b = storage.store(&(), &"hello".to_owned());
/// // Equal values share one ID.
/// assert_eq!(a, b);
/// assert_eq!(*Handle::downcast::<String>(storage.get(a)), "hello");
/// ```
///
/// # Implementing `Storage`
///
/// The database relies on IDs to tell values apart:
///
/// - Equal IDs must mean equal values. Otherwise, queries return outputs that
///   were memoized for different arguments.
/// - Equal values should get equal IDs. Otherwise, memoized outputs are not
///   found again, and every change to an output looks like a new value, so
///   queries are evaluated more often than needed.
///
/// All methods take `&self`, and [`Storage::store`] and [`Storage::get`] are
/// called from several threads at once, so implementations must synchronize
/// internally. To send a [`Snapshot`](crate::Snapshot) to another thread,
/// the storage must be [`Send`] and [`Sync`].
///
/// To keep [`perf::Handler`](crate::handler::perf::Handler) accurate, emit
/// [`EventKind::StoreValue`](crate::handler::EventKind::StoreValue) for each
/// value that is actually added, and
/// [`EventKind::FreeValues`](crate::handler::EventKind::FreeValues) from
/// [`Transfer::into_storage`].
///
/// # Panic safety
///
/// [`Storage::store`] and [`Transfer::transfer`] call user code: the
/// [`Clone`], [`Eq`], and [`Hash`] implementations of stored values, and
/// [`Handler::event`]. Any of these may panic. A storage must remain
/// consistent when that happens, which in practice means:
///
/// - Do not call user code while holding a lock that a panic would poison,
///   such as a write lock. The database treats a poisoned lock as a bug and
///   panics on every later access.
/// - Do not call user code halfway through a modification. Clone a value
///   before inserting it, and compare and hash it before or after, not
///   during.
/// - Emit handler events after all locks are released.
pub trait Storage: Default + 'static {
    /// Identifies a stored value.
    ///
    /// An ID is only meaningful for the storage that created it.
    type Id: Clone + Copy + Eq + Hash + Debug + Send + Sync;

    /// A shared reference to a stored value of unknown type.
    ///
    /// Returned by [`Storage::get`]. The database turns it into a typed
    /// handle with [`Handle::downcast`] and returns that from
    /// [`Snapshot::query`](crate::Snapshot::query).
    type Handle: Debug + Handle;

    /// Copies live values during [`Db::gc`](crate::Db::gc).
    ///
    /// Created by [`Storage::transfer`]. See [`Transfer`].
    type Transfer<'storage>: Transfer<Storage = Self>
    where
        Self: 'storage;

    /// Stores a value and returns its ID.
    ///
    /// If an equal value of the same type is already stored, this should
    /// return the existing ID instead of storing a copy. See the
    /// [trait documentation](Storage#implementing-storage) for the exact
    /// requirements.
    ///
    /// Emit [`EventKind::StoreValue`](crate::handler::EventKind::StoreValue)
    /// on `handler` if the value was added.
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::storage::Storage as _;
    /// use procrastinator::storage::default;
    ///
    /// let storage = default::Storage::default();
    /// let one = storage.store(&(), &1_u32);
    /// let two = storage.store(&(), &2_u32);
    /// assert_ne!(one, two);
    /// assert_eq!(one, storage.store(&(), &1_u32));
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the [`Clone`], [`Eq`], or [`Hash`] implementation of `T`
    /// panics, or if `handler` panics. The storage must be left consistent;
    /// see the [trait documentation](Storage#panic-safety).
    fn store<H, T>(&self, handler: &H, value: &T) -> Self::Id
    where
        H: Handler,
        T: Clone + Eq + Hash + Send + Sync + 'static;

    /// Returns a handle to the value with the given ID.
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::storage::Handle;
    /// use procrastinator::storage::Storage as _;
    /// use procrastinator::storage::default;
    ///
    /// let storage = default::Storage::default();
    /// let id = storage.store(&(), &'x');
    /// assert_eq!(*Handle::downcast::<char>(storage.get(id)), 'x');
    /// ```
    ///
    /// # Panics
    ///
    /// May panic if `id` was not returned by this storage.
    fn get(&self, id: Self::Id) -> Self::Handle;

    /// Starts copying values into a new, empty storage.
    ///
    /// The database calls this method during [`Db::gc`](crate::Db::gc),
    /// transfers every value it still needs with [`Transfer::transfer`], and
    /// then replaces this storage with the result of
    /// [`Transfer::into_storage`]. Values that were not transferred are
    /// dropped along with this storage.
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::storage::Handle;
    /// use procrastinator::storage::Storage as _;
    /// use procrastinator::storage::Transfer as _;
    /// use procrastinator::storage::default;
    ///
    /// let storage = default::Storage::default();
    /// let keep = storage.store(&(), &1_u32);
    /// storage.store(&(), &2_u32);
    ///
    /// let mut transfer = storage.transfer();
    /// // IDs are not preserved: use the returned ID with the new storage.
    /// let kept = transfer.transfer::<u32>(keep);
    /// let new_storage = transfer.into_storage(&());
    /// assert_eq!(*Handle::downcast::<u32>(new_storage.get(kept)), 1);
    /// ```
    fn transfer(&self) -> Self::Transfer<'_>;
}

/// Copies selected values from one storage into a new one.
///
/// Created by [`Storage::transfer`] at the start of
/// [`Db::gc`](crate::Db::gc). The old storage stays untouched until
/// [`Transfer::into_storage`] returns, so if garbage collection panics
/// halfway, the database keeps using the old storage as if nothing happened.
///
/// See [`Storage::transfer`] for an example.
pub trait Transfer {
    /// The type of storage that values are copied from and into.
    type Storage: Storage;

    /// Copies the value with the given ID into the new storage and returns its
    /// ID there.
    ///
    /// `id` is an ID in the old storage, and `T` is the type of the value it
    /// refers to. Transferring equal values should return equal IDs, as
    /// required for [`Storage::store`].
    ///
    /// # Panics
    ///
    /// May panic if `id` was not returned by the old storage, or if its value
    /// is not of type `T`. Also panics if the [`Eq`] or [`Hash`]
    /// implementation of `T` panics. In that case the old storage must be left
    /// unchanged.
    fn transfer<T>(&mut self, id: <Self::Storage as Storage>::Id) -> <Self::Storage as Storage>::Id
    where
        T: Clone + Eq + Hash + Send + Sync + 'static;

    /// Finishes the transfer and returns the new storage.
    ///
    /// Emit [`EventKind::FreeValues`](crate::handler::EventKind::FreeValues)
    /// on `handler` with the number of values that were not transferred.
    fn into_storage<H>(self, handler: &H) -> Self::Storage
    where
        H: Handler;
}

/// A reference to a stored value whose type is only known at runtime.
///
/// [`Storage::get`] returns an untyped handle, and the database converts it
/// with [`Handle::downcast`] to the typed handle that
/// [`Snapshot::query`](crate::Snapshot::query) returns. For
/// [`default::Storage`], the handle is an
/// <code>[Arc]&lt;dyn [Any] + [Send] + [Sync]&gt;</code>, and the typed handle
/// is an <code>[Arc]&lt;T&gt;</code>.
///
/// # Examples
///
/// ```
/// use procrastinator::storage::Handle;
/// use std::any::Any;
/// use std::sync::Arc;
///
/// let untyped: Arc<dyn Any + Send + Sync> = Arc::new(7_u8);
/// // `Arc` has an inherent `downcast` method too, so call the trait method
/// // explicitly.
/// let typed: Arc<u8> = Handle::downcast::<u8>(untyped);
/// assert_eq!(*typed, 7);
/// ```
pub trait Handle {
    /// A handle to a value of type `T`, which dereferences to the value.
    type TypedHandle<T>: Deref<Target = T>;

    /// Converts this handle into a handle to a value of type `T`.
    ///
    /// # Panics
    ///
    /// Panics if the value is not of type `T`.
    fn downcast<T>(self) -> Self::TypedHandle<T>
    where
        T: Send + Sync + 'static;
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
