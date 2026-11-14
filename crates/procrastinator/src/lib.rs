#![doc = include_str!("../README.md")]

extern crate alloc;

use crate::handler::Event;
use crate::handler::Handler;
use crate::handler::ScopeGuard;
use crate::handler::ScopedEvent;
use crate::handler::ScopedEventKind;
use crate::memo::MemoId;
use crate::memo::Memos;
use crate::query::Input;
use crate::query::InputId;
use crate::query::InputRegistry;
use crate::query::Query;
use crate::query_ops::QueryOpsRegistry;
use crate::shared::Shared;
use crate::storage::Handle;
use crate::storage::Storage;
use crate::storage::Transfer as _;
use crate::storage::default::Storage as DefaultStorage;
use crate::update::PendingCommit;
use crate::update::QueryUpdate;
use core::any::TypeId;
use core::any::type_name;
use core::cell::RefCell;
use core::mem;
use std::collections::HashSet;

pub mod handler;
pub(crate) mod memo;
pub mod query;
mod query_ops;
mod shared;
pub mod storage;
#[cfg(test)]
mod tests;
mod update;

pub(crate) const INCONSISTENT_STATE: &str = "bug: database in inconsistent state due to panic";

/// A database of inputs and memoized query results.
///
/// The `Db` owns every [`Input`] value and every memoized [`Query`] result.
/// It is the only place where inputs can be created ([`Db::new_input`]) and
/// changed ([`Db::set_input`]). Queries are not run on the `Db` directly, but
/// on a [`Snapshot`] taken with [`Db::snapshot`].
///
/// The type parameters select where values are kept and who is notified of
/// internal events:
///
/// - `S` is the [`Storage`] that holds argument and output values. The
///   default storage hands out query results as [`Arc`](alloc::sync::Arc)s.
/// - `H` is the [`Handler`] that receives events, for example for profiling.
///   The default, `()`, ignores all events.
///
/// # Examples
///
/// ```
/// use procrastinator::Db;
/// use procrastinator::Snapshot;
/// use procrastinator::handler::Handler;
/// use procrastinator::query::Input;
/// use procrastinator::query::InputId;
/// use procrastinator::query::Query;
/// use procrastinator::storage::Storage;
///
/// struct Width;
/// impl Input for Width {
///     type Value = u32;
/// }
///
/// struct Area;
/// impl Query for Area {
///     type Args = InputId<Width>;
///     type Out = u32;
///
///     fn eval<S, H>(snapshot: &Snapshot<S, H>, width: &Self::Args) -> Self::Out
///     where
///         S: Storage,
///         H: Handler,
///     {
///         let width = *snapshot.query::<Width>(width);
///         width * width
///     }
/// }
///
/// // The type annotation is needed to pick the default type parameters.
/// let mut db: Db = Db::default();
/// let width = db.new_input::<Width>(&3);
/// assert_eq!(*db.snapshot().query::<Area>(&width), 9);
///
/// db.set_input(width, &4);
/// assert_eq!(*db.snapshot().query::<Area>(&width), 16);
/// ```
///
/// # Blocking
///
/// [`Db::set_input`] and [`Db::gc`] wait until every [`Snapshot`] of this
/// database has been dropped. Calling either of them while the same thread
/// still holds a snapshot deadlocks.
#[derive(Debug)]
pub struct Db<S = DefaultStorage, H = ()>
where
    S: Storage,
{
    runtime: Shared<Runtime<S, H>>,
    inputs: InputRegistry<S>,
}

/// A read-only view of a [`Db`], used to run queries.
///
/// Snapshots are created with [`Db::snapshot`]. Creating one is cheap: it
/// shares the database rather than copying it. Every snapshot sees the same
/// inputs, and query results memoized through one snapshot are visible to
/// all others.
///
/// A snapshot can be sent to another thread, but not shared between threads.
/// Give each thread its own snapshot to run queries in parallel.
///
/// # Examples
///
/// ```
/// use procrastinator::Db;
/// use procrastinator::query::Input;
/// use std::thread;
///
/// struct Greeting;
/// impl Input for Greeting {
///     type Value = String;
/// }
///
/// let mut db: Db = Db::default();
/// let greeting = db.new_input::<Greeting>(&"hello".to_owned());
///
/// let workers: Vec<_> = (0..2)
///     .map(|_| {
///         let snapshot = db.snapshot();
///         thread::spawn(move || snapshot.query::<Greeting>(&greeting).len())
///     })
///     .collect();
/// for worker in workers {
///     assert_eq!(worker.join().unwrap(), 5);
/// }
///
/// // All snapshots have been dropped, so inputs can change again.
/// db.set_input(greeting, &"bye".to_owned());
/// ```
///
/// # Blocking
///
/// While any snapshot is alive, [`Db::set_input`] and [`Db::gc`] block. Drop
/// snapshots as soon as you are done with them, and never hold one on the
/// thread that mutates the `Db`.
#[derive(Debug)]
pub struct Snapshot<S = DefaultStorage, H = ()>
where
    S: Storage,
{
    runtime: Shared<Runtime<S, H>>,
    active_queries: RefCell<ActiveQueryStack<S>>,
}

#[derive(Debug)]
struct Runtime<S, H>
where
    S: Storage,
{
    rev: GlobalRevision,
    storage: S,
    memos: Memos<S>,
    query_ops: QueryOpsRegistry<S, H>,
    handler: H,
}

#[derive(Debug)]
struct GlobalRevision {
    rev: Revision,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Revision {
    raw: usize,
}

#[derive(Debug)]
struct ActiveQueryStack<S>(Vec<ActiveQuery<S>>)
where
    S: Storage;

#[derive(Debug)]
struct ActiveQuery<S>
where
    S: Storage,
{
    deps: Vec<MemoId<S>>,
}

/// An opaque value that uniquely identifies a query.
///
/// Within a single build, a query will always be assigned the same ID. IDs
/// identify the query type and implement `Eq` and `Hash`. However, IDs are not
/// guaranteed to be the same across builds.
///
/// # Examples
///
/// ```
/// use procrastinator::handler::Handler;
/// use procrastinator::query::Query;
/// use procrastinator::QueryId;
/// use procrastinator::Snapshot;
/// use procrastinator::storage::Storage;
///
/// struct Shout;
/// impl Query for Shout {
///     type Args = String;
///     type Out = String;
///
///     fn eval<S, H>(_snapshot: &Snapshot<S, H>, text: &Self::Args) -> Self::Out
///     where
///         S: Storage,
///         H: Handler,
///     {
///         text.to_uppercase()
///     }
/// }
///
/// struct Whisper;
/// impl Query for Whisper {
///     type Args = String;
///     type Out = String;
///
///     fn eval<S, H>(_snapshot: &Snapshot<S, H>, text: &Self::Args) -> Self::Out
///     where
///         S: Storage,
///         H: Handler,
///     {
///         text.to_lowercase()
///     }
/// }
///
/// assert_eq!(QueryId::of::<Shout>(), QueryId::of::<Shout>());
/// assert_eq!(QueryId::of::<Whisper>(), QueryId::of::<Whisper>());
/// // Query IDs of different queries compare unequal.
/// assert_ne!(QueryId::of::<Shout>(), QueryId::of::<Whisper>());
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct QueryId(TypeId);

impl<S, H> Db<S, H>
where
    S: Storage,
{
    /// Returns the event handler of this database.
    ///
    /// The handler receives an event whenever the database does something
    /// of note, like evaluating a query or storing a value. Use this method
    /// to read whatever the handler has collected.
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::Db;
    /// use procrastinator::handler::perf;
    /// use procrastinator::query::Input;
    /// use procrastinator::storage::default::Storage;
    ///
    /// struct Width;
    /// impl Input for Width {
    ///     type Value = u32;
    /// }
    ///
    /// let mut db: Db<Storage, perf::Handler> = Db::default();
    /// let width = db.new_input::<Width>(&3);
    /// db.snapshot().query::<Width>(&width);
    /// assert_eq!(db.event_handler().query_count(), 1);
    /// ```
    #[must_use]
    pub fn event_handler(&self) -> &H {
        &self.runtime.handler
    }

    /// Creates a new input with the given initial value.
    ///
    /// The returned [`InputId`] identifies the newly created input. Pass it to
    /// [`Snapshot::query`] to read the value, or to [`Db::set_input`] to change
    /// it. An `InputId` is only meaningful for the `Db` that created it;
    /// passing one to a different `Db` constitutes a logic error and will most
    /// likely panic.
    ///
    /// Unlike [`Db::set_input`], this method does not wait for snapshots to
    /// drop, since no running query can depend on an input that was created
    /// after it was called.
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::Db;
    /// use procrastinator::query::Input;
    ///
    /// struct Width;
    /// impl Input for Width {
    ///     type Value = u32;
    /// }
    ///
    /// let mut db: Db = Db::default();
    /// let small = db.new_input::<Width>(&3);
    /// let large = db.new_input::<Width>(&300);
    ///
    /// let snapshot = db.snapshot();
    /// assert_eq!(*snapshot.query::<Width>(&small), 3);
    /// assert_eq!(*snapshot.query::<Width>(&large), 300);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the [`Clone`], [`Eq`], or [`Hash`] implementation of the
    /// value, or the event handler panics. The panic propagates to the caller,
    /// and the database stays usable.
    pub fn new_input<I>(&mut self, value: &I::Value) -> InputId<I>
    where
        H: Handler,
        I: Input,
    {
        let rev = self.runtime.rev.get();
        let query_id = QueryId::of::<I>();
        self.runtime
            .query_ops
            .query_id::<I>(&self.runtime.handler, query_id);
        let value_id = self.runtime.storage.store(&self.runtime.handler, value);
        self.inputs.new_input(|input_id| {
            let dummy_args_id = self.runtime.storage.store(&self.runtime.handler, &input_id);
            self.runtime.memos.new_input(
                &self.runtime.handler,
                rev,
                query_id,
                dummy_args_id,
                value_id,
            )
        })
    }

    /// Replaces the value of an input with another value.
    ///
    /// Queries that depend on the input, directly or through other queries,
    /// are re-evaluated the next time they are called. Nothing is evaluated
    /// right away. If the new value is equal to the current one, all memoized
    /// results stay valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::Db;
    /// use procrastinator::query::Input;
    ///
    /// struct Width;
    /// impl Input for Width {
    ///     type Value = u32;
    /// }
    ///
    /// let mut db: Db = Db::default();
    /// let width = db.new_input::<Width>(&3);
    /// assert_eq!(*db.snapshot().query::<Width>(&width), 3);
    ///
    /// db.set_input(width, &4);
    /// assert_eq!(*db.snapshot().query::<Width>(&width), 4);
    /// ```
    ///
    /// # Blocking
    ///
    /// This method blocks until every [`Snapshot`] of this database has been
    /// dropped. Calling it while the current thread holds a snapshot
    /// deadlocks.
    ///
    /// # Panics
    ///
    /// Panics if the [`Clone`], [`Eq`], or [`Hash`] implementation of the
    /// value, or the event handler panics. The panic propagates to the
    /// caller, and the database stays usable: the input keeps its previous
    /// value.
    ///
    /// Passing an `input_id` that was created by a different `Db` is a logic
    /// error. It may panic now, panic later, or silently change an unrelated
    /// input.
    pub fn set_input<I>(&mut self, input_id: InputId<I>, value: &I::Value)
    where
        H: Handler,
        I: Input,
    {
        let runtime = self.runtime.wait_exclusive();
        let memo_id = self.inputs.memo_id(input_id);
        let new_value_id = runtime.storage.store(&runtime.handler, value);
        if runtime.memos.value_id(memo_id) != Some(new_value_id) {
            let current_rev = runtime.rev.bump();
            runtime.memos.set_input(memo_id, current_rev, new_value_id);
        }
    }

    /// Creates a [`Snapshot`] of this database, from which queries can be run.
    ///
    /// This is a cheap, constant-time operation. The snapshot shares the
    /// database rather than copying it.
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::Db;
    /// use procrastinator::query::Input;
    ///
    /// struct Width;
    /// impl Input for Width {
    ///     type Value = u32;
    /// }
    ///
    /// let mut db: Db = Db::default();
    /// let width = db.new_input::<Width>(&3);
    ///
    /// let snapshot = db.snapshot();
    /// assert_eq!(*snapshot.query::<Width>(&width), 3);
    /// // Drop the snapshot before changing inputs, or `set_input` deadlocks.
    /// drop(snapshot);
    ///
    /// db.set_input(width, &4);
    /// ```
    ///
    /// # Blocking
    ///
    /// [`Db::set_input`] and [`Db::gc`] block until every snapshot has been
    /// dropped. Keep snapshots short-lived, and do not hold one on the thread
    /// that mutates the `Db`.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot<S, H> {
        Snapshot::new(self.runtime.clone())
    }

    /// Frees all memoized query results.
    ///
    /// Inputs are kept, along with their values. Every other query is
    /// re-evaluated the next time it is called. Values that are no longer
    /// referenced are released, so a long-running process can call this
    /// method now and then to bound its memory use.
    ///
    /// # Blocking
    ///
    /// This method blocks until every [`Snapshot`] of this database has been
    /// dropped. Calling it while the current thread holds a snapshot
    /// deadlocks.
    ///
    /// # Panics
    ///
    /// Panics if the [`Eq`] or [`Hash`] implementation of a stored value, the
    /// [`Drop`] implementation of a freed value, or the event handler panics.
    /// The panic propagates to the caller, and the database stays usable.
    pub fn gc(&mut self)
    where
        H: Handler,
    {
        self.runtime.wait_exclusive().gc(&mut self.inputs);
    }
}

impl<S, H> Default for Db<S, H>
where
    S: Storage,
    H: Default,
{
    fn default() -> Self {
        Self {
            runtime: Shared::default(),
            inputs: InputRegistry::default(),
        }
    }
}

impl<S, H> Snapshot<S, H>
where
    S: Storage,
{
    fn new(runtime: Shared<Runtime<S, H>>) -> Self {
        Self {
            runtime,
            active_queries: RefCell::default(),
        }
    }

    /// Returns the event handler of the database this snapshot was taken
    /// from.
    ///
    /// This is the same handler as [`Db::event_handler`] returns. It is
    /// available here so that threads holding only a snapshot can read it.
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::Db;
    /// use procrastinator::handler::perf;
    /// use procrastinator::query::Input;
    /// use procrastinator::storage::default::Storage;
    ///
    /// struct Width;
    /// impl Input for Width {
    ///     type Value = u32;
    /// }
    ///
    /// let mut db: Db<Storage, perf::Handler> = Db::default();
    /// let width = db.new_input::<Width>(&3);
    /// let snapshot = db.snapshot();
    /// snapshot.query::<Width>(&width);
    /// snapshot.query::<Width>(&width);
    /// assert_eq!(snapshot.event_handler().query_count(), 2);
    /// ```
    #[must_use]
    pub fn event_handler(&self) -> &H {
        &self.runtime.handler
    }

    /// Runs query `Q` with the given arguments and returns a handle to its
    /// output.
    ///
    /// If the query was called with equal arguments before and none of its
    /// dependencies changed since, the memoized output is returned without
    /// evaluating [`Query::eval`]. Otherwise the query is evaluated and the
    /// output is memoized for later calls. Inputs are queries too, so this is
    /// also how an [`Input`] value is read.
    ///
    /// When called from inside [`Query::eval`], the called query is recorded as
    /// a dependency of the calling query.
    ///
    /// The returned handle dereferences to `Q::Out`. With the default
    /// [`Storage`], the handle is an [`Arc`](alloc::sync::Arc).
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::Db;
    /// use procrastinator::Snapshot;
    /// use procrastinator::handler::Handler;
    /// use procrastinator::query::Input;
    /// use procrastinator::query::InputId;
    /// use procrastinator::query::Query;
    /// use procrastinator::storage::Storage;
    ///
    /// struct Name;
    /// impl Input for Name {
    ///     type Value = String;
    /// }
    ///
    /// struct Greet;
    /// impl Query for Greet {
    ///     type Args = InputId<Name>;
    ///     type Out = String;
    ///
    ///     fn eval<S, H>(snapshot: &Snapshot<S, H>, name: &Self::Args) -> Self::Out
    ///     where
    ///         S: Storage,
    ///         H: Handler,
    ///     {
    ///         // `Name` becomes a dependency of `Greet`. Inside `eval`, the
    ///         // storage is generic, so the handle is only known to implement
    ///         // `Deref`. Dereference it to get at the value.
    ///         let name = snapshot.query::<Name>(name);
    ///         format!("Hello, {}!", *name)
    ///     }
    /// }
    ///
    /// let mut db: Db = Db::default();
    /// let name = db.new_input::<Name>(&"Ferris".to_owned());
    /// assert_eq!(*db.snapshot().query::<Greet>(&name), "Hello, Ferris!");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if [`Query::eval`], the [`Clone`], [`Eq`], or [`Hash`]
    /// implementation of an argument or output, or the event handler panics.
    /// The panic propagates to the caller, and the database stays usable:
    /// nothing is memoized for the failed call, so the next call evaluates the
    /// query again.
    ///
    /// Passing an [`InputId`] that was created by a different `Db` is a logic
    /// error. It may panic, or return the value of an unrelated input.
    pub fn query<Q>(&self, args: &Q::Args) -> <S::Handle as Handle>::TypedHandle<Q::Out>
    where
        H: Handler,
        Q: Query,
    {
        let query_id = QueryId::of::<Q>();
        let _query_guard = ScopeGuard::new(
            &self.runtime.handler,
            ScopedEvent::new(ScopedEventKind::Query),
        );
        self.runtime
            .query_ops
            .query_id::<Q>(&self.runtime.handler, query_id);
        let args_id = self.runtime.storage.store(&self.runtime.handler, args);
        let memo_id = self
            .runtime
            .memos
            .memo_id(&self.runtime.handler, query_id, args_id);
        self.track_dep(memo_id);
        self.verify_memo(self.runtime.rev.get(), memo_id);
        let value_id = self.memoized_value_id(memo_id);
        Handle::downcast::<Q::Out>(self.runtime.storage.get(value_id))
    }

    fn verify_memo(&self, current_rev: Revision, memo_id: MemoId<S>)
    where
        H: Handler,
    {
        let (last_verified, deps) = {
            let (last_verified, deps) = self.runtime.memos.verification(memo_id);
            // If this rev is up to date, skip it.
            if last_verified == current_rev {
                return;
            }
            (last_verified, deps)
        };
        let deps_postdate_memo = deps
            .into_iter()
            .any(|dep| self.dep_postdates_rev(current_rev, last_verified, dep));
        if !deps_postdate_memo && last_verified > Revision::NEVER_VERIFIED {
            self.runtime.memos.mark_verified(memo_id, current_rev);
            return;
        }
        self.eval_memo(current_rev, memo_id)
    }

    fn eval_memo(&self, current_rev: Revision, memo_id: MemoId<S>)
    where
        H: Handler,
    {
        let MemoId { query_id, args_id } = memo_id;
        let ops = self
            .runtime
            .query_ops
            .get(query_id)
            .expect("bug: query not registered");
        let args = self.runtime.storage.get(args_id);
        let mut query_update = self.install_query();
        let out = {
            let _eval_guard = ScopeGuard::new(
                &self.runtime.handler,
                ScopedEvent::new(ScopedEventKind::Eval),
            );
            (ops.eval)(self, args)
        };
        let out = (ops.store_output)(&self.runtime.storage, &self.runtime.handler, out.as_ref());
        let mut commit = PendingCommit::new(current_rev, memo_id);
        if self.runtime.memos.value_id(memo_id) != Some(out) {
            commit.new_value_id = Some(out);
        }
        query_update.commit = Some(commit);
    }

    fn dep_postdates_rev(
        &self,
        current_rev: Revision,
        memo_last_verified: Revision,
        dep: MemoId<S>,
    ) -> bool
    where
        H: Handler,
    {
        self.verify_memo(current_rev, dep);
        self.runtime.memos.last_changed(dep) > memo_last_verified
    }

    fn install_query(&self) -> QueryUpdate<'_, S, H>
    where
        H: Handler,
    {
        let mut active_queries = self.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        active_queries.push(ActiveQuery::default());
        QueryUpdate::new(self)
    }

    fn track_dep(&self, dep: MemoId<S>) {
        let mut active_queries = self.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        let caller = active_queries.last_mut();
        if let Some(ActiveQuery { deps }) = caller {
            deps.push(dep);
        }
    }

    fn memoized_value_id(&self, memo_id: MemoId<S>) -> S::Id {
        self.runtime
            .memos
            .value_id(memo_id)
            .expect("bug: memo entry has no stored value (value not yet computed or memoized)")
    }
}

impl<S, H> Runtime<S, H>
where
    S: Storage,
{
    fn gc(&mut self, inputs: &mut InputRegistry<S>)
    where
        H: Handler,
    {
        let Self {
            storage,
            memos,
            query_ops,
            handler,
            rev: _,
        } = self;
        let input_count = inputs.len();
        let mut storage_transfer = storage.transfer();
        let mut memos_transfer = memos.transfer(input_count);
        let mut input_queries = HashSet::with_capacity(input_count);
        let next_inputs = inputs.transfer(|input| {
            let memo_id = input.memo_id();
            input_queries.insert(memo_id.query_id);
            memos_transfer.retain(memo_id, |value_id| {
                input.transfer(&mut storage_transfer, value_id)
            })
        });
        let next_memos = memos_transfer.into_memos();
        let next_storage = storage_transfer.into_storage(handler);

        let prev_memos = {
            // Replace everything first before dropping collected values, since
            // Drop implementations can panic.
            let _prev_inputs = mem::replace(inputs, next_inputs);
            let _prev_storage = mem::replace(storage, next_storage);
            mem::replace(memos, next_memos)
        };

        handler.event(Event::new(handler::EventKind::DeregisterMemos(
            prev_memos.len().saturating_sub(memos.len()),
        )));
        query_ops.retain(handler, |query_id| input_queries.contains(&query_id));
    }
}

impl<S, H> Default for Runtime<S, H>
where
    S: Storage,
    H: Default,
{
    fn default() -> Self {
        Self {
            rev: GlobalRevision::default(),
            storage: S::default(),
            memos: Memos::default(),
            query_ops: QueryOpsRegistry::default(),
            handler: H::default(),
        }
    }
}

impl<S> Default for ActiveQueryStack<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl<S> Default for ActiveQuery<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self {
            deps: Vec::default(),
        }
    }
}

impl QueryId {
    /// Construct a new instance of the ID for a statically-known query.
    ///
    /// This is a constant-time operation.
    #[must_use]
    pub fn of<Q>() -> Self
    where
        Q: Query,
    {
        Self(TypeId::of::<Q>())
    }
}

impl GlobalRevision {
    fn bump(&mut self) -> Revision {
        let Self { rev } = self;
        *rev = rev.incr();
        self.get()
    }

    fn get(&self) -> Revision {
        let Self { rev } = self;
        *rev
    }
}

impl Default for GlobalRevision {
    fn default() -> Self {
        Self {
            rev: Revision::NEVER_VERIFIED.incr(),
        }
    }
}

impl Revision {
    const NEVER_VERIFIED: Self = Self { raw: 0 };

    fn incr(self) -> Self {
        match self.raw.checked_add(1) {
            None => panic!("revision overflowed"),
            Some(raw) => Self { raw },
        }
    }
}

pub(crate) fn panic_expected_different_type<T>() -> ! {
    panic!(
        "bug (type mismatch): expected `{}` but found different type (possible database mix-up)",
        type_name::<T>()
    )
}
