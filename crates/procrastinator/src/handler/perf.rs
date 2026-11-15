//! A ready-made [`Handler`](crate::handler::Handler) that collects
//! performance metrics.
//!
//! See [`Handler`] for details.

use crate::handler::Event;
use crate::handler::EventKind;
use crate::handler::Handler as HandlerTrait;
use crate::handler::ScopedEvent;
use crate::handler::ScopedEventKind;
use crate::storage::Storage;
use core::cell::Cell;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::time::Instant;

/// A handler that counts and times database operations.
///
/// The handler only ever adds to its counters. Read them with
/// [`Handler::stats`], which returns the totals since the database was
/// created. To measure a single operation, take the [`Stats`] before and after
/// it and subtract them with [`Stats::since`].
///
/// All counters are atomic, so the handler can be read from any thread while
/// queries run.
///
/// # Examples
///
/// ```
/// use procrastinator::Db;
/// use procrastinator::Snapshot;
/// use procrastinator::handler::Handler;
/// use procrastinator::handler::perf;
/// use procrastinator::query::Input;
/// use procrastinator::query::InputId;
/// use procrastinator::query::Query;
/// use procrastinator::storage::Storage;
/// use procrastinator::storage::default;
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
/// let mut db: Db<default::Storage, perf::Handler> = Db::default();
/// let width = db.new_input::<Width>(&3);
///
/// let snapshot = db.snapshot();
/// snapshot.query::<Area>(&width);
/// let first = snapshot.event_handler().stats();
/// // One call to `Area`, plus one call to `Width` from inside `Area::eval`.
/// assert_eq!(first.queries, 2);
/// assert_eq!(first.evals, 1);
/// // One memo for the input, and one for `Area`.
/// assert_eq!(first.memos.net(), 2);
///
/// snapshot.query::<Area>(&width);
/// let second = snapshot.event_handler().stats().since(first);
/// // The second call to `Area` reused the memoized output.
/// assert_eq!(second.queries, 1);
/// assert_eq!(second.evals, 0);
/// ```
#[derive(Debug, Default)]
pub struct Handler {
    query: AtomicScope,
    eval: AtomicScope,
    set_input: AtomicScope,
    gc: AtomicScope,
    wait_exclusive: AtomicScope,
    query_ops: AtomicCount,
    values: AtomicCount,
    memos: AtomicCount,
}

#[derive(Debug, Default)]
struct AtomicScope {
    count: AtomicU64,
    ns: AtomicU64,
}

#[derive(Debug, Default)]
struct AtomicCount {
    added: AtomicU64,
    removed: AtomicU64,
}

/// The metrics collected by a [`Handler`].
///
/// [`Handler::stats`] returns the totals since the database was created.
/// [`Stats::since`] subtracts two values, which gives the metrics of everything
/// that happened in between.
///
/// # Scopes
///
/// Scopes are counted and timed when they end. An operation that is still
/// running is not included, and neither are the operations it is nested in.
/// Operations that end in a panic are included.
///
/// Every time is a self-time: it excludes the time spent in nested scopes,
/// which is recorded under the nested scope instead. No span of time is counted
/// twice, so the times of one [`Snapshot`](crate::Snapshot) add up to the time
/// it spent inside its outermost [`Snapshot::query`](crate::Snapshot::query)
/// calls. Times of different snapshots are added together, so with queries
/// running on several threads the total can exceed the elapsed wall-clock time.
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
///
/// let before = db.event_handler().stats();
/// db.set_input(width, &4);
/// db.snapshot().query::<Width>(&width);
/// let stats = db.event_handler().stats().since(before);
///
/// assert_eq!(stats.set_inputs, 1);
/// assert_eq!(stats.queries, 1);
/// // Creating the input happened before the measurement started. Setting it
/// // stored one new value.
/// assert_eq!(stats.memos, perf::Count::default());
/// assert_eq!(stats.values.added, 1);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
#[non_exhaustive]
pub struct Stats {
    /// The number of [`Snapshot::query`](crate::Snapshot::query) calls,
    /// including calls from inside other queries and reads of inputs.
    pub queries: u64,
    /// The time spent in [`Snapshot::query`](crate::Snapshot::query) outside of
    /// [`Query::eval`](crate::query::Query::eval): storing arguments and
    /// outputs, looking up memos, and checking whether dependencies changed.
    pub query_overhead: Duration,
    /// The number of [`Query::eval`](crate::query::Query::eval) calls.
    ///
    /// A query is only evaluated if its memoized output could not be reused.
    /// Calling one query can evaluate several: its dependencies are
    /// re-evaluated first if their own dependencies changed.
    pub evals: u64,
    /// The time spent in [`Query::eval`](crate::query::Query::eval), excluding
    /// the queries it calls.
    pub eval_time: Duration,
    /// The number of [`Db::set_input`](crate::Db::set_input) calls.
    pub set_inputs: u64,
    /// The time spent in [`Db::set_input`](crate::Db::set_input), excluding
    /// [`Stats::wait_time`].
    pub set_input_time: Duration,
    /// The number of [`Db::gc`](crate::Db::gc) calls.
    pub gcs: u64,
    /// The time spent in [`Db::gc`](crate::Db::gc), excluding
    /// [`Stats::wait_time`].
    pub gc_time: Duration,
    /// The time [`Db::set_input`](crate::Db::set_input) and
    /// [`Db::gc`](crate::Db::gc) spent waiting for every
    /// [`Snapshot`](crate::Snapshot) to drop.
    pub wait_time: Duration,
    /// Query and input types registered with and deregistered from the
    /// database.
    ///
    /// See [`EventKind::RegisterQueryOps`].
    pub query_ops: Count,
    /// Values stored in and freed from the database.
    ///
    /// See [`EventKind::StoreValue`] for what counts as a value. Values that
    /// [`Db::gc`](crate::Db::gc) keeps are counted as neither.
    pub values: Count,
    /// Memos created in and removed from the database.
    ///
    /// See [`EventKind::RegisterMemo`] for what a memo is. Memos that
    /// [`Db::gc`](crate::Db::gc) keeps are counted as neither.
    pub memos: Count,
}

/// How many resources of one kind were added and removed.
///
/// Both numbers only ever grow. In the totals returned by [`Handler::stats`],
/// [`Count::net`] is the number of resources the database currently holds.
///
/// # Examples
///
/// ```
/// use procrastinator::handler::perf::Count;
///
/// let count = Count {
///     added: 2,
///     removed: 5,
/// };
/// assert_eq!(count.net(), -3);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Count {
    /// The number of resources that were added.
    pub added: u64,
    /// The number of resources that were removed.
    pub removed: u64,
}

impl<S> HandlerTrait<S> for Handler
where
    S: Storage,
{
    type Local = Cell<u64>;
    type Payload = (ScopedEventKind<S>, u64, Instant);

    fn event(&self, event: Event<S>) {
        let Self {
            query: _,
            eval: _,
            set_input: _,
            gc: _,
            wait_exclusive: _,
            query_ops,
            values,
            memos,
        } = self;
        let counter = match event.kind {
            EventKind::RegisterQueryOps(_) => &query_ops.added,
            EventKind::DeregisterQueryOps(_) => &query_ops.removed,
            EventKind::StoreValue(_) => &values.added,
            EventKind::FreeValue(_) => &values.removed,
            EventKind::RegisterMemo(_) => &memos.added,
            EventKind::DeregisterMemo(_) => &memos.removed,
            EventKind::TransferValue { .. } | EventKind::TransferMemo { .. } => return,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn enter_scope(&self, attributed_ns: &Self::Local, event: ScopedEvent<S>) -> Self::Payload {
        (event.kind, attributed_ns.get(), Instant::now())
    }

    fn exit_scope(&self, attributed_ns: &Self::Local, payload: Self::Payload) {
        let (kind, attributed_at_start, start) = payload;
        let elapsed = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let Self {
            query,
            eval,
            set_input,
            gc,
            wait_exclusive,
            query_ops: _,
            values: _,
            memos: _,
        } = self;
        // Scopes that share `attributed_ns` are properly nested. Whatever was
        // attributed since this scope started was therefore recorded by the
        // scopes nested inside it, and the remainder is this scope's own time.
        // This must not panic: scopes also exit while a panic unwinds, and a
        // panic then will abort the process.
        let attributed = attributed_ns.get();
        let nested = attributed.saturating_sub(attributed_at_start);
        let own = elapsed.saturating_sub(nested);
        attributed_ns.set(attributed.saturating_add(own));
        let scope = match kind {
            ScopedEventKind::Query(_) => query,
            ScopedEventKind::Eval(_) => eval,
            ScopedEventKind::SetInput(_) => set_input,
            ScopedEventKind::Gc => gc,
            ScopedEventKind::WaitExclusive => wait_exclusive,
        };
        scope.count.fetch_add(1, Ordering::Relaxed);
        scope.ns.fetch_add(own, Ordering::Relaxed);
    }
}

impl Handler {
    /// Returns the metrics collected since the database was created.
    ///
    /// The counters are read one by one. If queries are running on other
    /// threads in the meantime, the result may include some of the scopes and
    /// events that finish during the call and not others. Every counter only
    /// grows, so [`Stats::since`] is accurate up to those few operations.
    ///
    /// See [`Stats`] for an example.
    #[must_use]
    pub fn stats(&self) -> Stats {
        let Self {
            query,
            eval,
            set_input,
            gc,
            wait_exclusive,
            query_ops,
            values,
            memos,
        } = self;
        let (queries, query_overhead) = query.load();
        let (evals, eval_time) = eval.load();
        let (set_inputs, set_input_time) = set_input.load();
        let (gcs, gc_time) = gc.load();
        let (_, wait_time) = wait_exclusive.load();
        Stats {
            queries,
            evals,
            set_inputs,
            gcs,
            query_overhead,
            eval_time,
            set_input_time,
            gc_time,
            wait_time,
            query_ops: query_ops.load(),
            values: values.load(),
            memos: memos.load(),
        }
    }
}

impl AtomicScope {
    fn load(&self) -> (u64, Duration) {
        let Self { count, ns } = self;
        (
            count.load(Ordering::Relaxed),
            Duration::from_nanos(ns.load(Ordering::Relaxed)),
        )
    }
}

impl AtomicCount {
    fn load(&self) -> Count {
        let Self { added, removed } = self;
        let removed = removed.load(Ordering::Relaxed);
        let added = added.load(Ordering::Relaxed);
        Count { added, removed }
    }
}

impl Stats {
    /// Returns the metrics of everything that happened after `earlier` was
    /// taken.
    ///
    /// Both values must come from the same [`Handler`], and `earlier` must be
    /// the older one. Otherwise the result is meaningless, but harmless: fields
    /// saturate at 0.
    ///
    /// See [`Stats`] for an example.
    #[must_use]
    pub fn since(self, earlier: Self) -> Self {
        let Self {
            queries,
            evals,
            set_inputs,
            gcs,
            query_overhead,
            eval_time,
            set_input_time,
            gc_time,
            wait_time,
            query_ops,
            values,
            memos,
        } = self;
        Self {
            queries: queries.saturating_sub(earlier.queries),
            evals: evals.saturating_sub(earlier.evals),
            set_inputs: set_inputs.saturating_sub(earlier.set_inputs),
            gcs: gcs.saturating_sub(earlier.gcs),
            query_overhead: query_overhead.saturating_sub(earlier.query_overhead),
            eval_time: eval_time.saturating_sub(earlier.eval_time),
            set_input_time: set_input_time.saturating_sub(earlier.set_input_time),
            gc_time: gc_time.saturating_sub(earlier.gc_time),
            wait_time: wait_time.saturating_sub(earlier.wait_time),
            query_ops: query_ops.since(earlier.query_ops),
            values: values.since(earlier.values),
            memos: memos.since(earlier.memos),
        }
    }

    /// Returns the total time spent in
    /// [`Snapshot::query`](crate::Snapshot::query) calls.
    ///
    /// This is the sum of [`Stats::query_overhead`] and [`Stats::eval_time`].
    /// Nested calls are not counted twice.
    #[must_use]
    pub fn query_time(self) -> Duration {
        self.query_overhead.saturating_add(self.eval_time)
    }
}

impl Count {
    /// Returns the number of resources added minus the number removed.
    ///
    /// In the totals returned by [`Handler::stats`], this is the number of
    /// resources the database currently holds. In the result of
    /// [`Stats::since`], it is how much that number changed, which is negative
    /// if more resources were removed than added.
    ///
    /// See [`Count`] for an example.
    #[must_use]
    pub fn net(self) -> i64 {
        let Self { added, removed } = self;
        if added >= removed {
            i64::try_from(added.saturating_sub(removed)).unwrap_or(i64::MAX)
        } else {
            i64::try_from(removed.saturating_sub(added)).map_or(i64::MIN, i64::saturating_neg)
        }
    }

    fn since(self, earlier: Self) -> Self {
        let Self { added, removed } = self;
        Self {
            added: added.saturating_sub(earlier.added),
            removed: removed.saturating_sub(earlier.removed),
        }
    }
}
