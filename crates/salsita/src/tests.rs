use crate::Db;
use crate::Snapshot;
use crate::event;
use crate::event::PerfHandler;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use crate::storage::DefaultStorage;
use crate::storage::Storage;
use core::cell::Cell;
use core::fmt::Debug;
use core::hash::Hash;
use core::hash::Hasher;
use core::ops::Sub;
use std::panic;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

#[derive(PartialEq, Eq, Default, Debug)]
struct Counts {
    query_count: usize,
    eval_count: usize,
    registered_query_ops: isize,
    stored_values: isize,
    memo_count: isize,
}

#[test]
fn db_starts_empty() {
    assert_eq!(
        Counts::new(&Db::<DefaultStorage, PerfHandler>::default().snapshot()),
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 0,
            stored_values: 0,
            memo_count: 0
        }
    );
}

#[test]
fn queries_are_memoized_after_first_call() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
}

#[test]
fn only_dependent_queries_recompute_on_input_change() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    additional_input_queries(&mut db, count, burrito_salsa);
}

#[test]
fn unchanged_outputs_stop_propagation() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    db.set_input(price, &4);
    db.set_input(count, &5);
    assert_queries(
        &db,
        price,
        count,
        burrito_salsa,
        (
            6,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 1,
                memo_count: 0,
            },
        ),
        (
            30,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 0,
                memo_count: 0,
            },
        ),
        (
            35,
            Counts {
                query_count: 1,
                eval_count: 0,
                registered_query_ops: 0,
                stored_values: 0,
                memo_count: 0,
            },
        ),
        (
            200,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 1,
                memo_count: 0,
            },
        ),
    );
}

#[test]
fn propagation_updates_transitive_dependents() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    assert_query_delta::<PriceWithVat>(
        &db,
        &(price, count),
        &35,
        Counts {
            query_count: 1,
            eval_count: 0,
            registered_query_ops: 0,
            stored_values: 0,
            memo_count: 0,
        },
    );
    db.set_input(price, &4);
    assert_query_delta::<PriceWithVat>(
        &db,
        &(price, count),
        &23,
        Counts {
            query_count: 5,
            eval_count: 3,
            registered_query_ops: 0,
            stored_values: 3,
            memo_count: 0,
        },
    );
}

#[test]
fn modifications_are_blocked_until_snapshots_drop() {
    let mut db: Db = Db::default();
    let (sender, receiver) = mpsc::channel::<Snapshot>();
    let price = db.new_input::<BurritoPrice>(&8);
    // Sanity check: on the main thread we observe the value we just created.
    assert_eq!(*db.snapshot().query::<BurritoPrice>(&price), 8);
    let handle = thread::spawn({
        // Create one snapshot, which moves to the other thread, where it will
        // remain live until we drop it.
        let snapshot = db.snapshot();
        move || {
            {
                let snapshot = snapshot;
                // While this snapshot is live, `set_input` blocks &and will not
                // modify the input.
                assert_eq!(*snapshot.query::<BurritoPrice>(&price), 8);
                // Snapshot is dropped here; meaning that `set_input` is &free to
                // modify its input.
            }
            let snapshot = receiver.recv().unwrap();
            // The new snapshot sees the updated input.
            assert_eq!(*snapshot.query::<BurritoPrice>(&price), 4);
        }
    });
    db.set_input(price, &4);
    sender.send(db.snapshot()).unwrap();
    handle.join().unwrap();
}

#[test]
fn modifications_trigger_query_cancellation() {
    let mut db: Db = Db::default();
    let worker_ready = Arc::new((Mutex::new(false), Condvar::new()));
    let price = db.new_input::<BurritoPrice>(&8);
    // Spawn a worker thread that holds a live snapshot. While this snapshot
    // exists, calling `set_input()` &in the main thread should block and trigger
    // cancellation inside the worker thread.
    let handle = thread::spawn({
        let snapshot = db.snapshot();
        let worker_ready = worker_ready.clone();
        move || {
            // Sanity check: before any cancellation, the query evaluates as
            // expected.
            assert_eq!(*snapshot.query::<BurritoPriceWithShipping>(&price), (10));
            // Notify the main thread that the worker is ready.
            let (mutex, cvar) = &*worker_ready;
            *mutex.lock().unwrap() = true;
            cvar.notify_one();
            // Wait for cancellation to be signalled, which is the signal that
            // `set_input` is &blocking.
            while !snapshot.should_cancel() {
                thread::yield_now();
            }
            // Calling a query while the database canceling automatically
            // returns a memoized value and will not evaluate.
            assert_eq!(*snapshot.query::<BurritoPriceWithShipping>(&price), (10));
        }
    });
    // Wait until the worker is ready.
    let (mutex, cvar) = &*worker_ready;
    let mut guard = mutex.lock().unwrap();
    while !*guard {
        guard = cvar.wait(guard).unwrap();
    }
    // With the worker snapshot still alive, this call will: set the global
    // cancellation flag and block until the worker snapshot is dropped.
    db.set_input(price, &4);
    // Now that the new value for `price` is set successfully, calling the same
    // query will return an updated value.
    assert_eq!(
        *db.snapshot().query::<BurritoPriceWithShipping>(&price),
        (6)
    );
    // Join and unwrap the worker thread to propagate failed assertions.
    handle.join().unwrap();
}

#[test]
fn cancelled_eval_does_not_memoize_stale_deps() {
    let mut db: Db = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    let count = db.new_input::<BurritoCount>(&1);
    // Verify `BurritoPriceWithShipping` once, then change its input so that it
    // becomes stale.
    assert_eq!(*db.snapshot().query::<BurritoPriceWithShipping>(&price), 10);
    db.set_input(price, &4);
    let handle = thread::spawn({
        let snapshot = db.snapshot();
        move || {
            while !snapshot.should_cancel() {
                thread::yield_now();
            }
            // `TotalPrice` is new and therefore evaluated, but its stale
            // dependency returns early due to cancellation.
            snapshot.query::<TotalPrice>(&(price, count));
        }
    });
    // Setting an equal value triggers cancellation without bumping the
    // revision.
    db.set_input(price, &4);
    handle.join().unwrap();
    assert_eq!(*db.snapshot().query::<TotalPrice>(&(price, count)), 6);
}

#[test]
fn cancelled_dep_does_not_mark_caller_verified() {
    let mut db: Db = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    let count = db.new_input::<BurritoCount>(&1);
    // Verify `StallingTotalPrice` once, then change its input so that it
    // becomes stale.
    assert_eq!(
        *db.snapshot().query::<StallingTotalPrice>(&(price, count)),
        10
    );
    db.set_input(price, &4);
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn({
        let snapshot = db.snapshot();
        move || {
            // `StallingTotalPrice` is not cancelled when its own memo is
            // checked, but its dependency is re-evaluated during cancellation.
            STALL_EVAL_UNTIL_CANCELLED.set(Some(sender));
            snapshot.query::<StallingTotalPrice>(&(price, count));
        }
    });
    // Wait until the worker is evaluating the dependency.
    receiver.recv().unwrap();
    // Setting an equal value triggers cancellation without bumping the
    // revision.
    db.set_input(price, &4);
    handle.join().unwrap();
    assert_eq!(
        *db.snapshot().query::<StallingTotalPrice>(&(price, count)),
        6
    );
}

#[test]
fn garbage_collection_reclaims_unused_ops_and_storage_and_memos() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    additional_input_queries(&mut db, count, burrito_salsa);

    let before = Counts::new(&db.snapshot());
    db.gc();
    let after = Counts::new(&db.snapshot());
    assert_eq!(
        after - before,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: -4,
            stored_values: -10,
            memo_count: -7
        }
    );

    let before = Counts::new(&db.snapshot());
    init_queries(&db, price, count, burrito_salsa);
    let after = Counts::new(&db.snapshot());
    assert_eq!(
        after - before,
        Counts {
            query_count: 10,
            eval_count: 4,
            registered_query_ops: 4,
            stored_values: 6,
            memo_count: 4
        }
    );
}

#[test]
fn unchanged_reevaluation_records_new_deps() {
    let mut db: Db = Db::default();
    let pickup = db.new_input::<IsPickup>(&true);
    let fee = db.new_input::<DeliveryFee>(&5);
    // Pickup orders don't read the delivery fee.
    assert_eq!(*db.snapshot().query::<DeliveryCost>(&(pickup, fee)), 0);
    // Switching to delivery makes the query read the fee, but the output is
    // still 0, so the result is unchanged.
    db.set_input(pickup, &false);
    db.set_input(fee, &0);
    assert_eq!(*db.snapshot().query::<DeliveryCost>(&(pickup, fee)), 0);
    // The fee is now a dependency, so adjustments should be reflected in the
    // result.
    db.set_input(fee, &7);
    assert_eq!(*db.snapshot().query::<DeliveryCost>(&(pickup, fee)), 7);
}

#[test]
fn unchanged_nested_reevaluation_preserves_caller_deps() {
    let mut db: Db = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    let pickup = db.new_input::<IsPickup>(&true);
    let fee = db.new_input::<DeliveryFee>(&5);
    let args = (price, pickup, fee);
    assert_eq!(*db.snapshot().query::<DeliveredBurritoPrice>(&args), 8);
    // `DeliveredBurritoPrice` re-evaluates because the price changed, and
    // `DeliveryCost` re-evaluates to an unchanged value.
    db.set_input(price, &9);
    db.set_input(pickup, &false);
    db.set_input(fee, &0);
    assert_eq!(*db.snapshot().query::<DeliveredBurritoPrice>(&args), 9);
    // Price must still be tracked as a dependency of the caller.
    db.set_input(price, &10);
    assert_eq!(*db.snapshot().query::<DeliveredBurritoPrice>(&args), 10);
}

#[test]
fn verified_nested_deps_are_not_tracked_by_caller() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    let pickup = db.new_input::<IsPickup>(&true);
    let fee = db.new_input::<DeliveryFee>(&0);
    let args = (price, pickup, fee);
    assert_eq!(*db.snapshot().query::<DeliveryCost>(&(pickup, fee)), 0);
    // Bump the revision so that `DeliveredBurritoPrice` verifies the existing
    // `DeliveryCost` memo, rather than evaluating it.
    db.set_input(price, &9);
    assert_eq!(*db.snapshot().query::<DeliveredBurritoPrice>(&args), 9);
    // `DeliveryCost` re-evaluates to an unchanged value. Since the caller
    // depends on `IsPickup` only through `DeliveryCost`, it should not
    // re-evaluate.
    db.set_input(pickup, &false);
    assert_query_delta::<DeliveredBurritoPrice>(
        &db,
        &args,
        &9,
        Counts {
            query_count: 3,
            eval_count: 1,
            registered_query_ops: 0,
            stored_values: 0,
            memo_count: 0,
        },
    );
}

#[test]
fn garbage_collection_handles_derived_queries_without_deps() {
    let mut db: Db = Db::default();
    assert_eq!(*db.snapshot().query::<SalsaCupGrams>(&3), 120);
    db.gc();
    assert_eq!(*db.snapshot().query::<SalsaCupGrams>(&3), 120);
}

#[test]
fn inputs_can_be_set_after_garbage_collection() {
    let mut db: Db = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    // Store a derived value between the two inputs, so that collecting it moves
    // the second input to a different storage ID.
    assert_eq!(*db.snapshot().query::<BurritoPriceWithShipping>(&price), 10);
    let count = db.new_input::<BurritoCount>(&3);
    db.gc();
    db.set_input(count, &5);
    assert_eq!(*db.snapshot().query::<BurritoCount>(&count), 5);
    assert_eq!(*db.snapshot().query::<BurritoPrice>(&price), 8);
}

#[test]
fn garbage_collection_deduplicates_equal_inputs() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    // These inputs are deduplicated by the runtime, so these inputs point to
    // the same value.
    let price = db.new_input::<BurritoPrice>(&3);
    let count = db.new_input::<BurritoCount>(&3);
    // Garbage collection should recognize that both inputs point to the same
    // value and assign them the same storage ID.
    db.gc();
    assert_eq!(*db.snapshot().query::<TotalPrice>(&(price, count)), 15);
    // Set both inputs to the same value.
    db.set_input(price, &3);
    db.set_input(count, &3);
    // Both inputs, if deduplicated, point to the same storage slot. Hence
    // `eval_count == 0` and the result comes straight from the cache.
    assert_query_delta::<TotalPrice>(
        &db,
        &(price, count),
        &15,
        Counts {
            query_count: 1,
            eval_count: 0,
            registered_query_ops: 0,
            stored_values: 0,
            memo_count: 0,
        },
    );
}

#[test]
fn default_storage_insertion_cannot_cannot_race() {
    let storage = &DefaultStorage::default();
    let (stalled_sender, stalled_receiver) = mpsc::channel();
    let (resume_sender, resume_receiver) = mpsc::channel();
    let (id, stalled_id) = thread::scope(|scope| {
        let stalled = scope.spawn(move || {
            STALL_CLONE.set(Some((stalled_sender, resume_receiver)));
            storage.store(&(), &StallingClone(3))
        });
        // The other thread did not find the value in storage and is now cloning
        // it before insertion. Store an equal value in the meantime.
        stalled_receiver.recv().unwrap();
        let id = storage.store(&(), &StallingClone(3));
        resume_sender.send(()).unwrap();
        (id, stalled.join().unwrap())
    });
    assert_eq!(id, stalled_id);
}

#[test]
fn panicking_garbage_collection_leaves_db_unchanged() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    // Store a derived value between the two inputs, so that collecting it moves
    // the second input to a different storage ID.
    assert_eq!(*db.snapshot().query::<BurritoPriceWithShipping>(&price), 10);
    let count = db.new_input::<FragileCount>(&Fragile(3));
    assert_eq!(
        *db.snapshot().query::<FragileTotalPrice>(&(price, count)),
        30
    );
    // The first input transfers successfully before the second one panics.
    let before = Counts::new(&db.snapshot());
    PANIC_ON_HASH.set(true);
    let result = panic::catch_unwind(AssertUnwindSafe(|| db.gc()));
    PANIC_ON_HASH.set(false);
    assert!(result.is_err());
    assert_eq!(Counts::new(&db.snapshot()) - before, Counts::default());
    // Memos survived, so the derived query is not re-evaluated.
    assert_query_delta::<FragileTotalPrice>(
        &db,
        &(price, count),
        &30,
        Counts {
            query_count: 1,
            eval_count: 0,
            registered_query_ops: 0,
            stored_values: 0,
            memo_count: 0,
        },
    );
    db.set_input(count, &Fragile(2));
    assert_eq!(
        *db.snapshot().query::<FragileTotalPrice>(&(price, count)),
        20
    );
    // Subsequent garbage collection succeeds.
    db.gc();
    db.set_input(price, &4);
    assert_eq!(
        *db.snapshot().query::<FragileTotalPrice>(&(price, count)),
        12
    );
}

#[test]
fn panicking_eval_of_new_memo_is_retried() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    let count = db.new_input::<BurritoCount>(&3);
    PANIC_IN_EVAL.set(true);
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        db.snapshot().query::<FlakyTotalPrice>(&(price, count));
    }));
    PANIC_IN_EVAL.set(false);
    assert!(result.is_err());
    // The memo has no value, so it must be evaluated again in the same
    // revision.
    assert_query_delta::<FlakyTotalPrice>(
        &db,
        &(price, count),
        &24,
        Counts {
            query_count: 3,
            eval_count: 1,
            registered_query_ops: 0,
            stored_values: 1,
            memo_count: 0,
        },
    );
}

#[test]
fn panicking_reevaluation_does_not_keep_stale_value() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    let count = db.new_input::<BurritoCount>(&3);
    assert_eq!(*db.snapshot().query::<FlakyTotalPrice>(&(price, count)), 24);
    db.set_input(count, &5);
    // Re-evaluation panics after reading the price, but before reading the
    // count.
    PANIC_IN_EVAL.set(true);
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        db.snapshot().query::<FlakyTotalPrice>(&(price, count));
    }));
    PANIC_IN_EVAL.set(false);
    assert!(result.is_err());
    // The previous value remains stale, so it must be evaluated again.
    assert_query_delta::<FlakyTotalPrice>(
        &db,
        &(price, count),
        &40,
        Counts {
            query_count: 3,
            eval_count: 1,
            registered_query_ops: 0,
            stored_values: 1,
            memo_count: 0,
        },
    );
    // Both price and count should be recorded as dependencies after evaluation.
    db.set_input(count, &2);
    assert_eq!(*db.snapshot().query::<FlakyTotalPrice>(&(price, count)), 16);
}

#[test]
fn panicking_nested_eval_is_retried_by_caller() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    let count = db.new_input::<BurritoCount>(&3);
    // The callee panics, which unwinds through the caller's evaluation.
    PANIC_IN_EVAL.set(true);
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        db.snapshot().query::<FlakyPriceWithVat>(&(price, count));
    }));
    PANIC_IN_EVAL.set(false);
    assert!(result.is_err());
    // Neither memo has a value, so both must be evaluated again in the same
    // revision.
    assert_query_delta::<FlakyPriceWithVat>(
        &db,
        &(price, count),
        &29,
        Counts {
            query_count: 4,
            eval_count: 2,
            registered_query_ops: 0,
            stored_values: 2,
            memo_count: 0,
        },
    );
}

#[test]
fn panicking_hash_impls_cannot_trigger_during_storage_rehash() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let _count = db.new_input::<FragileCount>(&Fragile(3));
    // Storing unrelated values never hashes `Fragile` when looking them up, but
    // growing the storage index rehashes values stored so far.
    PANIC_ON_HASH.set(true);
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let snapshot = db.snapshot();
        for cups in 0..2 {
            snapshot.query::<SalsaCupGrams>(&cups);
        }
    }));
    PANIC_ON_HASH.set(false);
    assert!(result.is_ok());
}

fn init_inputs(
    db: &mut Db<DefaultStorage, PerfHandler>,
) -> (
    InputId<BurritoPrice>,
    InputId<BurritoCount>,
    InputId<SalsaPerBurrito>,
) {
    let price = assert_new_input(
        db,
        8,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 1,
            stored_values: 2,
            memo_count: 1,
        },
    );
    let count = assert_new_input::<BurritoCount>(
        db,
        3,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 1,
            stored_values: 2,
            memo_count: 1,
        },
    );
    let burrito_salsa = assert_new_input::<SalsaPerBurrito>(
        db,
        40,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 1,
            stored_values: 2,
            memo_count: 1,
        },
    );
    (price, count, burrito_salsa)
}

fn init_queries(
    db: &Db<DefaultStorage, PerfHandler>,
    price: InputId<BurritoPrice>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
) {
    assert_queries(
        &db,
        price,
        count,
        burrito_salsa,
        (
            10,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 1,
                stored_values: 1,
                memo_count: 1,
            },
        ),
        (
            30,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 1,
                stored_values: 2,
                memo_count: 1,
            },
        ),
        (
            35,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 1,
                stored_values: 1,
                memo_count: 1,
            },
        ),
        (
            120,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 1,
                stored_values: 2,
                memo_count: 1,
            },
        ),
    );
}

fn additional_input_queries(
    db: &mut Db<DefaultStorage, PerfHandler>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
) {
    let discount_price = assert_new_input::<BurritoPrice>(
        db,
        4,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 0,
            stored_values: 2,
            memo_count: 1,
        },
    );
    assert_queries(
        &db,
        discount_price,
        count,
        burrito_salsa,
        (
            6,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 1,
                memo_count: 1,
            },
        ),
        (
            18,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 2,
                memo_count: 1,
            },
        ),
        (
            23,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 1,
                memo_count: 1,
            },
        ),
        (
            120,
            Counts {
                query_count: 1,
                eval_count: 0,
                registered_query_ops: 0,
                stored_values: 0,
                memo_count: 0,
            },
        ),
    );
}

fn assert_new_input<I>(
    db: &mut Db<DefaultStorage, PerfHandler>,
    value: I::Value,
    counts: Counts,
) -> InputId<I>
where
    I: Input,
{
    let before = Counts::new(&db.snapshot());
    let input_id = db.new_input::<I>(&value);
    let after = Counts::new(&db.snapshot());
    assert_eq!(after - before, counts);
    input_id
}

fn assert_queries(
    db: &Db<DefaultStorage, PerfHandler>,
    price: InputId<BurritoPrice>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
    price_w_shipping: (usize, Counts),
    total_price: (usize, Counts),
    price_with_vat: (usize, Counts),
    salsa_in_order: (usize, Counts),
) {
    assert_query_delta::<BurritoPriceWithShipping>(
        db,
        &price,
        &(price_w_shipping.0),
        price_w_shipping.1,
    );
    assert_query_delta::<TotalPrice>(db, &(price, count), &total_price.0, total_price.1);
    assert_query_delta::<PriceWithVat>(db, &(price, count), &(price_with_vat.0), price_with_vat.1);
    assert_query_delta::<SalsaInOrder>(
        db,
        &(burrito_salsa, count),
        &(salsa_in_order.0),
        salsa_in_order.1,
    );
}

fn assert_query_delta<Q>(
    db: &Db<DefaultStorage, PerfHandler>,
    args: &Q::Args,
    exp_out: &Q::Out,
    d_counts: Counts,
) where
    Q: Query,
    Q::Out: Eq + Debug,
{
    let snapshot = db.snapshot();
    let before = Counts::new(&snapshot);
    let out = snapshot.query::<Q>(args);
    assert_eq!(out.as_ref(), exp_out);
    let after = Counts::new(&snapshot);
    assert_eq!(after - before, d_counts);
}

impl Counts {
    fn new(snapshot: &Snapshot<DefaultStorage, PerfHandler>) -> Self {
        let handler = snapshot.event_handler();
        Self {
            query_count: handler.query_count(),
            eval_count: handler.eval_count(),
            registered_query_ops: handler.registered_query_ops().try_into().unwrap(),
            stored_values: handler.stored_values().try_into().unwrap(),
            memo_count: handler.memo_count().try_into().unwrap(),
        }
    }
}

impl Sub for Counts {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            query_count: self.query_count - rhs.query_count,
            eval_count: self.eval_count - rhs.eval_count,
            registered_query_ops: self.registered_query_ops - rhs.registered_query_ops,
            stored_values: self.stored_values - rhs.stored_values,
            memo_count: self.memo_count - rhs.memo_count,
        }
    }
}

struct BurritoPrice;
impl Input for BurritoPrice {
    type Value = usize;
}

struct BurritoPriceWithShipping;
impl Query for BurritoPriceWithShipping {
    type Args = InputId<BurritoPrice>;
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        *snapshot.query::<BurritoPrice>(args) + 2
    }
}

struct BurritoCount;
impl Input for BurritoCount {
    type Value = usize;
}

struct TotalPrice;
impl Query for TotalPrice {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (price, count) = args;
        *snapshot.query::<BurritoPriceWithShipping>(price) * *snapshot.query::<BurritoCount>(count)
    }
}

struct PriceWithVat;
impl Query for PriceWithVat {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        (*snapshot.query::<TotalPrice>(args)) + 5
    }
}

struct SalsaPerBurrito;
impl Input for SalsaPerBurrito {
    type Value = usize;
}

struct SalsaInOrder;
impl Query for SalsaInOrder {
    type Args = (InputId<SalsaPerBurrito>, InputId<BurritoCount>);
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (burrito_salsa, count) = args;
        *snapshot.query::<BurritoCount>(count) * *snapshot.query::<SalsaPerBurrito>(burrito_salsa)
    }
}

struct IsPickup;
impl Input for IsPickup {
    type Value = bool;
}

struct DeliveryFee;
impl Input for DeliveryFee {
    type Value = usize;
}

struct DeliveryCost;
impl Query for DeliveryCost {
    type Args = (InputId<IsPickup>, InputId<DeliveryFee>);
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (pickup, fee) = args;
        if *snapshot.query::<IsPickup>(pickup) {
            0
        } else {
            *snapshot.query::<DeliveryFee>(fee)
        }
    }
}

struct DeliveredBurritoPrice;
impl Query for DeliveredBurritoPrice {
    type Args = (
        InputId<BurritoPrice>,
        InputId<IsPickup>,
        InputId<DeliveryFee>,
    );
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (price, pickup, fee) = args;
        *snapshot.query::<BurritoPrice>(price) + *snapshot.query::<DeliveryCost>(&(*pickup, *fee))
    }
}

struct SalsaCupGrams;
impl Query for SalsaCupGrams {
    type Args = usize;
    type Out = usize;

    fn eval<S, H>(_: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        args * 40
    }
}

thread_local! {
    static PANIC_ON_HASH: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Fragile(usize);

impl Hash for Fragile {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        assert!(
            !PANIC_ON_HASH.get(),
            "injected panic while hashing `Fragile`"
        );
        self.0.hash(state);
    }
}

struct FragileCount;
impl Input for FragileCount {
    type Value = Fragile;
}

struct FragileTotalPrice;
impl Query for FragileTotalPrice {
    type Args = (InputId<BurritoPrice>, InputId<FragileCount>);
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (price, count) = args;
        *snapshot.query::<BurritoPriceWithShipping>(price) * snapshot.query::<FragileCount>(count).0
    }
}

thread_local! {
    static PANIC_IN_EVAL: Cell<bool> = const { Cell::new(false) };
}

struct FlakyTotalPrice;
impl Query for FlakyTotalPrice {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (price, count) = args;
        let price = *snapshot.query::<BurritoPrice>(price);
        assert!(!PANIC_IN_EVAL.get(), "injected panic in `FlakyTotalPrice`");
        price * *snapshot.query::<BurritoCount>(count)
    }
}

struct FlakyPriceWithVat;
impl Query for FlakyPriceWithVat {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        *snapshot.query::<FlakyTotalPrice>(args) + 5
    }
}

thread_local! {
    static STALL_CLONE: Cell<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>> = const { Cell::new(None) };
}

#[derive(PartialEq, Eq, Hash, Debug)]
struct StallingClone(usize);

impl Clone for StallingClone {
    fn clone(&self) -> Self {
        if let Some((stalled, resume)) = STALL_CLONE.take() {
            stalled.send(()).unwrap();
            resume.recv().unwrap();
        }
        Self(self.0)
    }
}

thread_local! {
    static STALL_EVAL_UNTIL_CANCELLED: Cell<Option<mpsc::Sender<()>>> = const { Cell::new(None) };
}

struct StallingPriceWithShipping;
impl Query for StallingPriceWithShipping {
    type Args = InputId<BurritoPrice>;
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        if let Some(sender) = STALL_EVAL_UNTIL_CANCELLED.take() {
            sender.send(()).unwrap();
            while !snapshot.should_cancel() {
                thread::yield_now();
            }
        }
        *snapshot.query::<BurritoPrice>(args) + 2
    }
}

struct StallingTotalPrice;
impl Query for StallingTotalPrice {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (price, count) = args;
        *snapshot.query::<StallingPriceWithShipping>(price) * *snapshot.query::<BurritoCount>(count)
    }
}
