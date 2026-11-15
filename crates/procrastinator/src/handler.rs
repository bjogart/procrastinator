//! Hooks for observing what a database does internally.
//!
//! A [`Db`](crate::Db) notifies its [`Handler`] whenever something of note
//! happens: a query is called or evaluated, a value is stored, a memo is
//! created, or [`Db::gc`](crate::Db::gc) frees memory. Handlers are useful for
//! profiling, logging, and tests that check how much work was done.
//!
//! The handler is the second type parameter of [`Db`](crate::Db). The default,
//! `()`, ignores all events. The [`perf`] module provides a handler that
//! collects performance metrics, and you can write your own by implementing
//! [`Handler`].
//!
//! # Examples
//!
//! ```
//! use procrastinator::Db;
//! use procrastinator::handler::perf;
//! use procrastinator::query::Input;
//! use procrastinator::storage::default::Storage;
//!
//! struct Width;
//! impl Input for Width {
//!     type Value = u32;
//! }
//!
//! let mut db: Db<Storage, perf::Handler> = Db::default();
//! let width = db.new_input::<Width>(&3);
//! db.snapshot().query::<Width>(&width);
//! assert_eq!(db.event_handler().query_count(), 1);
//! ```

use crate::MemoId;
use crate::QueryId;
use crate::storage::Storage;

pub mod perf;

const VALUE_ALREADY_TAKEN: &str = "bug: guard payload already taken";

/// Receives events from a database.
///
/// A [`Db`](crate::Db) calls its handler at points of interest during its work.
/// There are two kinds of events:
///
/// - An [`Event`] marks something that happened at a single point in time, like
///   storing a value. It is passed to [`Handler::event`].
/// - A [`ScopedEvent`] marks a span of time, like the evaluation of a query.
///   The database calls [`Handler::enter_scope`] when the span starts and
///   [`Handler::exit_scope`] when it ends. Scopes nest: a query that calls
///   another query is still in its own scope while the inner one runs.
///
/// Handlers that track nested scopes can keep state in [`Handler::Local`]. The
/// [`Db`](crate::Db) and every [`Snapshot`](crate::Snapshot) own one value of
/// that type each, and pass it to the scope methods.
///
/// The database creates its handler with [`Default`], and you can read it back
/// with [`Db::event_handler`](crate::Db::event_handler) or
/// [`Snapshot::event_handler`](crate::Snapshot::event_handler).
///
/// All methods take `&self`, and with multiple snapshots they may be called
/// from several threads at once. Use atomics or locks to record state. To send
/// a [`Snapshot`](crate::Snapshot) to another thread, the handler must be
/// [`Send`] and [`Sync`].
///
/// # Examples
///
/// A handler that counts query calls:
///
/// ```
/// use procrastinator::Db;
/// use procrastinator::handler::Event;
/// use procrastinator::handler::Handler;
/// use procrastinator::handler::ScopedEvent;
/// use procrastinator::handler::ScopedEventKind;
/// use procrastinator::query::Input;
/// use procrastinator::storage::default;
/// use procrastinator::storage::Storage;
/// use std::sync::atomic::AtomicUsize;
/// use std::sync::atomic::Ordering;
///
/// #[derive(Default)]
/// struct QueryCounter {
///     queries: AtomicUsize,
/// }
///
/// impl<S> Handler<S> for QueryCounter
/// where
///     S: Storage,
/// {
///     type Local = ();
///     type Payload = ();
///
///     fn event(&self, _event: Event<S>) {}
///
///     fn enter_scope(&self, (): &Self::Local, event: ScopedEvent<S>) -> Self::Payload {
///         if let ScopedEventKind::Query(_) = event.kind {
///             self.queries.fetch_add(1, Ordering::Relaxed);
///         }
///     }
///
///     fn exit_scope(&self, (): &Self::Local, (): Self::Payload) {}
/// }
///
/// struct Width;
/// impl Input for Width {
///     type Value = u32;
/// }
///
/// let mut db: Db<default::Storage, QueryCounter> = Db::default();
/// let width = db.new_input::<Width>(&3);
/// let snapshot = db.snapshot();
/// snapshot.query::<Width>(&width);
/// snapshot.query::<Width>(&width);
/// assert_eq!(snapshot.event_handler().queries.load(Ordering::Relaxed), 2);
/// ```
///
/// # Panics
///
/// A panic in a handler method propagates out of the database operation that
/// triggered the event. Note that [`Handler::exit_scope`] also runs while a
/// panic in a query unwinds, and panicking there aborts the process.
pub trait Handler<S>
where
    Self: Sized + Default,
    S: Storage,
{
    /// State shared by the scopes of one [`Db`](crate::Db) or one
    /// [`Snapshot`](crate::Snapshot).
    ///
    /// The database and each of its snapshots create one value of this type
    /// with [`Default`], and pass it to every [`Handler::enter_scope`] and
    /// [`Handler::exit_scope`] call they make. Scopes that share a value are
    /// properly nested and never run at the same time, so the value needs no
    /// synchronization: a [`Cell`](core::cell::Cell) is enough to mutate it. A
    /// handler can use it to pass information between a scope and the scopes
    /// nested inside it, for example to subtract the time spent in nested
    /// scopes from the time of the outer scope.
    ///
    /// The value lives as long as its owner, and it moves along when a snapshot
    /// is sent to another thread. To send a [`Snapshot`](crate::Snapshot) to
    /// another thread, this type must be [`Send`]. Use `()` if no state is
    /// needed.
    ///
    /// # Examples
    ///
    /// A handler that records how deeply query calls were nested at most:
    ///
    /// ```
    /// use procrastinator::Db;
    /// use procrastinator::Snapshot;
    /// use procrastinator::handler::Event;
    /// use procrastinator::handler::Handler;
    /// use procrastinator::handler::ScopedEvent;
    /// use procrastinator::handler::ScopedEventKind;
    /// use procrastinator::query::Input;
    /// use procrastinator::query::InputId;
    /// use procrastinator::query::Query;
    /// use procrastinator::storage::Storage;
    /// use procrastinator::storage::default;
    /// use std::cell::Cell;
    /// use std::sync::atomic::AtomicUsize;
    /// use std::sync::atomic::Ordering;
    ///
    /// #[derive(Default)]
    /// struct MaxDepth {
    ///     max: AtomicUsize,
    /// }
    ///
    /// impl<S> Handler<S> for MaxDepth
    /// where
    ///     S: Storage,
    /// {
    ///     // The number of query calls that are currently in progress.
    ///     type Local = Cell<usize>;
    ///     // Whether the scope is a query call.
    ///     type Payload = bool;
    ///
    ///     fn event(&self, _event: Event<S>) {}
    ///
    ///     fn enter_scope(&self, depth: &Self::Local, event: ScopedEvent<S>) -> Self::Payload {
    ///         let is_query = matches!(event.kind, ScopedEventKind::Query(_));
    ///         if is_query {
    ///             depth.set(depth.get() + 1);
    ///             self.max.fetch_max(depth.get(), Ordering::Relaxed);
    ///         }
    ///         is_query
    ///     }
    ///
    ///     fn exit_scope(&self, depth: &Self::Local, is_query: Self::Payload) {
    ///         if is_query {
    ///             depth.set(depth.get() - 1);
    ///         }
    ///     }
    /// }
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
    ///         H: Handler<S>,
    ///     {
    ///         let width = *snapshot.query::<Width>(width);
    ///         width * width
    ///     }
    /// }
    ///
    /// let mut db: Db<default::Storage, MaxDepth> = Db::default();
    /// let width = db.new_input::<Width>(&3);
    /// let snapshot = db.snapshot();
    /// snapshot.query::<Area>(&width);
    /// // `Area` called `Width`.
    /// assert_eq!(snapshot.event_handler().max.load(Ordering::Relaxed), 2);
    /// ```
    type Local: Default;

    /// Data carried from the start of a scope to its end.
    ///
    /// The value returned by [`Handler::enter_scope`] is passed to the matching
    /// [`Handler::exit_scope`] call. For example, a profiler can use the start
    /// time of the scope as its payload. Use `()` if no data is needed.
    type Payload;

    /// Called when an [`Event`] happens.
    ///
    /// Inspect [`Event::kind`] to see what happened.
    ///
    /// # Examples
    ///
    /// A handler that tracks how many memos exist:
    ///
    /// ```
    /// use procrastinator::Db;
    /// use procrastinator::handler::Event;
    /// use procrastinator::handler::EventKind;
    /// use procrastinator::handler::Handler;
    /// use procrastinator::handler::ScopedEvent;
    /// use procrastinator::query::Input;
    /// use procrastinator::storage::default;
    /// use procrastinator::storage::Storage;
    /// use std::sync::atomic::AtomicUsize;
    /// use std::sync::atomic::Ordering;
    ///
    /// #[derive(Default)]
    /// struct MemoCounter {
    ///     memos: AtomicUsize,
    /// }
    ///
    /// impl<S> Handler<S> for MemoCounter
    /// where
    ///     S: Storage,
    /// {
    ///     type Local = ();
    ///     type Payload = ();
    ///
    ///     fn event(&self, event: Event<S>) {
    ///         match event.kind {
    ///             EventKind::RegisterMemo(_) => {
    ///                 self.memos.fetch_add(1, Ordering::Relaxed);
    ///             }
    ///             EventKind::DeregisterMemo(_) => {
    ///                 self.memos.fetch_sub(1, Ordering::Relaxed);
    ///             }
    ///             _ => {}
    ///         }
    ///     }
    ///
    ///     fn enter_scope(&self, (): &Self::Local, _event: ScopedEvent<S>) -> Self::Payload {}
    ///
    ///     fn exit_scope(&self, (): &Self::Local, (): Self::Payload) {}
    /// }
    ///
    /// struct Width;
    /// impl Input for Width {
    ///     type Value = u32;
    /// }
    ///
    /// let mut db: Db<default::Storage, MemoCounter> = Db::default();
    /// // Every input has a memo that holds its value.
    /// db.new_input::<Width>(&3);
    /// db.new_input::<Width>(&4);
    /// assert_eq!(db.event_handler().memos.load(Ordering::Relaxed), 2);
    /// ```
    fn event(&self, event: Event<S>);

    /// Called when a [`ScopedEvent`] starts.
    ///
    /// The returned payload is passed to the [`Handler::exit_scope`] call that
    /// ends this scope. Every scope that is entered is exited exactly once,
    /// even if a query panics.
    ///
    /// `local` belongs to the [`Db`](crate::Db) or
    /// [`Snapshot`](crate::Snapshot) that entered the scope. Scopes that share
    /// a `local` are properly nested, so the most recently entered scope is
    /// always the first to exit. Scopes of different snapshots may interleave.
    /// See [`Handler::Local`] for details.
    ///
    /// # Examples
    ///
    /// A handler that measures the total time spent evaluating queries:
    ///
    /// ```
    /// use procrastinator::handler::Event;
    /// use procrastinator::handler::Handler;
    /// use procrastinator::handler::ScopedEvent;
    /// use procrastinator::handler::ScopedEventKind;
    /// use procrastinator::storage::Storage;
    /// use std::sync::Mutex;
    /// use std::time::Duration;
    /// use std::time::Instant;
    ///
    /// #[derive(Default)]
    /// struct EvalTimer {
    ///     total: Mutex<Duration>,
    /// }
    ///
    /// impl<S> Handler<S> for EvalTimer
    /// where
    ///     S: Storage,
    /// {
    ///     type Local = ();
    ///     // The start time of an evaluation, or `None` for other scopes.
    ///     type Payload = Option<Instant>;
    ///
    ///     fn event(&self, _event: Event<S>) {}
    ///
    ///     fn enter_scope(&self, (): &Self::Local, event: ScopedEvent<S>) -> Self::Payload {
    ///         match event.kind {
    ///             ScopedEventKind::Eval(_) => Some(Instant::now()),
    ///             _ => None,
    ///         }
    ///     }
    ///
    ///     fn exit_scope(&self, (): &Self::Local, start: Self::Payload) {
    ///         if let Some(start) = start {
    ///             *self.total.lock().unwrap() += start.elapsed();
    ///         }
    ///     }
    /// }
    /// ```
    fn enter_scope(&self, local: &Self::Local, event: ScopedEvent<S>) -> Self::Payload;

    /// Called when a [`ScopedEvent`] ends.
    ///
    /// Receives the payload that [`Handler::enter_scope`] returned when the
    /// scope started, and the same `local` that was passed to that call. See
    /// [`Handler::enter_scope`] for an example.
    ///
    /// This method also runs while a panic unwinds. Panicking here then aborts
    /// the process.
    fn exit_scope(&self, local: &Self::Local, payload: Self::Payload);
}

/// Something that happened at a single point in time.
///
/// Passed to [`Handler::event`]. See there for an example.
#[derive(Clone, Copy, Debug)]
pub struct Event<S>
where
    S: Storage,
{
    /// What happened.
    pub kind: EventKind<S>,
}

/// The kinds of [`Event`].
///
/// Events come in pairs that increase and decrease a count, so a handler can
/// track how many of each resource the database currently holds.
#[derive(Clone, Copy, Debug)]
pub enum EventKind<S>
where
    S: Storage,
{
    /// A query type was registered with the database.
    ///
    /// This happens the first time a query or input type is used, and again
    /// if it is used after [`Db::gc`](crate::Db::gc) deregistered it.
    RegisterQueryOps(QueryId),
    /// [`Db::gc`](crate::Db::gc) deregistered a query type.
    ///
    /// Garbage collection keeps only the registrations of input types.
    DeregisterQueryOps(QueryId),
    /// A value was stored.
    ///
    /// Values include query arguments, query outputs, and input values. The
    /// default [`Storage`](crate::storage::default::Storage) stores each
    /// distinct value only once, so storing a value equal to one that is
    /// already stored does not trigger this event.
    StoreValue(S::Id),
    /// A value was transferred to a new storage ID by
    /// [`Db::gc`](crate::Db::gc).
    TransferValue {
        /// The ID that formerly referred to this value.
        from: S::Id,
        /// The ID that currently refers to this value.
        to: S::Id,
    },
    /// A value was freed by [`Db::gc`](crate::Db::gc).
    FreeValue(S::Id),
    /// A memo was created.
    ///
    /// A memo holds the memoized output of one query for one set of arguments.
    /// Every input also has a memo, which holds its value.
    RegisterMemo(MemoId<S>),
    /// A memo was kept by [`Db::gc`](crate::Db::gc) and given a new ID.
    ///
    /// Garbage collection keeps only the memos of inputs. A kept memo's
    /// arguments may move to a new storage ID, which changes its [`MemoId`].
    /// This event fires for every kept memo, even if its ID did not change.
    TransferMemo {
        /// The ID that formerly referred to this memo.
        from: MemoId<S>,
        /// The ID that currently refers to this memo.
        to: MemoId<S>,
    },
    /// [`Db::gc`](crate::Db::gc) removed a memo.
    ///
    /// Garbage collection removes all memos except those that belong to inputs.
    /// Input memos trigger [`EventKind::TransferMemo`] instead.
    DeregisterMemo(MemoId<S>),
}

/// Something that spans a period of time.
///
/// Passed to [`Handler::enter_scope`] when the span starts. See there for an
/// example.
#[derive(Clone, Copy, Debug)]
pub struct ScopedEvent<S>
where
    S: Storage,
{
    /// What kind of span this is.
    pub kind: ScopedEventKind<S>,
}

/// The kinds of [`ScopedEvent`].
#[derive(Clone, Copy, Debug)]
pub enum ScopedEventKind<S>
where
    S: Storage,
{
    /// A call to [`Snapshot::query`](crate::Snapshot::query).
    ///
    /// The scope covers the whole call: checking whether the memoized output is
    /// up to date, and evaluating the query if it is not.
    Query(QueryId),
    /// An evaluation of [`Query::eval`](crate::query::Query::eval).
    ///
    /// This scope is only entered if memoized output from a query could not be
    /// reused. Note that, while an [`Eval`](ScopedEventKind::Eval) scope is
    /// always nested inside a [`Query`](ScopedEventKind::Query) scope, these do
    /// not necessarily correspond to one another. An evaluation scope may also
    /// belong to a dependency of the current query scope.
    Eval(MemoId<S>),
    /// A call to [`Db::set_input`](crate::Db::set_input) for the input whose
    /// memo has this ID.
    ///
    /// The scope covers the whole call: waiting for every
    /// [`Snapshot`](crate::Snapshot) to drop (see
    /// [`WaitExclusive`](ScopedEventKind::WaitExclusive)), storing the new
    /// value, and updating the input's memo.
    SetInput(MemoId<S>),
    /// A call to [`Db::gc`](crate::Db::gc).
    ///
    /// The scope covers the whole call: waiting for every
    /// [`Snapshot`](crate::Snapshot) to drop (see
    /// [`WaitExclusive`](ScopedEventKind::WaitExclusive)), transferring inputs,
    /// freeing values and memos, and deregistering query types. The events for
    /// these steps fire inside this scope.
    Gc,
    /// Waiting until every [`Snapshot`](crate::Snapshot) of the database has
    /// been dropped.
    ///
    /// [`Db::set_input`](crate::Db::set_input) and [`Db::gc`](crate::Db::gc)
    /// need exclusive access to the database, so they block until no snapshots
    /// remain. This scope is always nested inside a
    /// [`SetInput`](ScopedEventKind::SetInput) or [`Gc`](ScopedEventKind::Gc)
    /// scope, and ends before that call does any other work. It is entered even
    /// if no snapshots exist, in which case it ends right away.
    WaitExclusive,
}

/// Enters a scope on creation and exits it when dropped.
///
/// The guard keeps every [`Handler::enter_scope`] call matched with exactly one
/// [`Handler::exit_scope`] call, also when unwinding from a panic.
#[derive(Debug)]
pub(crate) struct ScopeGuard<'handler, S, H>
where
    S: Storage,
    H: Handler<S>,
{
    handler: &'handler H,
    local: &'handler H::Local,
    payload: Option<H::Payload>,
}

impl<S> Handler<S> for ()
where
    S: Storage,
{
    type Local = ();
    type Payload = ();

    fn event(&self, _: Event<S>) {}

    fn enter_scope(&self, (): &Self::Local, _: ScopedEvent<S>) -> Self::Payload {}

    fn exit_scope(&self, (): &Self::Local, (): Self::Payload) {}
}

impl<S> Event<S>
where
    S: Storage,
{
    /// Constructs a new event.
    #[must_use]
    pub fn new(kind: EventKind<S>) -> Self {
        Self { kind }
    }
}

impl<S> ScopedEvent<S>
where
    S: Storage,
{
    /// Constructs a new scoped event.
    #[must_use]
    pub fn new(kind: ScopedEventKind<S>) -> Self {
        Self { kind }
    }
}

impl<'handler, S, H> ScopeGuard<'handler, S, H>
where
    S: Storage,
    H: Handler<S>,
{
    pub(crate) fn new(
        handler: &'handler H,
        local: &'handler H::Local,
        event: ScopedEvent<S>,
    ) -> Self {
        Self {
            handler,
            local,
            payload: Some(handler.enter_scope(local, event)),
        }
    }
}

impl<S, H> Drop for ScopeGuard<'_, S, H>
where
    S: Storage,
    H: Handler<S>,
{
    fn drop(&mut self) {
        let Self {
            handler,
            local,
            payload,
        } = self;
        handler.exit_scope(local, payload.take().expect(VALUE_ALREADY_TAKEN));
    }
}
