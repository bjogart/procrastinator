//! A ready-made [`Handler`](crate::handler::Handler) that collects
//! performance metrics.
//!
//! See [`Handler`] for details.

use crate::handler::Event;
use crate::handler::EventKind;
use crate::handler::ScopedEvent;
use crate::handler::ScopedEventKind;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::time::Instant;

use crate::handler::Handler as HandlerTrait;

/// A handler that counts and times database operations.
///
/// It tracks two kinds of metrics:
///
/// - Accumulators count query calls and evaluations, and measure the time
///   spent in them. [`Handler::reset`] sets them back to zero, so you can
///   measure one operation at a time.
/// - Gauges report how many query types, values, and memos the database
///   currently holds. They are not affected by [`Handler::reset`].
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
///         H: Handler,
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
/// snapshot.query::<Area>(&width);
///
/// let handler = snapshot.event_handler();
/// // Two calls to `Area`, plus one call to `Width` from inside `Area::eval`.
/// assert_eq!(handler.query_count(), 3);
/// // The second call to `Area` reused the memoized output.
/// assert_eq!(handler.eval_count(), 1);
/// // One memo for the input, and one for `Area`.
/// assert_eq!(handler.memo_count(), 2);
/// ```
#[derive(Debug, Default)]
pub struct Handler {
    query_time: AtomicDuration,
    eval_time: AtomicDuration,
    query_count: AtomicUsize,
    eval_count: AtomicUsize,
    registered_query_ops: AtomicUsize,
    stored_values: AtomicUsize,
    memo_count: AtomicUsize,
}

#[derive(Debug, Default)]
struct AtomicDuration {
    ns: AtomicU64,
}

impl HandlerTrait for Handler {
    type Payload = (ScopedEventKind, Instant);

    fn event(&self, event: Event) {
        match event.kind {
            EventKind::RegisterQueryOps => {
                self.registered_query_ops.fetch_add(1, Ordering::Relaxed);
            }
            EventKind::DeregisterQueryOps => {
                self.registered_query_ops.fetch_sub(1, Ordering::Relaxed);
            }
            EventKind::StoreValue => {
                self.stored_values.fetch_add(1, Ordering::Relaxed);
            }
            EventKind::FreeValues(count) => {
                self.stored_values.fetch_sub(count, Ordering::Relaxed);
            }
            EventKind::RegisterMemo => {
                self.memo_count.fetch_add(1, Ordering::Relaxed);
            }
            EventKind::DeregisterMemos(n) => {
                self.memo_count.fetch_sub(n, Ordering::Relaxed);
            }
        }
    }

    fn enter_scope(&self, event: ScopedEvent) -> Self::Payload {
        match event.kind {
            ScopedEventKind::Query => self.query_count.fetch_add(1, Ordering::Relaxed),
            ScopedEventKind::Eval => self.eval_count.fetch_add(1, Ordering::Relaxed),
        };
        (event.kind, Instant::now())
    }

    fn exit_scope(&self, payload: Self::Payload) {
        let (kind, start) = payload;
        let elapsed = start.elapsed();
        match kind {
            ScopedEventKind::Query => self.query_time.add(elapsed),
            ScopedEventKind::Eval => self.eval_time.add(elapsed),
        }
    }
}

impl Handler {
    /// Reset metric accumulators.
    ///
    /// This function will reset counts and durations, but not global values,
    /// like number of memos allocated.
    ///
    /// Note that, though individual counters are atomic, the reset touches
    /// multiple counters. Events emitted during a reset on a different thread
    /// can desynchronize counters. Users should take care to only reset from
    /// the main thread while no other queries are running.
    pub fn reset(&self) {
        let Self {
            query_time,
            eval_time,
            query_count,
            eval_count,
            registered_query_ops: _,
            stored_values: _,
            memo_count: _,
        } = self;
        query_time.reset();
        eval_time.reset();
        query_count.store(0, Ordering::Relaxed);
        eval_count.store(0, Ordering::Relaxed);
    }

    /// Returns the total time spent in query calls since the last reset.
    ///
    /// This sums the duration of every [`ScopedEventKind::Query`] scope.
    /// Nested calls are counted in full for each level, and calls on
    /// different threads are added together, so the total can exceed the
    /// elapsed wall-clock time.
    pub fn query_time(&self) -> Duration {
        self.query_time.duration()
    }

    /// Returns the total time spent evaluating queries since the last reset.
    ///
    /// This sums the duration of every [`ScopedEventKind::Eval`] scope. As
    /// with [`Handler::query_time`], nested evaluations are counted at
    /// every level and threads are added together.
    pub fn eval_time(&self) -> Duration {
        self.eval_time.duration()
    }

    /// Returns the number of query calls since the last reset.
    ///
    /// Every call to [`Snapshot::query`](crate::Snapshot::query) or
    /// [`Snapshot::query_with`](crate::Snapshot::query_with) is counted,
    /// including calls from inside other queries.
    pub fn query_count(&self) -> usize {
        self.query_count.load(Ordering::Relaxed)
    }

    /// Returns the number of query evaluations since the last reset.
    ///
    /// A query call only evaluates the query if its memoized output could not
    /// be reused. The ratio between [`Handler::eval_count`] and
    /// [`Handler::query_count`] shows how much work memoization saved.
    pub fn eval_count(&self) -> usize {
        self.eval_count.load(Ordering::Relaxed)
    }

    /// Returns the number of query and input types the database currently has
    /// registered.
    pub fn registered_query_ops(&self) -> usize {
        self.registered_query_ops.load(Ordering::Relaxed)
    }

    /// Returns the number of values the database currently stores.
    ///
    /// See [`EventKind::StoreValue`] for what counts as a value.
    pub fn stored_values(&self) -> usize {
        self.stored_values.load(Ordering::Relaxed)
    }

    /// Returns the number of memos the database currently holds.
    ///
    /// See [`EventKind::RegisterMemo`] for what a memo is.
    pub fn memo_count(&self) -> usize {
        self.memo_count.load(Ordering::Relaxed)
    }
}

impl AtomicDuration {
    fn add(&self, duration: Duration) {
        let Ok(ns) = duration.as_nanos().try_into() else {
            // After `u64::MAX` nanoseconds a few seconds are unlikely to move
            // the needle. Ignore it.
            return;
        };
        self.ns.fetch_add(ns, Ordering::Relaxed);
    }

    fn duration(&self) -> Duration {
        Duration::from_nanos(self.ns.load(Ordering::Relaxed))
    }

    fn reset(&self) {
        self.ns.store(0, Ordering::Relaxed);
    }
}
