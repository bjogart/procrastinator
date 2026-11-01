//! Cancellation policies for [`Snapshot::query_with`].
//!
//! When [`Db::set_input`](crate::Db::set_input) is called, it waits for all
//! snapshots to drop and asks running queries to finish early: from then on,
//! [`Snapshot::should_cancel`] returns `true`. A cancellation policy decides
//! how query calls behave in the meantime:
//!
//! - [`Fresh`] keeps evaluating queries as usual, so results stay up to date.
//!   This is what [`Snapshot::query`] uses.
//! - [`Stale`] returns memoized outputs without checking whether they are up
//!   to date, which avoids evaluating queries whose results will soon be
//!   discarded. This policy can potentially skip recursing into dependencies
//!   but at the cost that returned results may be invalid.
//!
//! Regardless of policy, users may choose to poll [`Snapshot::should_cancel`]
//! inside [`Query::eval`] and abort evaluation early by returning a fallback
//! value. During cancellation, results are never memoized, so this will never
//! affect future evaluations.
//!
//! [`Snapshot::query_with`]: crate::Snapshot::query_with
//! [`Snapshot::query`]: crate::Snapshot::query
//! [`Snapshot::should_cancel`]: crate::Snapshot::should_cancel
//! [`Query::eval`]: crate::query::Query::eval
//!
//! # Examples
//!
//! ```
//! use procrastinator::Db;
//! use procrastinator::Snapshot;
//! use procrastinator::handler::Handler;
//! use procrastinator::query::Input;
//! use procrastinator::query::InputId;
//! use procrastinator::query::Query;
//! use procrastinator::query::cancel::Stale;
//! use procrastinator::storage::Storage;
//!
//! struct CancellableSum;
//! impl Query for CancellableSum {
//!     type Args = Vec<u32>;
//!     type Out = Option<u32>;
//!
//!     fn eval<S, H>(snapshot: &Snapshot<S, H>, values: &Self::Args) -> Self::Out
//!     where
//!         S: Storage,
//!         H: Handler,
//!     {
//!         let mut sum = 0;
//!         for v in values {
//!             // Return early if we are asked to cancel.
//!             if snapshot.should_cancel() {
//!                 return None
//!             }
//!             sum += v;
//!         }
//!         Some(sum)
//!     }
//! }
//!
//! let mut db: Db = Db::default();
//! // This assertion will never cancel because cancellation is hard to test
//! // without multiple threads any synchronization primitives, but
//! // `CancellableSum` will poll `Snapshot::should_cancel` and exit early so new
//! // inputs can propagate quickly.
//! assert_eq!(*db.snapshot().query::<CancellableSum>(&vec![1, 2, 3, 4]), Some(10));
//! ```

use crate::query::cancel::sealed::CancellationPolicy;

pub(crate) mod sealed {
    pub trait CancellationPolicy {
        const CAN_RETURN_STALE_RESULTS: bool;
    }
}

/// A cancellation policy that always returns up-to-date results.
///
/// Queries are verified and evaluated as usual, also while
/// [`Snapshot::should_cancel`](crate::Snapshot::should_cancel) returns
/// `true`. Outputs evaluated during cancellation are returned but not
/// memoized.
///
/// This is the policy that [`Snapshot::query`](crate::Snapshot::query) uses.
///
/// # Examples
///
/// ```
/// use procrastinator::Db;
/// use procrastinator::query::Input;
/// use procrastinator::query::cancel::Fresh;
///
/// struct Width;
/// impl Input for Width {
///     type Value = u32;
/// }
///
/// let mut db: Db = Db::default();
/// let width = db.new_input::<Width>(&3);
///
/// // These two calls are equivalent.
/// let snapshot = db.snapshot();
/// assert_eq!(
///     *snapshot.query_with::<Width, Fresh>(&width),
///     *snapshot.query::<Width>(&width),
/// );
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Fresh;

/// A cancellation policy that prefers finishing early over up-to-date
/// results.
///
/// While [`Snapshot::should_cancel`](crate::Snapshot::should_cancel) returns
/// `true`, a query that has a memoized output returns it right away, without
/// checking whether its dependencies changed. The output may therefore be
/// based on outdated inputs. Only queries that were never evaluated before
/// are evaluated, and their outputs are not memoized.
///
/// Outside of cancellation, `Stale` behaves exactly like [`Fresh`].
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
/// use procrastinator::query::cancel::Stale;
/// use procrastinator::storage::Storage;
/// use std::thread;
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
/// let mut db: Db = Db::default();
/// let width = db.new_input::<Width>(&3);
/// assert_eq!(*db.snapshot().query::<Area>(&width), 9);
/// db.set_input(width, &4);
///
/// let snapshot = db.snapshot();
/// let worker = thread::spawn(move || {
///     // Wait until the main thread starts cancellation.
///     while !snapshot.should_cancel() {
///         thread::yield_now();
///     }
///     let fresh = *snapshot.query::<Area>(&width);
///     let stale = *snapshot.query_with::<Area, Stale>(&width);
///     (fresh, stale)
/// });
///
/// // Setting an equal value starts cancellation without changing anything.
/// db.set_input(width, &4);
/// // `Fresh` sees the current width of 4, while `Stale` returns the memoized
/// // area for the previous width of 3.
/// assert_eq!(worker.join().unwrap(), (16, 9));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Stale;

impl CancellationPolicy for Fresh {
    const CAN_RETURN_STALE_RESULTS: bool = false;
}

impl CancellationPolicy for Stale {
    const CAN_RETURN_STALE_RESULTS: bool = true;
}
