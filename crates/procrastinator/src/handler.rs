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

use std::thread;
use std::thread::ThreadId;

pub mod perf;

const VALUE_ALREADY_TAKEN: &str = "bug: guard payload already taken";

/// Receives events from a database.
///
/// A [`Db`](crate::Db) calls its handler at points of interest during its
/// work. There are two kinds of events:
///
/// - An [`Event`] marks something that happened at a single point in time,
///   like storing a value. It is passed to [`Handler::event`].
/// - A [`ScopedEvent`] marks a span of time, like the evaluation of a query.
///   The database calls [`Handler::enter_scope`] when the span starts and
///   [`Handler::exit_scope`] when it ends. Scopes nest: a query that calls
///   another query is still in its own scope while the inner one runs.
///
/// The database creates its handler with [`Default`], and you can read it
/// back with [`Db::event_handler`](crate::Db::event_handler) or
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
/// use procrastinator::storage::default::Storage;
/// use std::sync::atomic::AtomicUsize;
/// use std::sync::atomic::Ordering;
///
/// #[derive(Default)]
/// struct QueryCounter {
///     queries: AtomicUsize,
/// }
///
/// impl Handler for QueryCounter {
///     type Payload = ();
///
///     fn event(&self, _event: Event) {}
///
///     fn enter_scope(&self, event: ScopedEvent) -> Self::Payload {
///         if let ScopedEventKind::Query = event.kind {
///             self.queries.fetch_add(1, Ordering::Relaxed);
///         }
///     }
///
///     fn exit_scope(&self, (): Self::Payload) {}
/// }
///
/// struct Width;
/// impl Input for Width {
///     type Value = u32;
/// }
///
/// let mut db: Db<Storage, QueryCounter> = Db::default();
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
pub trait Handler
where
    Self: Sized,
{
    /// Data carried from the start of a scope to its end.
    ///
    /// The value returned by [`Handler::enter_scope`] is passed to the
    /// matching [`Handler::exit_scope`] call. For example, a profiler can use
    /// the start time of the scope as its payload. Use `()` if no data is
    /// needed.
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
    /// use procrastinator::storage::default::Storage;
    /// use std::sync::atomic::AtomicUsize;
    /// use std::sync::atomic::Ordering;
    ///
    /// #[derive(Default)]
    /// struct MemoCounter {
    ///     memos: AtomicUsize,
    /// }
    ///
    /// impl Handler for MemoCounter {
    ///     type Payload = ();
    ///
    ///     fn event(&self, event: Event) {
    ///         match event.kind {
    ///             EventKind::RegisterMemo => {
    ///                 self.memos.fetch_add(1, Ordering::Relaxed);
    ///             }
    ///             EventKind::DeregisterMemos(count) => {
    ///                 self.memos.fetch_sub(count, Ordering::Relaxed);
    ///             }
    ///             _ => {}
    ///         }
    ///     }
    ///
    ///     fn enter_scope(&self, _event: ScopedEvent) -> Self::Payload {}
    ///
    ///     fn exit_scope(&self, (): Self::Payload) {}
    /// }
    ///
    /// struct Width;
    /// impl Input for Width {
    ///     type Value = u32;
    /// }
    ///
    /// let mut db: Db<Storage, MemoCounter> = Db::default();
    /// // Every input has a memo that holds its value.
    /// db.new_input::<Width>(&3);
    /// db.new_input::<Width>(&4);
    /// assert_eq!(db.event_handler().memos.load(Ordering::Relaxed), 2);
    /// ```
    fn event(&self, event: Event);

    /// Called when a [`ScopedEvent`] starts.
    ///
    /// The returned payload is passed to the [`Handler::exit_scope`] call that
    /// ends this scope. Every scope that is entered is exited exactly once,
    /// even if a query panics. Scopes on one thread are properly nested, so
    /// the most recently entered scope on a thread is always the first to
    /// exit. Scopes on different threads may interleave; use
    /// [`ScopedEvent::thread_id`] to tell them apart.
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
    /// use std::sync::Mutex;
    /// use std::time::Duration;
    /// use std::time::Instant;
    ///
    /// #[derive(Default)]
    /// struct EvalTimer {
    ///     total: Mutex<Duration>,
    /// }
    ///
    /// impl Handler for EvalTimer {
    ///     // The start time of an evaluation, or `None` for other scopes.
    ///     type Payload = Option<Instant>;
    ///
    ///     fn event(&self, _event: Event) {}
    ///
    ///     fn enter_scope(&self, event: ScopedEvent) -> Self::Payload {
    ///         match event.kind {
    ///             ScopedEventKind::Eval => Some(Instant::now()),
    ///             ScopedEventKind::Query => None,
    ///         }
    ///     }
    ///
    ///     fn exit_scope(&self, start: Self::Payload) {
    ///         if let Some(start) = start {
    ///             *self.total.lock().unwrap() += start.elapsed();
    ///         }
    ///     }
    /// }
    /// ```
    fn enter_scope(&self, event: ScopedEvent) -> Self::Payload;

    /// Called when a [`ScopedEvent`] ends.
    ///
    /// Receives the payload that [`Handler::enter_scope`] returned when the
    /// scope started. See [`Handler::enter_scope`] for an example.
    ///
    /// This method also runs while a panic unwinds out of a query. Panicking
    /// here then aborts the process.
    fn exit_scope(&self, payload: Self::Payload);
}

/// Something that happened at a single point in time.
///
/// Passed to [`Handler::event`]. See there for an example.
#[derive(Clone, Copy, Debug)]
pub struct Event {
    /// The thread on which the event happened.
    pub thread_id: ThreadId,
    /// What happened.
    pub kind: EventKind,
}

/// The kinds of [`Event`].
///
/// Events come in pairs that increase and decrease a count, so a handler can
/// track how many of each resource the database currently holds.
#[derive(Clone, Copy, Debug)]
pub enum EventKind {
    /// A query type was registered with the database.
    ///
    /// This happens the first time a query or input type is used, and again
    /// if it is used after [`Db::gc`](crate::Db::gc) deregistered it.
    RegisterQueryOps,
    /// [`Db::gc`](crate::Db::gc) deregistered a query type.
    ///
    /// Garbage collection keeps only the registrations of input types.
    DeregisterQueryOps,
    /// A value was stored.
    ///
    /// Values include query arguments, query outputs, and input values. The
    /// default [`Storage`](crate::storage::default::Storage) stores each
    /// distinct value only once, so storing a value equal to one that is
    /// already stored does not trigger this event.
    StoreValue,
    /// [`Db::gc`](crate::Db::gc) freed this many stored values.
    FreeValues(usize),
    /// A memo was created.
    ///
    /// A memo holds the memoized output of one query for one set of
    /// arguments. Every input also has a memo, which holds its value.
    RegisterMemo,
    /// [`Db::gc`](crate::Db::gc) removed this many memos.
    DeregisterMemos(usize),
}

/// Something that spans a period of time.
///
/// Passed to [`Handler::enter_scope`] when the span starts. See there for an
/// example.
#[derive(Clone, Copy, Debug)]
pub struct ScopedEvent {
    /// The thread on which the span runs.
    pub thread_id: ThreadId,
    /// What kind of span this is.
    pub kind: ScopedEventKind,
}

/// The kinds of [`ScopedEvent`].
#[derive(Clone, Copy, Debug)]
pub enum ScopedEventKind {
    /// A call to [`Snapshot::query`](crate::Snapshot::query) or
    /// [`Snapshot::query_with`](crate::Snapshot::query_with).
    ///
    /// The scope covers the whole call: checking whether the memoized output
    /// is up to date, and evaluating the query if it is not.
    Query,
    /// An evaluation of [`Query::eval`](crate::query::Query::eval).
    ///
    /// This scope is always nested inside a [`ScopedEventKind::Query`] scope,
    /// and it is only entered if the memoized output could not be reused.
    Eval,
}

/// Enters a scope on creation and exits it when dropped.
///
/// The guard keeps every [`Handler::enter_scope`] call matched with exactly one
/// [`Handler::exit_scope`] call, also when unwinding from a panic.
#[derive(Debug)]
pub(crate) struct ScopeGuard<'handler, H>
where
    H: Handler,
{
    handler: &'handler H,
    payload: Option<H::Payload>,
}

impl Handler for () {
    type Payload = ();

    fn event(&self, _: Event) {}

    fn enter_scope(&self, _: ScopedEvent) -> Self::Payload {}

    fn exit_scope(&self, (): Self::Payload) {}
}

impl Event {
    pub(crate) fn new(kind: EventKind) -> Self {
        Self {
            thread_id: thread::current().id(),
            kind,
        }
    }
}

impl ScopedEvent {
    pub(crate) fn new(kind: ScopedEventKind) -> Self {
        Self {
            thread_id: thread::current().id(),
            kind,
        }
    }
}

impl<'handler, H> ScopeGuard<'handler, H>
where
    H: Handler,
{
    pub(crate) fn new(handler: &'handler H, event: ScopedEvent) -> Self {
        Self {
            handler,
            payload: Some(handler.enter_scope(event)),
        }
    }
}

impl<H> Drop for ScopeGuard<'_, H>
where
    H: Handler,
{
    fn drop(&mut self) {
        self.handler
            .exit_scope(self.payload.take().expect(VALUE_ALREADY_TAKEN));
    }
}
