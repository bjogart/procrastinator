//! Queries and inputs: the building blocks of a computation.
//!
//! A computation in Procrastinator is a set of [`Query`] types that call each
//! other, with [`Input`] values at the bottom. Queries are like pure
//! functions, and inputs are like variables that are set from outside the
//! computation. The [`Db`](crate::Db) memoizes every query output, and tracks
//! which queries and inputs each query read. When an input changes, only the
//! queries that depend on it are evaluated again, and only once they are
//! called.
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
//! use procrastinator::storage::Storage;
//!
//! struct Words;
//! impl Input for Words {
//!     type Value = String;
//! }
//!
//! struct WordCount;
//! impl Query for WordCount {
//!     type Args = InputId<Words>;
//!     type Out = usize;
//!
//!     fn eval<S, H>(snapshot: &Snapshot<S, H>, words: &Self::Args) -> Self::Out
//!     where
//!         S: Storage,
//!         H: Handler,
//!     {
//!         snapshot.query::<Words>(words).split_whitespace().count()
//!     }
//! }
//!
//! let mut db: Db = Db::default();
//! let words = db.new_input::<Words>(&"to be or not to be".to_owned());
//! assert_eq!(*db.snapshot().query::<WordCount>(&words), 6);
//!
//! db.set_input(words, &"that is the question".to_owned());
//! assert_eq!(*db.snapshot().query::<WordCount>(&words), 4);
//! ```

use crate::Snapshot;
use crate::handler::Handler;
use crate::memo::MemoId;
use crate::storage::Storage;
use crate::storage::Transfer as _;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::hash::Hash;
use core::hash::Hasher;
use core::marker::PhantomData;

pub mod cancel;

/// A memoized computation, like a pure function.
///
/// A query declares its argument type, its output type, and how to compute
/// the output with [`Query::eval`]. It is called with
/// [`Snapshot::query`](crate::Snapshot::query), which evaluates it only if no
/// up-to-date memoized output exists for the given arguments.
///
/// The type that implements `Query` is never instantiated. It only serves as
/// a name for the query, so a unit struct is the usual choice.
///
/// # Examples
///
/// ```
/// use procrastinator::Db;
/// use procrastinator::Snapshot;
/// use procrastinator::handler::Handler;
/// use procrastinator::query::Query;
/// use procrastinator::storage::Storage;
///
/// struct Fibonacci;
/// impl Query for Fibonacci {
///     type Args = u64;
///     type Out = u64;
///
///     fn eval<S, H>(snapshot: &Snapshot<S, H>, n: &Self::Args) -> Self::Out
///     where
///         S: Storage,
///         H: Handler,
///     {
///         if *n < 2 {
///             *n
///         } else {
///             // Each `Fibonacci` call is memoized, so this takes linear
///             // rather than exponential time.
///             let a = *snapshot.query::<Fibonacci>(&(n - 1));
///             let b = *snapshot.query::<Fibonacci>(&(n - 2));
///             a + b
///         }
///     }
/// }
///
/// let db: Db = Db::default();
/// assert_eq!(*db.snapshot().query::<Fibonacci>(&50), 12_586_269_025);
/// ```
///
/// # Purity
///
/// The database assumes that queries are pure: the output of a query depends
/// only on its arguments and on what it reads through the snapshot. A query
/// must not read anything that can change behind the database's back, such as
/// mutable statics, interior mutability, randomness, the time, environment
/// variables, or the file system. Make such data an [`Input`] instead.
///
/// The same query may also be evaluated twice in parallel, on different
/// threads. For a pure query, this only costs time.
///
/// # Cycles
///
/// A query must not call itself with the same arguments, directly or through
/// other queries. Like infinite recursion in a regular function, a cycle
/// overflows the stack.
pub trait Query: Send + Sync + 'static {
    /// Arguments to the query.
    ///
    /// Two calls with equal arguments share one memoized output. Use a tuple
    /// or struct to pass multiple arguments.
    type Args: Clone + Eq + Hash + Send + Sync + 'static;

    /// The output of the query.
    ///
    /// When a query is evaluated again, the new output is compared to the
    /// previous one. If they are equal, queries that depend on this one keep
    /// their memoized outputs, so a change to an input stops propagating as
    /// soon as it no longer affects any outputs.
    type Out: Clone + Eq + Hash + Send + Sync + 'static;

    /// Computes the output of the query for the given arguments.
    ///
    /// You can call this method directly to evaluate a query, but results will
    /// not be memoized. Call [`Snapshot::query`](crate::Snapshot::query)
    /// instead, which memoizes the output and records the query as a dependency
    /// of its caller.
    ///
    /// Use `snapshot` to call other queries and to read inputs. Every query
    /// called through it becomes a dependency of this query. The method is
    /// generic over the storage `S` and handler `H`, so that a query works with
    /// any [`Db`](crate::Db).
    ///
    /// # Examples
    ///
    /// ```
    /// use procrastinator::Db;
    /// use procrastinator::Snapshot;
    /// use procrastinator::handler::Handler;
    /// use procrastinator::query::Query;
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
    /// let db: Db = Db::default();
    /// assert_eq!(*db.snapshot().query::<Shout>(&"hi".to_owned()), "HI");
    /// ```
    ///
    /// # Panics
    ///
    /// A panic in this method propagates out of the
    /// [`Snapshot::query`](crate::Snapshot::query) call that evaluated it. No
    /// output is memoized for the panicking query, so the next call evaluates
    /// it again.
    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: Handler;
}

/// A value that is set from outside the computation, like a variable.
///
/// Implementing `Input` declares the type of the value. The
/// implementing type is never instantiated, so a unit struct is the usual
/// choice. One input type can have many instances, each with its own value.
/// [`Db::new_input`](crate::Db::new_input) creates an instance and returns its
/// [`InputId`], and [`Db::set_input`](crate::Db::set_input) changes its value.
///
/// Every input is also a [`Query`], with [`InputId<Self>`](InputId) as its
/// arguments and [`Input::Value`] as its output. Queries read an input by
/// calling it like any other query, which records the input as a dependency.
/// As a consequence, a type cannot implement both `Input` and `Query`.
///
/// # Inputs or arguments?
///
/// Both input values and query arguments are memoized, but they behave
/// differently when data changes. An argument is part of the memo key: a
/// query called with a different argument is a different memo, evaluated from
/// scratch. An input keeps its [`InputId`] when its value changes, so queries
/// that read it are re-validated instead, and any query whose output turns out
/// unchanged spares its dependents from evaluating again.
///
/// As a rule of thumb, use an input for data that comes from outside the
/// computation and changes over time while staying "the same thing", such as
/// the contents of a file or a configuration setting. Use arguments for values
/// that select what to compute, such as names, indices, or [`InputId`]s, and
/// for any value that is computed by a query, since queries cannot create
/// inputs. Arguments are hashed on every call, so passing an `InputId` is also
/// cheaper than passing a large value directly.
///
/// # Examples
///
/// ```
/// use procrastinator::Db;
/// use procrastinator::query::Input;
///
/// struct Temperature;
/// impl Input for Temperature {
///     type Value = i32;
/// }
///
/// let mut db: Db = Db::default();
/// let inside = db.new_input::<Temperature>(&21);
/// let outside = db.new_input::<Temperature>(&-3);
///
/// let snapshot = db.snapshot();
/// assert_eq!(*snapshot.query::<Temperature>(&inside), 21);
/// assert_eq!(*snapshot.query::<Temperature>(&outside), -3);
/// ```
///
/// # Panics
///
/// Calling [`Query::eval`] on an input panics. Input values are set with
/// [`Db::new_input`](crate::Db::new_input) and
/// [`Db::set_input`](crate::Db::set_input), never computed.
pub trait Input: Send + Sync + 'static {
    /// The type of the input's value.
    type Value: Clone + Eq + Hash + Send + Sync;
}

#[derive(Debug)]
pub(crate) struct InputRegistry<S>(Vec<InputEntry<S>>)
where
    S: Storage;

#[derive(Debug)]
struct InputEntry<S>
where
    S: Storage,
{
    memo_id: MemoId<S>,
    transfer: TransferInput<S>,
}

#[derive(Debug)]
pub(crate) struct InputTransfer<'entry, S>(&'entry InputEntry<S>)
where
    S: Storage;

type TransferInput<S> = fn(
    transfer: &mut <S as Storage>::Transfer<'_>,
    curr_args_id: <S as Storage>::Id,
    curr_value_id: <S as Storage>::Id,
) -> (<S as Storage>::Id, <S as Storage>::Id);

/// Identifies one instance of input `I` in a [`Db`](crate::Db).
///
/// Returned by [`Db::new_input`](crate::Db::new_input). Pass it to
/// [`Snapshot::query`](crate::Snapshot::query) to read the value, or to
/// [`Db::set_input`](crate::Db::set_input) to change it.
///
/// An `InputId` is a small [`Copy`] value that implements [`Eq`] and
/// [`Hash`], so it can be part of a query's arguments. It stays valid for
/// the lifetime of the database, also across [`Db::gc`](crate::Db::gc).
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
/// struct Price;
/// impl Input for Price {
///     type Value = u32;
/// }
///
/// struct Cheapest;
/// impl Query for Cheapest {
///     type Args = (InputId<Price>, InputId<Price>);
///     type Out = u32;
///
///     fn eval<S, H>(snapshot: &Snapshot<S, H>, (a, b): &Self::Args) -> Self::Out
///     where
///         S: Storage,
///         H: Handler,
///     {
///         let a = *snapshot.query::<Price>(a);
///         let b = *snapshot.query::<Price>(b);
///         a.min(b)
///     }
/// }
///
/// let mut db: Db = Db::default();
/// let shop_a = db.new_input::<Price>(&12);
/// let shop_b = db.new_input::<Price>(&10);
/// assert_eq!(*db.snapshot().query::<Cheapest>(&(shop_a, shop_b)), 10);
/// ```
///
/// # Panics
///
/// An `InputId` is only meaningful for the database that created it. Using it
/// with a different database is a logic error, which may panic or refer to an
/// unrelated input.
pub struct InputId<I>(usize, PhantomData<I>);

impl<I> Query for I
where
    I: Input,
{
    type Args = InputId<Self>;

    type Out = <Self as Input>::Value;

    fn eval<S, H>(_: &Snapshot<S, H>, _: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: Handler,
    {
        panic!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}

impl<S> InputRegistry<S>
where
    S: Storage,
{
    pub(crate) fn new_input<I>(
        &mut self,
        bootstrap_memo: impl FnOnce(InputId<I>) -> MemoId<S>,
    ) -> InputId<I>
    where
        I: Input,
    {
        let Self(inputs) = self;
        let input_id = InputId(inputs.len(), PhantomData);
        let memo_id = bootstrap_memo(input_id);
        inputs.push(InputEntry {
            memo_id,
            transfer: transfer::<S, I>,
        });
        return input_id;

        fn transfer<S, I>(
            transfer: &mut <S as Storage>::Transfer<'_>,
            curr_args_id: <S as Storage>::Id,
            curr_value_id: <S as Storage>::Id,
        ) -> (<S as Storage>::Id, <S as Storage>::Id)
        where
            S: Storage,
            I: Input,
        {
            let next_args_id = transfer.transfer::<InputId<I>>(curr_args_id);
            let next_value_id = transfer.transfer::<<I as Query>::Out>(curr_value_id);
            (next_args_id, next_value_id)
        }
    }

    pub(crate) fn memo_id<I>(&self, input_id: InputId<I>) -> MemoId<S> {
        let InputId(idx, _) = input_id;
        self.0
            .get(idx)
            .expect("bug: unknown input ID (was this ID created by another database?)")
            .memo_id
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn transfer(
        &self,
        mut transfer: impl FnMut(InputTransfer<'_, S>) -> MemoId<S>,
    ) -> Self {
        let next = self
            .0
            .iter()
            .map(|entry| InputEntry {
                memo_id: transfer(InputTransfer(entry)),
                transfer: entry.transfer,
            })
            .collect();
        Self(next)
    }
}

impl<S> InputTransfer<'_, S>
where
    S: Storage,
{
    pub(crate) fn memo_id(&self) -> MemoId<S> {
        self.0.memo_id
    }

    pub(crate) fn transfer(
        &self,
        storage_transfer: &mut S::Transfer<'_>,
        value_id: S::Id,
    ) -> (MemoId<S>, S::Id) {
        let Self(InputEntry {
            memo_id: MemoId { query_id, args_id },
            transfer: transfer_input,
        }) = self;
        let (next_args_id, next_value_id) = transfer_input(storage_transfer, *args_id, value_id);
        let next_memo_id = MemoId {
            query_id: *query_id,
            args_id: next_args_id,
        };
        (next_memo_id, next_value_id)
    }
}

impl<S> Default for InputRegistry<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl<I> Clone for InputId<I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I> Copy for InputId<I> {}

impl<I> PartialEq for InputId<I> {
    fn eq(&self, other: &Self) -> bool {
        let Self(self_id, self_marker) = self;
        let Self(other_id, other_marker) = other;
        self_id == other_id && self_marker == other_marker
    }
}

impl<I> Eq for InputId<I> {}

impl<I> Hash for InputId<I> {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let Self(id, marker) = self;
        id.hash(state);
        marker.hash(state);
    }
}

impl<I> Debug for InputId<I> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Self(memo_id, _marker) = self;
        f.debug_tuple("InputId").field(&memo_id).finish()
    }
}
