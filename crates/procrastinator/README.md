# Procrastinator

Procrastinator is a tiny library for on-demand incremental computation.

Many programs&mdash;like language servers and build tools&mdash;compute results from data that changes only a little between updates.
Caching helps cut down on precious CPU cycles,
but hand-written caching is famously difficult.
Procrastinator provides tools to do the bookkeeping for you.
You write your program as pure functions called _queries_.
Results are memoized and dependencies are tracked automatically.
When an input changes, only affected queries are re-evaluated,
and only once the result is needed.

## Installation

Add Procrastinator to your project with:

```sh
cargo add procrastinator
```

The minimum supported Rust version is 1.85.1.

## Toy example

Picture this:
a side panel in a webshop that shows the price with shipping of items in a shopping cart.
In simple Rust code, that may look something like this:

```rust
struct Entry {
    price: usize,
    quantity: usize,
}

fn entry_total(entry: &Entry) -> usize {
    entry.price * entry.quantity
}

fn subtotal(cart: &[Entry]) -> usize {
    cart.iter().map(entry_total).sum()
}

fn shipping(cart: &[Entry]) -> usize {
    if subtotal(cart) < 25 { 10 } else { 0 }
}

fn total(cart: &[Entry]) -> usize {
    subtotal(cart) + shipping(cart)
}

// Buy 2 of the same T-shirt.
let mut cart = vec![Entry { price: 12, quantity: 2 }];
assert_eq!(total(&cart), 34);

// Actually---two different T-shirts might be nice as well. Let's swap out
// one T-shirt for another one with the same price.
cart = vec![
    Entry { price: 12, quantity: 1 },
    Entry { price: 12, quantity: 1 },
];
assert_eq!(total(&cart), 34);

// Add a cheap mug for free shipping.
cart = vec![
    Entry { price: 12, quantity: 1 },
    Entry { price: 12, quantity: 1 },
    Entry { price: 6, quantity: 1 },
];
assert_eq!(total(&cart), 30);
```

This example contains a lot of redundancy.
Every `total()` call makes two calls to `subtotal()`, one directly, and one inside `shipping()`.
After the swap from a duplicate T-shirt to a second T-shirt,
each call to `subtotal()` calls `entry_total(&Entry { price: 12, quantity: 1 })` twice&mdash;four times total,
accounting for the `subtotal()` repetition.
And there is more duplication in this example:
adding the mug initiates another round of T-shirt calculation,
even though the T-shirts were not modified at all.
Here is the same example using Procrastinator:

```rust
use procrastinator::Db;
use procrastinator::Snapshot;
use procrastinator::handler::Handler;
use procrastinator::query::Input;
use procrastinator::query::InputId;
use procrastinator::query::Query;
use procrastinator::storage::Storage;

// Inputs are like mutable variables or values from outside the system.
struct Cart;
impl Input for Cart {
    type Value = Vec<Entry>;
}

// The same `Entry` as before. Procrastinator requires values to implement
// `Clone`, `Eq`, `Hash`, and the auto traits `Send` and `Sync`.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Entry {
    price: usize,
    quantity: usize,
}

// Queries take the place of functions.
struct EntryTotal;
impl Query for EntryTotal {
    type Args = Entry;
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, entry: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: Handler,
    {
        entry.price * entry.quantity
    }
}

struct Subtotal;
impl Query for Subtotal {
    // A variable is accessed by name; `Input`s are accessed by `InputId`s.
    type Args = InputId<Cart>;
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, cart: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: Handler,
    {
        // Queries are called with `Snapshot::query`. Inputs also implement
        // `Query` and can be accessed the same way as queries.
        let entries = snapshot.query::<Cart>(cart);
        entries
            .iter()
            .map(|entry| {
                // To reduce the number of clones, `snapshot.query` wraps
                // values in `Arc` by default, so the result needs to be
                // dereferenced here.
                *snapshot.query::<EntryTotal>(entry)
            })
            .sum()
    }
}

struct Shipping;
impl Query for Shipping {
    type Args = InputId<Cart>;
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, cart: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: Handler,
    {
        if *snapshot.query::<Subtotal>(cart) < 25 {
            10
        } else {
            0
        }
    }
}

struct Total;
impl Query for Total {
    type Args = InputId<Cart>;
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, cart: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: Handler,
    {
        let subtotal = *snapshot.query::<Subtotal>(cart);
        let shipping = *snapshot.query::<Shipping>(cart);
        subtotal + shipping
    }
}

// Values and intermediate results live in a `Db`. The `let mut db: Db` type
// annotation looks redundant here, but it is necessary to infer the default
// type parameters.
let mut db: Db = Db::default();

// The previously mutable variable is now stored inside the database as an
// `Input`. `db.new_input::<Cart>()` declares a new variable and returns an
// `InputId<Cart>` that can be used to access the value.
let cart = db.new_input::<Cart>(&vec![Entry { price: 12, quantity: 2 }]);
// `Db` can mutate inputs. Queries depend on inputs, so you need to make a
// snapshot of database state before you can call queries on it. Snapshots can
// be sent to other threads as well. Snapshots read the same database
// underneath, so making a new snapshot is a cheap, constant-time operation.
let snapshot = db.snapshot();
assert_eq!(*snapshot.query::<Total>(&cart), 34);
// One caveat to snapshots: modifying an input with `db.set_input()` blocks
// until all live snapshots have dropped. Picture it: `db.set_input()` will
// not finish until the snapshot drops at the end of the scope, but the end of
// the scope will never be reached until `db.set_input()` returns---a deadlock.
// If you assign a snapshot to a variable, make sure to drop it once you are
// done! Making a new snapshot is quick, but deadlocks are forever.
drop(snapshot);

// Now swap one T-shirt for another one. Upon entering `Subtotal::eval()`, the
// first entry is evaluated. The second entry---which is the same as the
// first---is cached. Since the output of `Subtotal` is the same as before,
// `Shipping` and `Total` are cached as well.
db.set_input(
    cart,
    &vec![
        Entry { price: 12, quantity: 1 },
        Entry { price: 12, quantity: 1 },
    ],
);
assert_eq!(*db.snapshot().query::<Total>(&cart), 34);

// Finally, add the mug. `Subtotal` evaluates again. This time the output is
// different, and that result propagates to both `Shipping` and `Total`, which
// also re-evaluate. The T-shirt entries did not change and they stay cached.
db.set_input(
    cart,
    &vec![
        Entry { price: 12, quantity: 1 },
        Entry { price: 12, quantity: 1 },
        Entry { price: 6, quantity: 1 },
    ],
);
assert_eq!(*db.snapshot().query::<Total>(&cart), 30);

// Inputs and intermediate values are stored inside `db`. In a long-running
// process, these add up. `db.gc()` clears out intermediate values. Note that
// this function, like `db.set_input()`, blocks until snapshots drop, so do
// not call it on the same thread as a live snapshot.
db.gc();
```

Don't take the comments above on faith:
[`examples/call_counts.rs`](https://github.com/bjogart/procrastinator/blob/trunk/crates/procrastinator/examples/call_counts.rs) runs both versions of this example with static counters.
Across all three carts, the free functions call `entry_total()` 12 times,
while `EntryTotal` is evaluated only thrice.
To see the counts for every step, run:

```sh
cargo run -p procrastinator --example call_counts
```

Each concept in Procrastinator maps to a familiar construct from regular Rust:

- An **input** is like a variable that is set from outside the computation.
  Implementing [`Input`] declares the type of its value.
  [`Db::new_input`] creates a new input and returns an [`InputId`], which queries take as an argument.
- A **query** is like a pure function.
  Implementing [`Query`] declares its argument and output types,
  and [`Query::eval`] is its body.
  Procrastinator assumes queries are pure.
  In other words, query results should depend only on their arguments and what they read through the snapshot.

  Here is a short and non-exhaustive list of items that should not affect a query's behavior:

  - mutable static variables
  - interior mutability
  - randomness
  - time
  - environment variables
  - the file system

  If you need any of these, they should be [`Input`]s!
- The **[`Db`]** owns inputs and memoized results.
  It is the only way to modify inputs, through [`Db::set_input`].
- A **[`Snapshot`]** is a read-only view of the `Db`, from which queries are run.
  Each thread uses its own snapshot.
- Calling **[`Snapshot::query`]** is like calling a function.
  It returns a handle that dereferences to the output.

[`Input`]: https://docs.rs/procrastinator/latest/procrastinator/query/trait.Input.html
[`InputId`]: https://docs.rs/procrastinator/latest/procrastinator/query/struct.InputId.html
[`Query`]: https://docs.rs/procrastinator/latest/procrastinator/query/trait.Query.html
[`Query::eval`]: https://docs.rs/procrastinator/latest/procrastinator/query/trait.Query.html#tymethod.eval
[`Db`]: https://docs.rs/procrastinator/latest/procrastinator/struct.Db.html
[`Db::new_input`]: https://docs.rs/procrastinator/latest/procrastinator/struct.Db.html#method.new_input
[`Db::set_input`]: https://docs.rs/procrastinator/latest/procrastinator/struct.Db.html#method.set_input
[`Snapshot`]: https://docs.rs/procrastinator/latest/procrastinator/struct.Snapshot.html
[`Snapshot::query`]: https://docs.rs/procrastinator/latest/procrastinator/struct.Snapshot.html#method.query

## Related crates

Procrastinator is not the only crate in this space, or the first.
Consider this crate if you value a small crate that is easy to learn and easy to understand.
Queries and inputs are plain trait implementations,
and it has no code generation at build time.
Query calls are easily parallelizable,
cancellation is panic-free,
and it provides the opportunity for custom storage structs and profiling handlers.
In exchange, Procrastinator may offer less functionality than the crates below:

- [Salsa](https://github.com/salsa-rs/salsa) is arguably the most popular incremental computation crate in Rust,
  and is the closest relative of this crate conceptually.
  It defines queries and data with attribute macros.
  Salsa supports features that Procrastinator does not,
  including durability levels,
  interned and tracked structs,
  accumulators,
  LRU eviction of memoized values,
  and recovery from query cycles.
  Consider Salsa if your project is large enough to benefit from those features.
- [comemo](https://github.com/typst/comemo) takes a different approach:
  constrained memoization of existing functions.
  Attribute macros mark functions to memoize and types to track.
  Consider comemo if you would like to add incrementality to existing code without structuring it around a database.

## Contributing

Contributions, questions, and pull requests are welcome!

Proposals are evaluated against the goals of this crate:

- It stays small.
- It stays accessible to users and contributors alike.
- It uses no macros in user-facing APIs and no `unsafe` code.
- It uses no non-local control flow:
  for example, cancellation does not rely on panics or unwinding.

CI is intentionally strict.
Among other things, it enables most Clippy lints and treats warnings as errors.
It runs on pull requests, and you can run the same checks locally with:

```sh
cargo xtask test
```

This requires [`rg`](https://github.com/BurntSushi/ripgrep)
and [`taplo`](https://github.com/tamasfe/taplo) to be available on your `PATH`.

## License

Procrastinator is licensed under either of:

- [Apache License, Version 2.0](https://github.com/bjogart/procrastinator/blob/trunk/LICENSE-APACHE)
- [MIT license](https://github.com/bjogart/procrastinator/blob/trunk/LICENSE-MIT)

at your option.

Unless you explicitly state otherwise,
any contribution intentionally submitted for inclusion in this crate by you,
as defined in the Apache-2.0 license,
shall be dual licensed as above,
without any additional terms or conditions.