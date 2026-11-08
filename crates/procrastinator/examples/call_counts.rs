//! Counts how often each step of the README's toy example runs, once with free
//! functions and once with Procrastinator queries.
//!
//! Every count is printed and also asserted, so this example fails if the
//! claims about caching in the README stop being true.

#![expect(
    clippy::arithmetic_side_effects,
    reason = "Matches the README. Values are small constants."
)]
#![expect(
    clippy::missing_assert_message,
    reason = "Matches the README. Failing assertions print both values."
)]
#![expect(
    clippy::renamed_function_params,
    reason = "Matches the README. Argument names are more descriptive than `args`."
)]
#![expect(
    unused_crate_dependencies,
    reason = "Examples inherit crate dependencies."
)]

use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

mod free_functions {
    use crate::Calls;
    use crate::Entry;
    use crate::MUG;
    use crate::SHIRT;
    use crate::SHIRTS;
    use core::sync::atomic::Ordering;

    static CALLS: Calls = Calls::new();

    fn entry_total(entry: &Entry) -> usize {
        CALLS.entry_total.fetch_add(1, Ordering::Relaxed);
        entry.price * entry.quantity
    }

    fn subtotal(cart: &[Entry]) -> usize {
        CALLS.subtotal.fetch_add(1, Ordering::Relaxed);
        cart.iter().map(entry_total).sum()
    }

    fn shipping(cart: &[Entry]) -> usize {
        CALLS.shipping.fetch_add(1, Ordering::Relaxed);
        if subtotal(cart) < 25 { 10 } else { 0 }
    }

    fn total(cart: &[Entry]) -> usize {
        CALLS.total.fetch_add(1, Ordering::Relaxed);
        subtotal(cart) + shipping(cart)
    }

    pub(crate) fn run() {
        let mut cart = vec![SHIRTS];
        assert_eq!(total(&cart), 34);
        CALLS.report("Free functions: 2 of the same T-shirt", [2, 2, 1, 1]);

        cart = vec![SHIRT, SHIRT];
        assert_eq!(total(&cart), 34);
        CALLS.report("Free functions: swap a T-shirt", [4, 2, 1, 1]);

        cart = vec![SHIRT, SHIRT, MUG];
        assert_eq!(total(&cart), 30);
        CALLS.report("Free functions: add the mug", [6, 2, 1, 1]);
    }
}

mod queries {
    use crate::Calls;
    use crate::Entry;
    use crate::MUG;
    use crate::SHIRT;
    use crate::SHIRTS;
    use core::sync::atomic::Ordering;
    use procrastinator::Db;
    use procrastinator::Snapshot;
    use procrastinator::handler::Handler;
    use procrastinator::query::Input;
    use procrastinator::query::InputId;
    use procrastinator::query::Query;
    use procrastinator::storage::Storage;

    // Queries should not touch mutable statics. These counters are the
    // exception that proves the rule: they only observe evaluations, and they
    // never affect a query's result.
    static CALLS: Calls = Calls::new();

    struct Cart;

    struct EntryTotal;

    struct Subtotal;

    struct Shipping;

    struct Total;

    impl Input for Cart {
        type Value = Vec<Entry>;
    }

    impl Query for EntryTotal {
        type Args = Entry;
        type Out = usize;

        fn eval<S, H>(_snapshot: &Snapshot<S, H>, entry: &Self::Args) -> Self::Out
        where
            S: Storage,
            H: Handler,
        {
            CALLS.entry_total.fetch_add(1, Ordering::Relaxed);
            entry.price * entry.quantity
        }
    }

    impl Query for Subtotal {
        type Args = InputId<Cart>;
        type Out = usize;

        fn eval<S, H>(snapshot: &Snapshot<S, H>, cart: &Self::Args) -> Self::Out
        where
            S: Storage,
            H: Handler,
        {
            CALLS.subtotal.fetch_add(1, Ordering::Relaxed);
            let entries = snapshot.query::<Cart>(cart);
            entries
                .iter()
                .map(|entry| *snapshot.query::<EntryTotal>(entry))
                .sum()
        }
    }

    impl Query for Shipping {
        type Args = InputId<Cart>;
        type Out = usize;

        fn eval<S, H>(snapshot: &Snapshot<S, H>, cart: &Self::Args) -> Self::Out
        where
            S: Storage,
            H: Handler,
        {
            CALLS.shipping.fetch_add(1, Ordering::Relaxed);
            if *snapshot.query::<Subtotal>(cart) < 25 {
                10
            } else {
                0
            }
        }
    }

    impl Query for Total {
        type Args = InputId<Cart>;
        type Out = usize;

        fn eval<S, H>(snapshot: &Snapshot<S, H>, cart: &Self::Args) -> Self::Out
        where
            S: Storage,
            H: Handler,
        {
            CALLS.total.fetch_add(1, Ordering::Relaxed);
            let subtotal = *snapshot.query::<Subtotal>(cart);
            let shipping = *snapshot.query::<Shipping>(cart);
            subtotal + shipping
        }
    }

    pub(crate) fn run() {
        let mut db: Db = Db::default();

        let cart = db.new_input::<Cart>(&vec![SHIRTS]);
        assert_eq!(*db.snapshot().query::<Total>(&cart), 34);
        CALLS.report("Procrastinator: 2 of the same T-shirt", [1, 1, 1, 1]);

        // The second entry is cached, and `Subtotal` does not change, so
        // `Shipping` and `Total` are not evaluated at all.
        db.set_input(cart, &vec![SHIRT, SHIRT]);
        assert_eq!(*db.snapshot().query::<Total>(&cart), 34);
        CALLS.report("Procrastinator: swap a T-shirt", [1, 1, 0, 0]);

        // Only the mug's entry is new. `Subtotal` changes this time, so
        // `Shipping` and `Total` are evaluated again.
        db.set_input(cart, &vec![SHIRT, SHIRT, MUG]);
        assert_eq!(*db.snapshot().query::<Total>(&cart), 30);
        CALLS.report("Procrastinator: add the mug", [1, 1, 1, 1]);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn call_counts_match_readme() {
        crate::main();
    }
}

const SHIRTS: Entry = Entry {
    price: 12,
    quantity: 2,
};
const SHIRT: Entry = Entry {
    price: 12,
    quantity: 1,
};
const MUG: Entry = Entry {
    price: 6,
    quantity: 1,
};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct Entry {
    price: usize,
    quantity: usize,
}

/// Calls since the last [`Calls::report`], per step of the toy example.
#[derive(Debug)]
struct Calls {
    entry_total: AtomicUsize,
    subtotal: AtomicUsize,
    shipping: AtomicUsize,
    total: AtomicUsize,
}

impl Calls {
    const fn new() -> Self {
        Self {
            entry_total: AtomicUsize::new(0),
            subtotal: AtomicUsize::new(0),
            shipping: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
        }
    }

    /// Print the counters, reset them, and check them against `expected`, which
    /// lists `entry_total`, `subtotal`, `shipping` and `total` in that order.
    #[expect(
        clippy::print_stdout,
        reason = "Printing the counts is the point of this example."
    )]
    fn report(&self, step: &str, expected: [usize; 4]) {
        let Self {
            entry_total,
            subtotal,
            shipping,
            total,
        } = self;
        let counts = [entry_total, subtotal, shipping, total]
            .map(|counter| counter.swap(0, Ordering::Relaxed));
        let [entry_total, subtotal, shipping, total] = counts;
        println!(
            "{step:<38} entry_total: {entry_total}, subtotal: {subtotal}, shipping: {shipping}, total: {total}"
        );
        assert_eq!(counts, expected, "unexpected call counts: {step}");
    }
}

fn main() {
    free_functions::run();
    queries::run();
}
