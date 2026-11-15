#![expect(
    clippy::arithmetic_side_effects,
    reason = "Readability is preferred in tests. Side effects are unlikely since operations make heavy use of small constant values and assertions."
)]
#![expect(
    clippy::unwrap_used,
    reason = "Unwrap in tests indicates a failure state."
)]

use crate::Db;
use crate::Snapshot;
use crate::handler::Handler;
use crate::handler::perf;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use crate::storage::Storage;
use crate::storage::default::Storage as DefaultStorage;
use core::cell::Cell;
use core::fmt::Debug;
use core::hash::Hash;
use core::hash::Hasher;
use core::panic::AssertUnwindSafe;
use core::time::Duration;
use std::panic;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
struct Counts {
    query_count: u64,
    eval_count: u64,
    registered_query_ops: i64,
    stored_values: i64,
    memo_count: i64,
}

#[test]
fn db_starts_empty() {
    assert_eq!(
        Counts::from(
            Db::<DefaultStorage, perf::Handler>::default()
                .event_handler()
                .stats()
        ),
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
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
}

#[test]
fn only_dependent_queries_recompute_on_input_change() {
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    additional_input_queries(&mut db, count, burrito_salsa);
}

#[test]
fn unchanged_outputs_stop_propagation() {
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    db.set_input(price, &4);
    db.set_input(count, &5);
    assert_queries(
        &db,
        AssertQueriesArgs {
            price,
            count,
            burrito_salsa,
            price_w_shipping: QueryResult {
                value: 6,
                counts: Counts {
                    query_count: 2,
                    eval_count: 1,
                    registered_query_ops: 0,
                    stored_values: 1,
                    memo_count: 0,
                },
            },
            total_price: QueryResult {
                value: 30,
                counts: Counts {
                    query_count: 3,
                    eval_count: 1,
                    registered_query_ops: 0,
                    stored_values: 0,
                    memo_count: 0,
                },
            },
            price_with_vat: QueryResult {
                value: 35,
                counts: Counts {
                    query_count: 1,
                    eval_count: 0,
                    registered_query_ops: 0,
                    stored_values: 0,
                    memo_count: 0,
                },
            },
            salsa_in_order: QueryResult {
                value: 200,
                counts: Counts {
                    query_count: 3,
                    eval_count: 1,
                    registered_query_ops: 0,
                    stored_values: 1,
                    memo_count: 0,
                },
            },
        },
    );
}

#[test]
fn propagation_updates_transitive_dependents() {
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
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
                // While this snapshot is live, `set_input` blocks and will not
                // modify the input.
                assert_eq!(*snapshot.query::<BurritoPrice>(&price), 8);
                // Snapshot is dropped here; meaning that `set_input` is free to
                // modify its input.
            };
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
fn garbage_collection_reclaims_unused_ops_and_storage_and_memos() {
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    additional_input_queries(&mut db, count, burrito_salsa);

    let before = db.event_handler().stats();
    db.gc();
    let after = db.event_handler().stats();
    assert_eq!(
        Counts::from(after.since(before)),
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: -4,
            stored_values: -10,
            memo_count: -7
        }
    );

    let before = db.event_handler().stats();
    init_queries(&db, price, count, burrito_salsa);
    let after = db.event_handler().stats();
    assert_eq!(
        Counts::from(after.since(before)),
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
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
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
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
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
fn default_storage_insertion_cannot_race() {
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
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
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
    let before = db.event_handler().stats();
    PANIC_ON_HASH.set(true);
    let result = panic::catch_unwind(AssertUnwindSafe(|| db.gc()));
    PANIC_ON_HASH.set(false);
    assert!(result.is_err());
    assert_eq!(
        Counts::from(db.event_handler().stats().since(before)),
        Counts::default()
    );
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
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
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
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
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
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
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
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
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
    result.unwrap();
}

#[test]
fn perf_times_do_not_count_nested_scopes_twice() {
    let db: Db<DefaultStorage, perf::Handler> = Db::default();
    let snapshot = db.snapshot();
    let start = Instant::now();
    snapshot.query::<NestedNap>(&0);
    let wall = start.elapsed();
    let stats = snapshot.event_handler().stats();
    assert_eq!(stats.queries, 2);
    assert_eq!(stats.evals, 2);
    assert!(stats.eval_time >= NAP_DURATION);
    // Both query calls and both evaluations contain the nap. If any of them
    // included its nested scopes, the total would be at least twice the nap.
    assert!(stats.query_time() <= wall);
}

#[test]
fn perf_times_of_parallel_snapshots_do_not_interfere() {
    let db: Db<DefaultStorage, perf::Handler> = Db::default();
    let walls: Vec<Duration> = thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|worker| {
                let snapshot = db.snapshot();
                scope.spawn(move || {
                    let start = Instant::now();
                    snapshot.query::<NestedNap>(&worker);
                    start.elapsed()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect()
    });
    let stats = db.event_handler().stats();
    assert_eq!(stats.queries, 4);
    assert_eq!(stats.evals, 4);
    assert!(stats.eval_time >= NAP_DURATION * 2);
    assert!(stats.query_time() <= walls.iter().sum());
}

#[test]
fn perf_counts_panicking_scopes_and_stays_consistent() {
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    let count = db.new_input::<BurritoCount>(&3);
    let snapshot = db.snapshot();
    PANIC_IN_EVAL.set(true);
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        snapshot.query::<FlakyPriceWithVat>(&(price, count));
    }));
    PANIC_IN_EVAL.set(false);
    assert!(result.is_err());
    // `FlakyPriceWithVat` called `FlakyTotalPrice`, which read the price and
    // then panicked. All of these scopes ended, so all of them are counted.
    let unwound = snapshot.event_handler().stats();
    assert_eq!(unwound.queries, 3);
    assert_eq!(unwound.evals, 2);
    // The same snapshot keeps measuring correctly after unwinding.
    let start = Instant::now();
    snapshot.query::<FlakyPriceWithVat>(&(price, count));
    let wall = start.elapsed();
    let stats = snapshot.event_handler().stats().since(unwound);
    assert_eq!(stats.queries, 4);
    assert_eq!(stats.evals, 2);
    assert!(stats.query_time() <= wall);
}

#[test]
fn perf_tracks_db_operations() {
    let mut db: Db<DefaultStorage, perf::Handler> = Db::default();
    let price = db.new_input::<BurritoPrice>(&8);
    assert_eq!(*db.snapshot().query::<BurritoPriceWithShipping>(&price), 10);
    let before = db.event_handler().stats();
    let start = Instant::now();
    db.set_input(price, &4);
    db.gc();
    let wall = start.elapsed();
    let stats = db.event_handler().stats().since(before);
    assert_eq!(stats.queries, 0);
    assert_eq!(stats.evals, 0);
    assert_eq!(stats.set_inputs, 1);
    assert_eq!(stats.gcs, 1);
    assert!(stats.set_input_time + stats.gc_time + stats.wait_time <= wall);
    // Garbage collection keeps the input, and removes the derived query.
    assert_eq!(
        stats.query_ops,
        perf::Count {
            added: 0,
            removed: 1
        }
    );
    assert_eq!(
        stats.memos,
        perf::Count {
            added: 0,
            removed: 1
        }
    );
    // The new input value was added. The old input value and the output of
    // the derived query were removed.
    assert_eq!(
        stats.values,
        perf::Count {
            added: 1,
            removed: 2
        }
    );
    assert_eq!(stats.values.net(), -1);
}

#[test]
fn perf_stats_subtraction_saturates() {
    let later = perf::Stats {
        queries: 3,
        eval_time: NAP_DURATION,
        memos: perf::Count {
            added: 1,
            removed: 4,
        },
        ..perf::Stats::default()
    };
    assert_eq!(later.since(perf::Stats::default()), later);
    assert_eq!(perf::Stats::default().since(later), perf::Stats::default());
    assert_eq!(later.memos.net(), -3);
    let max = perf::Count {
        added: u64::MAX,
        removed: 0,
    };
    assert_eq!(max.net(), i64::MAX);
    let min = perf::Count {
        added: 0,
        removed: u64::MAX,
    };
    assert_eq!(min.net(), i64::MIN);
}

fn init_inputs(
    db: &mut Db<DefaultStorage, perf::Handler>,
) -> (
    InputId<BurritoPrice>,
    InputId<BurritoCount>,
    InputId<SalsaPerBurrito>,
) {
    let price = assert_new_input(
        db,
        &8,
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
        &3,
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
        &40,
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
    db: &Db<DefaultStorage, perf::Handler>,
    price: InputId<BurritoPrice>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
) {
    assert_queries(
        db,
        AssertQueriesArgs {
            price,
            count,
            burrito_salsa,
            price_w_shipping: QueryResult {
                value: 10,
                counts: Counts {
                    query_count: 2,
                    eval_count: 1,
                    registered_query_ops: 1,
                    stored_values: 1,
                    memo_count: 1,
                },
            },
            total_price: QueryResult {
                value: 30,
                counts: Counts {
                    query_count: 3,
                    eval_count: 1,
                    registered_query_ops: 1,
                    stored_values: 2,
                    memo_count: 1,
                },
            },
            price_with_vat: QueryResult {
                value: 35,
                counts: Counts {
                    query_count: 2,
                    eval_count: 1,
                    registered_query_ops: 1,
                    stored_values: 1,
                    memo_count: 1,
                },
            },
            salsa_in_order: QueryResult {
                value: 120,
                counts: Counts {
                    query_count: 3,
                    eval_count: 1,
                    registered_query_ops: 1,
                    stored_values: 2,
                    memo_count: 1,
                },
            },
        },
    );
}

fn additional_input_queries(
    db: &mut Db<DefaultStorage, perf::Handler>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
) {
    let discount_price = assert_new_input::<BurritoPrice>(
        db,
        &4,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 0,
            stored_values: 2,
            memo_count: 1,
        },
    );
    assert_queries(
        db,
        AssertQueriesArgs {
            price: discount_price,
            count,
            burrito_salsa,
            price_w_shipping: QueryResult {
                value: 6,
                counts: Counts {
                    query_count: 2,
                    eval_count: 1,
                    registered_query_ops: 0,
                    stored_values: 1,
                    memo_count: 1,
                },
            },
            total_price: QueryResult {
                value: 18,
                counts: Counts {
                    query_count: 3,
                    eval_count: 1,
                    registered_query_ops: 0,
                    stored_values: 2,
                    memo_count: 1,
                },
            },
            price_with_vat: QueryResult {
                value: 23,
                counts: Counts {
                    query_count: 2,
                    eval_count: 1,
                    registered_query_ops: 0,
                    stored_values: 1,
                    memo_count: 1,
                },
            },
            salsa_in_order: QueryResult {
                value: 120,
                counts: Counts {
                    query_count: 1,
                    eval_count: 0,
                    registered_query_ops: 0,
                    stored_values: 0,
                    memo_count: 0,
                },
            },
        },
    );
}

fn assert_new_input<I>(
    db: &mut Db<DefaultStorage, perf::Handler>,
    value: &I::Value,
    counts: Counts,
) -> InputId<I>
where
    I: Input,
{
    let before = db.event_handler().stats();
    let input_id = db.new_input::<I>(value);
    let after = db.event_handler().stats();
    assert_eq!(Counts::from(after.since(before)), counts);
    input_id
}

struct AssertQueriesArgs {
    price: InputId<BurritoPrice>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
    price_w_shipping: QueryResult,
    total_price: QueryResult,
    price_with_vat: QueryResult,
    salsa_in_order: QueryResult,
}

struct QueryResult {
    value: usize,
    counts: Counts,
}

fn assert_queries(db: &Db<DefaultStorage, perf::Handler>, args: AssertQueriesArgs) {
    let AssertQueriesArgs {
        price,
        count,
        burrito_salsa,
        price_w_shipping,
        total_price,
        price_with_vat,
        salsa_in_order,
    } = args;
    assert_query_delta::<BurritoPriceWithShipping>(
        db,
        &price,
        &price_w_shipping.value,
        price_w_shipping.counts,
    );
    assert_query_delta::<TotalPrice>(db, &(price, count), &total_price.value, total_price.counts);
    assert_query_delta::<PriceWithVat>(
        db,
        &(price, count),
        &(price_with_vat.value),
        price_with_vat.counts,
    );
    assert_query_delta::<SalsaInOrder>(
        db,
        &(burrito_salsa, count),
        &(salsa_in_order.value),
        salsa_in_order.counts,
    );
}

fn assert_query_delta<Q>(
    db: &Db<DefaultStorage, perf::Handler>,
    args: &Q::Args,
    exp_out: &Q::Out,
    d_counts: Counts,
) where
    Q: Query,
    Q::Out: Eq + Debug,
{
    let snapshot = db.snapshot();
    let before = snapshot.event_handler().stats();
    let out = snapshot.query::<Q>(args);
    assert_eq!(out.as_ref(), exp_out);
    let after = snapshot.event_handler().stats();
    assert_eq!(Counts::from(after.since(before)), d_counts);
}

impl From<perf::Stats> for Counts {
    fn from(stats: perf::Stats) -> Self {
        Self {
            query_count: stats.queries,
            eval_count: stats.evals,
            registered_query_ops: stats.query_ops.net(),
            stored_values: stats.values.net(),
            memo_count: stats.memos.net(),
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
        H: Handler<S>,
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
        H: Handler<S>,
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
        H: Handler<S>,
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
        H: Handler<S>,
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
        H: Handler<S>,
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
        H: Handler<S>,
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
        H: Handler<S>,
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
        H: Handler<S>,
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
        H: Handler<S>,
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
        H: Handler<S>,
    {
        *snapshot.query::<FlakyTotalPrice>(args) + 5
    }
}

const NAP_DURATION: Duration = Duration::from_millis(10);

struct NestedNap;
impl Query for NestedNap {
    type Args = u64;
    type Out = u64;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: Handler<S>,
    {
        *snapshot.query::<Nap>(args)
    }
}

struct Nap;
impl Query for Nap {
    type Args = u64;
    type Out = u64;

    fn eval<S, H>(_: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: Handler<S>,
    {
        thread::sleep(NAP_DURATION);
        *args
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
