#![expect(
    clippy::too_many_lines,
    reason = "Code with long types and many arguments formats poorly."
)]

use crate::macros::Add3;
use crate::macros::Add6;
use crate::macros::Add9;
use crate::macros::Add10;
use crate::macros::Add27;
use crate::macros::Add30;
use crate::macros::Add100;
use crate::macros::Dep1;
use crate::macros::Dep3;
use crate::macros::Dep6;
use crate::macros::Dep9;
use crate::macros::Dep10;
use crate::macros::Dep27;
use crate::macros::Dep30;
use crate::macros::Dep100;
use crate::macros::Inc1;
use crate::macros::Inc1V2;
use crate::macros::Inc1V3;
use crate::macros::Inc1V4;
use crate::macros::Inc1V5;
use crate::macros::Inc1V6;
use crate::macros::Inc1V7;
use crate::macros::Inc1V8;
use crate::macros::Inc1V9;
use crate::macros::Inc1V10;
use crate::macros::Inc1V11;
use crate::macros::Inc1V12;
use crate::macros::Inc1V13;
use crate::macros::Inc1V14;
use crate::macros::Inc1V15;
use crate::macros::Inc1V16;
use crate::macros::Inc1V17;
use crate::macros::Inc1V18;
use crate::macros::Inc1V19;
use crate::macros::Inc1V20;
use crate::macros::Inc1V21;
use crate::macros::Inc1V22;
use crate::macros::Inc1V23;
use crate::macros::Inc1V24;
use crate::macros::Inc1V25;
use crate::macros::Inc1V26;
use crate::macros::Inc1V27;
use crate::macros::Inc1V28;
use crate::macros::Inc1V29;
use crate::macros::Inc1V30;
use crate::macros::Inc1V31;
use crate::macros::Inc1V32;
use crate::macros::Inc1V33;
use crate::macros::Inc1V34;
use crate::macros::Inc1V35;
use crate::macros::Inc1V36;
use crate::macros::Inc1V37;
use crate::macros::Inc1V38;
use crate::macros::Inc1V39;
use crate::macros::Inc1V40;
use crate::macros::Inc1V41;
use crate::macros::Inc1V42;
use crate::macros::Inc1V43;
use crate::macros::Inc1V44;
use crate::macros::Inc1V45;
use crate::macros::Inc1V46;
use crate::macros::Inc1V47;
use crate::macros::Inc1V48;
use crate::macros::Inc1V49;
use crate::macros::Inc1V50;
use crate::macros::Inc1V51;
use crate::macros::Inc1V52;
use crate::macros::Inc1V53;
use crate::macros::Inc1V54;
use crate::macros::Inc1V55;
use crate::macros::Inc1V56;
use crate::macros::Inc1V57;
use crate::macros::Inc1V58;
use crate::macros::Inc1V59;
use crate::macros::Inc1V60;
use crate::macros::Inc1V61;
use crate::macros::Inc1V62;
use crate::macros::Inc1V63;
use crate::macros::Inc1V64;
use crate::macros::Inc1V65;
use crate::macros::Inc1V66;
use crate::macros::Inc1V67;
use crate::macros::Inc1V68;
use crate::macros::Inc1V69;
use crate::macros::Inc1V70;
use crate::macros::Inc1V71;
use crate::macros::Inc1V72;
use crate::macros::Inc1V73;
use crate::macros::Inc1V74;
use crate::macros::Inc1V75;
use crate::macros::Inc1V76;
use crate::macros::Inc1V77;
use crate::macros::Inc1V78;
use crate::macros::Inc1V79;
use crate::macros::Inc1V80;
use crate::macros::Inc1V81;
use crate::macros::Inc1V82;
use crate::macros::Inc1V83;
use crate::macros::Inc1V84;
use crate::macros::Inc1V85;
use crate::macros::Inc1V86;
use crate::macros::Inc1V87;
use crate::macros::Inc1V88;
use crate::macros::Inc1V89;
use crate::macros::Inc1V90;
use crate::macros::Inc1V91;
use crate::macros::Inc1V92;
use crate::macros::Inc1V93;
use crate::macros::Inc1V94;
use crate::macros::Inc1V95;
use crate::macros::Inc1V96;
use crate::macros::Inc1V97;
use crate::macros::Inc1V98;
use crate::macros::Inc1V99;
use crate::macros::Inc1V100;
use crate::macros::Inp;
use crate::macros::Inp2;
use crate::macros::Inp3;
use crate::macros::Inp4;
use crate::macros::Inp5;
use crate::macros::Inp6;
use crate::macros::Inp7;
use crate::macros::Inp8;
use crate::macros::Inp9;
use crate::macros::Tuple3;
use crate::macros::Tuple6;
use crate::macros::Tuple9;
use crate::macros::Tuple10;
use crate::macros::Tuple27;
use crate::macros::Tuple30;
use crate::macros::Tuple100;
use core::fmt::Debug;
use core::iter;
use salsita::Db;
use salsita::event::PerfHandler;
use salsita::query::Query;
use salsita::storage::DefaultStorage;
use std::thread;

#[derive(serde::Serialize)]
pub(crate) struct Benches(Vec<Bench>);

#[derive(serde::Serialize)]
pub(crate) struct Bench {
    name: String,
    scenarios: Vec<Scenario>,
}

#[derive(serde::Serialize)]
struct Scenario {
    name: String,
    counts: Counts,
    timings: Vec<Timings>,
}

/// Query/evaluation counts for a single run.
///
/// All counts are recorded by [`PerfHandler`] instrumentation.
#[derive(serde::Serialize)]
struct Counts {
    /// *Every* `Db::query::<Q>` invocation that occurred during the run,
    /// including nested calls made from within `Query::eval`.
    query: usize,
    /// The number of times `Query::eval` executed (i.e., cache misses /
    /// re-evaluations). Does *not* include memo hits where `eval` did not run.
    eval: usize,
}

/// Timing measurements for a single run.
///
/// All times are in **nanoseconds**, recorded by [`PerfHandler`] instrumentation.
#[derive(serde::Serialize)]
struct Timings {
    /// Wall-clock time measured for the top-level `Db::query` call that the
    /// scenario invoked. Includes memo lookups, bookkeeping, invalidation
    /// handling triggered by each query, and the `eval` work described below.
    query: u64,
    /// Sum of time spent executing `Query::eval` across the whole call tree.
    /// Because `Query::eval` implementations may recursively call
    /// `snapshot.query::<...>` (and those queries may themselves call `eval`),
    /// `eval_time_ns` includes the nested `eval` time in the entire evaluation
    /// tree. In other words, `eval_time_ns` is the total CPU time spent
    /// *inside* `eval` implementations.
    eval: u64,
}

type Sink3<D1, D2, D3> = Dep3<Add3, D1, D2, D3>;

type Sink6<D1, D2, D3, D4, D5, D6> = Dep6<Add6, D1, D2, D3, D4, D5, D6>;

type Sink9<D1, D2, D3, D4, D5, D6, D7, D8, D9> = Dep9<Add9, D1, D2, D3, D4, D5, D6, D7, D8, D9>;

type Sink10<D1, D2, D3, D4, D5, D6, D7, D8, D9, D10> =
    Dep10<Add10, D1, D2, D3, D4, D5, D6, D7, D8, D9, D10>;

type Sink27<
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
> = Dep27<
    Add27,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
>;

type Sink30<
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
    D28,
    D29,
    D30,
> = Dep30<
    Add30,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
    D28,
    D29,
    D30,
>;

type Sink100<
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
    D28,
    D29,
    D30,
    D31,
    D32,
    D33,
    D34,
    D35,
    D36,
    D37,
    D38,
    D39,
    D40,
    D41,
    D42,
    D43,
    D44,
    D45,
    D46,
    D47,
    D48,
    D49,
    D50,
    D51,
    D52,
    D53,
    D54,
    D55,
    D56,
    D57,
    D58,
    D59,
    D60,
    D61,
    D62,
    D63,
    D64,
    D65,
    D66,
    D67,
    D68,
    D69,
    D70,
    D71,
    D72,
    D73,
    D74,
    D75,
    D76,
    D77,
    D78,
    D79,
    D80,
    D81,
    D82,
    D83,
    D84,
    D85,
    D86,
    D87,
    D88,
    D89,
    D90,
    D91,
    D92,
    D93,
    D94,
    D95,
    D96,
    D97,
    D98,
    D99,
    D100,
> = Dep100<
    Add100,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
    D28,
    D29,
    D30,
    D31,
    D32,
    D33,
    D34,
    D35,
    D36,
    D37,
    D38,
    D39,
    D40,
    D41,
    D42,
    D43,
    D44,
    D45,
    D46,
    D47,
    D48,
    D49,
    D50,
    D51,
    D52,
    D53,
    D54,
    D55,
    D56,
    D57,
    D58,
    D59,
    D60,
    D61,
    D62,
    D63,
    D64,
    D65,
    D66,
    D67,
    D68,
    D69,
    D70,
    D71,
    D72,
    D73,
    D74,
    D75,
    D76,
    D77,
    D78,
    D79,
    D80,
    D81,
    D82,
    D83,
    D84,
    D85,
    D86,
    D87,
    D88,
    D89,
    D90,
    D91,
    D92,
    D93,
    D94,
    D95,
    D96,
    D97,
    D98,
    D99,
    D100,
>;
type Branch<O> = Dep1<O, Inp>;

/// Star-shaped graph variants (single input -> many dependents). `starN`
/// means one input node feeds N dependent nodes (fanout = N).
///
/// This is useful to stress fanout and reveal invalidation cost when a
/// single input change causes re-evaluation of many dependents.
type Star10 = Sink10<
    Branch<Inc1>,
    Branch<Inc1V2>,
    Branch<Inc1V3>,
    Branch<Inc1V4>,
    Branch<Inc1V5>,
    Branch<Inc1V6>,
    Branch<Inc1V7>,
    Branch<Inc1V8>,
    Branch<Inc1V9>,
    Branch<Inc1V10>,
>;

type Star30 = Sink30<
    Branch<Inc1>,
    Branch<Inc1V2>,
    Branch<Inc1V3>,
    Branch<Inc1V4>,
    Branch<Inc1V5>,
    Branch<Inc1V6>,
    Branch<Inc1V7>,
    Branch<Inc1V8>,
    Branch<Inc1V9>,
    Branch<Inc1V10>,
    Branch<Inc1V11>,
    Branch<Inc1V12>,
    Branch<Inc1V13>,
    Branch<Inc1V14>,
    Branch<Inc1V15>,
    Branch<Inc1V16>,
    Branch<Inc1V17>,
    Branch<Inc1V18>,
    Branch<Inc1V19>,
    Branch<Inc1V20>,
    Branch<Inc1V21>,
    Branch<Inc1V22>,
    Branch<Inc1V23>,
    Branch<Inc1V24>,
    Branch<Inc1V25>,
    Branch<Inc1V26>,
    Branch<Inc1V27>,
    Branch<Inc1V28>,
    Branch<Inc1V29>,
    Branch<Inc1V30>,
>;

type Star100 = Sink100<
    Branch<Inc1>,
    Branch<Inc1V2>,
    Branch<Inc1V3>,
    Branch<Inc1V4>,
    Branch<Inc1V5>,
    Branch<Inc1V6>,
    Branch<Inc1V7>,
    Branch<Inc1V8>,
    Branch<Inc1V9>,
    Branch<Inc1V10>,
    Branch<Inc1V11>,
    Branch<Inc1V12>,
    Branch<Inc1V13>,
    Branch<Inc1V14>,
    Branch<Inc1V15>,
    Branch<Inc1V16>,
    Branch<Inc1V17>,
    Branch<Inc1V18>,
    Branch<Inc1V19>,
    Branch<Inc1V20>,
    Branch<Inc1V21>,
    Branch<Inc1V22>,
    Branch<Inc1V23>,
    Branch<Inc1V24>,
    Branch<Inc1V25>,
    Branch<Inc1V26>,
    Branch<Inc1V27>,
    Branch<Inc1V28>,
    Branch<Inc1V29>,
    Branch<Inc1V30>,
    Branch<Inc1V31>,
    Branch<Inc1V32>,
    Branch<Inc1V33>,
    Branch<Inc1V34>,
    Branch<Inc1V35>,
    Branch<Inc1V36>,
    Branch<Inc1V37>,
    Branch<Inc1V38>,
    Branch<Inc1V39>,
    Branch<Inc1V40>,
    Branch<Inc1V41>,
    Branch<Inc1V42>,
    Branch<Inc1V43>,
    Branch<Inc1V44>,
    Branch<Inc1V45>,
    Branch<Inc1V46>,
    Branch<Inc1V47>,
    Branch<Inc1V48>,
    Branch<Inc1V49>,
    Branch<Inc1V50>,
    Branch<Inc1V51>,
    Branch<Inc1V52>,
    Branch<Inc1V53>,
    Branch<Inc1V54>,
    Branch<Inc1V55>,
    Branch<Inc1V56>,
    Branch<Inc1V57>,
    Branch<Inc1V58>,
    Branch<Inc1V59>,
    Branch<Inc1V60>,
    Branch<Inc1V61>,
    Branch<Inc1V62>,
    Branch<Inc1V63>,
    Branch<Inc1V64>,
    Branch<Inc1V65>,
    Branch<Inc1V66>,
    Branch<Inc1V67>,
    Branch<Inc1V68>,
    Branch<Inc1V69>,
    Branch<Inc1V70>,
    Branch<Inc1V71>,
    Branch<Inc1V72>,
    Branch<Inc1V73>,
    Branch<Inc1V74>,
    Branch<Inc1V75>,
    Branch<Inc1V76>,
    Branch<Inc1V77>,
    Branch<Inc1V78>,
    Branch<Inc1V79>,
    Branch<Inc1V80>,
    Branch<Inc1V81>,
    Branch<Inc1V82>,
    Branch<Inc1V83>,
    Branch<Inc1V84>,
    Branch<Inc1V85>,
    Branch<Inc1V86>,
    Branch<Inc1V87>,
    Branch<Inc1V88>,
    Branch<Inc1V89>,
    Branch<Inc1V90>,
    Branch<Inc1V91>,
    Branch<Inc1V92>,
    Branch<Inc1V93>,
    Branch<Inc1V94>,
    Branch<Inc1V95>,
    Branch<Inc1V96>,
    Branch<Inc1V97>,
    Branch<Inc1V98>,
    Branch<Inc1V99>,
    Branch<Inc1V100>,
>;

type Link<D> = Dep1<Inc1, D>;

type Link5<D> = Link<Link<Link<Link<Link<D>>>>>;

type Link25<D> = Link5<Link5<Link5<Link5<Link5<D>>>>>;

/// Chain-shaped graph variants `(A -> B -> C -> ...)`. `chainN` denotes a
/// linear chain of N distinct queries where each node depends on the
/// previous node.
///
/// This exercises propagation through long narrow dependency paths and
/// exposes per-level recursion/overhead.
type Chain5 = Link5<Inp>;

type Chain25 = Link25<Inp>;

type Chain100 = Link25<Link25<Link25<Link25<Inp>>>>;

type Root = Inp;

type Tree<O, P> = Dep1<O, P>;

type TreeD1I1 = Tree<Inc1, Root>;

type TreeD1I2 = Tree<Inc1V2, Root>;

type TreeD1I3 = Tree<Inc1V3, Root>;

type TreeD2I1 = Tree<Inc1V4, TreeD1I1>;

type TreeD2I2 = Tree<Inc1V5, TreeD1I1>;

type TreeD2I3 = Tree<Inc1V6, TreeD1I1>;

type TreeD2I4 = Tree<Inc1V7, TreeD1I2>;

type TreeD2I5 = Tree<Inc1V8, TreeD1I2>;

type TreeD2I6 = Tree<Inc1V9, TreeD1I2>;

type TreeD2I7 = Tree<Inc1V10, TreeD1I3>;

type TreeD2I8 = Tree<Inc1V11, TreeD1I3>;

type TreeD2I9 = Tree<Inc1V12, TreeD1I3>;

type TreeD3I1 = Tree<Inc1V13, TreeD2I1>;

type TreeD3I2 = Tree<Inc1V14, TreeD2I1>;

type TreeD3I3 = Tree<Inc1V15, TreeD2I1>;

type TreeD3I4 = Tree<Inc1V16, TreeD2I2>;

type TreeD3I5 = Tree<Inc1V17, TreeD2I2>;

type TreeD3I6 = Tree<Inc1V18, TreeD2I2>;

type TreeD3I7 = Tree<Inc1V19, TreeD2I3>;

type TreeD3I8 = Tree<Inc1V20, TreeD2I3>;

type TreeD3I9 = Tree<Inc1V21, TreeD2I3>;

type TreeD3I10 = Tree<Inc1V22, TreeD2I4>;

type TreeD3I11 = Tree<Inc1V23, TreeD2I4>;

type TreeD3I12 = Tree<Inc1V24, TreeD2I4>;

type TreeD3I13 = Tree<Inc1V25, TreeD2I5>;

type TreeD3I14 = Tree<Inc1V26, TreeD2I5>;

type TreeD3I15 = Tree<Inc1V27, TreeD2I5>;

type TreeD3I16 = Tree<Inc1V28, TreeD2I6>;

type TreeD3I17 = Tree<Inc1V29, TreeD2I6>;

type TreeD3I18 = Tree<Inc1V30, TreeD2I6>;

type TreeD3I19 = Tree<Inc1V31, TreeD2I7>;

type TreeD3I20 = Tree<Inc1V32, TreeD2I7>;

type TreeD3I21 = Tree<Inc1V33, TreeD2I7>;

type TreeD3I22 = Tree<Inc1V34, TreeD2I8>;

type TreeD3I23 = Tree<Inc1V35, TreeD2I8>;

type TreeD3I24 = Tree<Inc1V36, TreeD2I8>;

type TreeD3I25 = Tree<Inc1V37, TreeD2I9>;

type TreeD3I26 = Tree<Inc1V38, TreeD2I9>;

type TreeD3I27 = Tree<Inc1V39, TreeD2I9>;

/// Balanced k-ary trees of small depth. `tree_kKdD` is a K-ary tree of
/// depth D. These combine branching and depth in a controlled manner.
///
/// These represents hierarchical graphs (such as AST / module dependency
/// shapes). Useful to measure bulk recomputation costs when internal nodes
/// invalidate whole subtrees.
type TreeK3D2 = Sink3<TreeD1I1, TreeD1I2, TreeD1I3>;

type TreeK3D3 =
    Sink9<TreeD2I1, TreeD2I2, TreeD2I3, TreeD2I4, TreeD2I5, TreeD2I6, TreeD2I7, TreeD2I8, TreeD2I9>;

type TreeK3D4 = Sink27<
    TreeD3I1,
    TreeD3I2,
    TreeD3I3,
    TreeD3I4,
    TreeD3I5,
    TreeD3I6,
    TreeD3I7,
    TreeD3I8,
    TreeD3I9,
    TreeD3I10,
    TreeD3I11,
    TreeD3I12,
    TreeD3I13,
    TreeD3I14,
    TreeD3I15,
    TreeD3I16,
    TreeD3I17,
    TreeD3I18,
    TreeD3I19,
    TreeD3I20,
    TreeD3I21,
    TreeD3I22,
    TreeD3I23,
    TreeD3I24,
    TreeD3I25,
    TreeD3I26,
    TreeD3I27,
>;

type FanIn<I> = Dep1<Inc1, I>;

type FanOut<O, H> = Dep1<O, H>;

type Hub3 = Dep3<Add3, FanIn<Inp>, FanIn<Inp2>, FanIn<Inp3>>;

type Hub6 = Dep6<Add6, FanIn<Inp>, FanIn<Inp2>, FanIn<Inp3>, FanIn<Inp4>, FanIn<Inp5>, FanIn<Inp6>>;

type Hub9 = Dep9<
    Add9,
    FanIn<Inp>,
    FanIn<Inp2>,
    FanIn<Inp3>,
    FanIn<Inp4>,
    FanIn<Inp5>,
    FanIn<Inp6>,
    FanIn<Inp7>,
    FanIn<Inp8>,
    FanIn<Inp9>,
>;

/// Hourglass-shaped graph. `hourglassN` means N inputs converge into a
/// single intermediate region and then diverge again into N outputs.
///
/// This kind of graph stresses shared subexpressions and reuse. A correct
/// incremental engine should compute the shared region once and reuse it.
type Hourglass3 = Sink3<FanOut<Inc1, Hub3>, FanOut<Inc1V2, Hub3>, FanOut<Inc1V3, Hub3>>;

type Hourglass6 = Sink6<
    FanOut<Inc1, Hub6>,
    FanOut<Inc1V2, Hub6>,
    FanOut<Inc1V3, Hub6>,
    FanOut<Inc1V4, Hub6>,
    FanOut<Inc1V5, Hub6>,
    FanOut<Inc1V6, Hub6>,
>;

type Hourglass9 = Sink9<
    FanOut<Inc1, Hub9>,
    FanOut<Inc1V2, Hub9>,
    FanOut<Inc1V3, Hub9>,
    FanOut<Inc1V4, Hub9>,
    FanOut<Inc1V5, Hub9>,
    FanOut<Inc1V6, Hub9>,
    FanOut<Inc1V7, Hub9>,
    FanOut<Inc1V8, Hub9>,
    FanOut<Inc1V9, Hub9>,
>;

impl Benches {
    pub(crate) fn new<
        const WARMUP_COUNT: usize,
        const N: usize,
        const PARALLEL_GENERATION_COUNT: usize,
        const PARALLEL_SNAPSHOT_COUNT: usize,
    >() -> Self {
        Self(vec![
            star10::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            star30::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            star100::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            chain5::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            chain25::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            chain100::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            tree_k3d2::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            tree_k3d3::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            tree_k3d4::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            hourglass3::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            hourglass6::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
            hourglass9::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT>(),
        ])
    }
}

pub(crate) fn star10<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios =
        bench_graph::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT, Star10>(
            |db| {
                let inp = db.new_input::<Inp>(&Some(1));
                let inputs = Tuple10(inp, inp, inp, inp, inp, inp, inp, inp, inp, inp);
                (inputs, Some(20))
            },
            |db, Tuple10(inp, _, _, _, _, _, _, _, _, _), toggle| {
                let toggle = usize::from(toggle);
                db.set_input(*inp, &Some(1 + toggle));
                Some(20 + 10 * toggle)
            },
        );
    Bench::new("star10", scenarios)
}

pub(crate) fn star30<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios =
        bench_graph::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT, Star30>(
            |db| {
                let inp = db.new_input::<Inp>(&Some(1));
                let inputs = Tuple30(
                    inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                    inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                );
                (inputs, Some(60))
            },
            |db,
             Tuple30(
                inp,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
                _,
            ),
             toggle| {
                let toggle = usize::from(toggle);
                db.set_input(*inp, &Some(1 + toggle));
                Some(60 + 30 * toggle)
            },
        );
    Bench::new("star30", scenarios)
}

pub(crate) fn star100<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios = bench_graph::<
        WARMUP_COUNT,
        N,
        PARALLEL_GENERATION_COUNT,
        PARALLEL_SNAPSHOT_COUNT,
        Star100,
    >(
        |db| {
            let inp = db.new_input::<Inp>(&Some(1));
            let inputs = Tuple100(
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp,
            );
            (inputs, Some(200))
        },
        |db,
         Tuple100(
            inp,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
        ),
         toggle| {
            let toggle = usize::from(toggle);
            db.set_input(*inp, &Some(1 + toggle));
            Some(200 + toggle * 100)
        },
    );
    Bench::new("star100", scenarios)
}

pub(crate) fn chain5<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios =
        bench_graph::<WARMUP_COUNT, N, PARALLEL_GENERATION_COUNT, PARALLEL_SNAPSHOT_COUNT, Chain5>(
            |db| {
                let inputs = db.new_input::<Inp>(&Some(0));
                (inputs, Some(5))
            },
            |db, inp, toggle| {
                let toggle = usize::from(toggle);
                db.set_input(*inp, &Some(toggle));
                Some(5 + toggle)
            },
        );
    Bench::new("chain5", scenarios)
}

pub(crate) fn chain25<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios = bench_graph::<
        WARMUP_COUNT,
        N,
        PARALLEL_GENERATION_COUNT,
        PARALLEL_SNAPSHOT_COUNT,
        Chain25,
    >(
        |db| {
            let inputs = db.new_input::<Inp>(&Some(0));
            (inputs, Some(25))
        },
        |db, inp, toggle| {
            let toggle = usize::from(toggle);
            db.set_input(*inp, &Some(toggle));
            Some(25 + toggle)
        },
    );
    Bench::new("chain25", scenarios)
}

pub(crate) fn chain100<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios = bench_graph::<
        WARMUP_COUNT,
        N,
        PARALLEL_GENERATION_COUNT,
        PARALLEL_SNAPSHOT_COUNT,
        Chain100,
    >(
        |db| {
            let inputs = db.new_input::<Inp>(&Some(0));
            (inputs, Some(100))
        },
        |db, inp, toggle| {
            let toggle = usize::from(toggle);
            db.set_input(*inp, &Some(toggle));
            Some(100 + toggle)
        },
    );
    Bench::new("chain100", scenarios)
}

pub(crate) fn tree_k3d2<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios = bench_graph::<
        WARMUP_COUNT,
        N,
        PARALLEL_GENERATION_COUNT,
        PARALLEL_SNAPSHOT_COUNT,
        TreeK3D2,
    >(
        |db| {
            let inp = db.new_input::<Inp>(&Some(0));
            let inputs = Tuple3(inp, inp, inp);
            (inputs, Some(3))
        },
        |db, Tuple3(inp, _, _), toggle| {
            let toggle = usize::from(toggle);
            db.set_input(*inp, &Some(toggle));
            Some(3 + 3 * toggle)
        },
    );
    Bench::new("tree_k3d2", scenarios)
}

pub(crate) fn tree_k3d3<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios = bench_graph::<
        WARMUP_COUNT,
        N,
        PARALLEL_GENERATION_COUNT,
        PARALLEL_SNAPSHOT_COUNT,
        TreeK3D3,
    >(
        |db| {
            let inp = db.new_input::<Inp>(&Some(0));
            let inputs = Tuple9(inp, inp, inp, inp, inp, inp, inp, inp, inp);
            (inputs, Some(18))
        },
        |db, Tuple9(inp, _, _, _, _, _, _, _, _), toggle| {
            let toggle = usize::from(toggle);
            db.set_input(*inp, &Some(toggle));
            Some(18 + 9 * toggle)
        },
    );
    Bench::new("tree_k3d3", scenarios)
}

pub(crate) fn tree_k3d4<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios = bench_graph::<
        WARMUP_COUNT,
        N,
        PARALLEL_GENERATION_COUNT,
        PARALLEL_SNAPSHOT_COUNT,
        TreeK3D4,
    >(
        |db| {
            let inp = db.new_input::<Inp>(&Some(0));
            let inputs = Tuple27(
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
            );
            (inputs, Some(81))
        },
        |db,
         Tuple27(
            inp,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
        ),
         toggle| {
            let toggle = usize::from(toggle);
            db.set_input(*inp, &Some(toggle));
            Some(81 + 27 * toggle)
        },
    );
    Bench::new("tree_k3d4", scenarios)
}

pub(crate) fn hourglass3<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios = bench_graph::<
        WARMUP_COUNT,
        N,
        PARALLEL_GENERATION_COUNT,
        PARALLEL_SNAPSHOT_COUNT,
        Hourglass3,
    >(
        |db| {
            let inp = Tuple3(
                db.new_input::<Inp>(&Some(2)),
                db.new_input::<Inp2>(&Some(1)),
                db.new_input::<Inp3>(&Some(1)),
            );
            let inputs = Tuple3(inp, inp, inp);
            (inputs, Some(24))
        },
        |db, Tuple3(Tuple3(inp1, inp2, _), _, _), toggle| {
            db.set_input(*inp1, &Some(1 + usize::from(!toggle)));
            db.set_input(*inp2, &Some(1 + usize::from(toggle)));
            Some(24)
        },
    );
    Bench::new("hourglass3", scenarios)
}

pub(crate) fn hourglass6<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios = bench_graph::<
        WARMUP_COUNT,
        N,
        PARALLEL_GENERATION_COUNT,
        PARALLEL_SNAPSHOT_COUNT,
        Hourglass6,
    >(
        |db| {
            let inp = Tuple6(
                db.new_input::<Inp>(&Some(2)),
                db.new_input::<Inp2>(&Some(1)),
                db.new_input::<Inp3>(&Some(1)),
                db.new_input::<Inp4>(&Some(1)),
                db.new_input::<Inp5>(&Some(1)),
                db.new_input::<Inp6>(&Some(1)),
            );
            let inputs = Tuple6(inp, inp, inp, inp, inp, inp);
            (inputs, Some(84))
        },
        |db, Tuple6(Tuple6(inp1, inp2, _, _, _, _), _, _, _, _, _), toggle| {
            db.set_input(*inp1, &Some(1 + usize::from(!toggle)));
            db.set_input(*inp2, &Some(1 + usize::from(toggle)));
            Some(84)
        },
    );
    Bench::new("hourglass6", scenarios)
}

pub(crate) fn hourglass9<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
>() -> Bench {
    let scenarios = bench_graph::<
        WARMUP_COUNT,
        N,
        PARALLEL_GENERATION_COUNT,
        PARALLEL_SNAPSHOT_COUNT,
        Hourglass9,
    >(
        |db| {
            let inp = Tuple9(
                db.new_input::<Inp>(&Some(2)),
                db.new_input::<Inp2>(&Some(1)),
                db.new_input::<Inp3>(&Some(1)),
                db.new_input::<Inp4>(&Some(1)),
                db.new_input::<Inp5>(&Some(1)),
                db.new_input::<Inp6>(&Some(1)),
                db.new_input::<Inp7>(&Some(1)),
                db.new_input::<Inp8>(&Some(1)),
                db.new_input::<Inp9>(&Some(1)),
            );
            let inputs = Tuple9(inp, inp, inp, inp, inp, inp, inp, inp, inp);
            (inputs, Some(180))
        },
        |db, Tuple9(Tuple9(inp1, inp2, _, _, _, _, _, _, _), _, _, _, _, _, _, _, _), toggle| {
            db.set_input(*inp1, &Some(1 + usize::from(!toggle)));
            db.set_input(*inp2, &Some(1 + usize::from(toggle)));
            Some(180)
        },
    );
    Bench::new("hourglass9", scenarios)
}

impl Bench {
    fn new(name: &'static str, scenarios: Vec<Scenario>) -> Self {
        Self {
            name: name.to_owned(),
            scenarios,
        }
    }
}

fn bench_graph<
    const WARMUP_COUNT: usize,
    const N: usize,
    const PARALLEL_GENERATION_COUNT: usize,
    const PARALLEL_SNAPSHOT_COUNT: usize,
    Sink,
>(
    alloc_inputs: impl Fn(&mut Db<DefaultStorage, PerfHandler>) -> (Sink::Args, Sink::Out),
    update_inputs: impl Fn(&mut Db<DefaultStorage, PerfHandler>, &Sink::Args, bool) -> Sink::Out,
) -> Vec<Scenario>
where
    Sink: Query,
    Sink::Out: Copy + Eq + Debug,
{
    // Measurements for a full build scenario.
    //
    // Starting with an empty `Db`, call the sink query once, causing every
    // reachable query to be evaluated and inserted into the memo table. This
    // measures the cost of an initial full build (time to register memos &
    // evaluate nodes).
    let cold = bench_scenario::<WARMUP_COUNT, N, _>(
        "cold",
        |_| {},
        |db, ()| {
            let (inputs, exp_out) = alloc_inputs(db);
            let out = db.snapshot().query::<Sink>(&inputs);
            assert_eq!(*out, exp_out);
        },
    );

    // Measurements for a no-op scenario.
    //
    // Starting with a fresh `Db` that is already populated/memoized for the
    // graph, call the sink query. No inputs are changed as part of this
    // scenario. This measures steady-state read/query cost when the memo table
    // is warm and every `Db::query` call is a memo hit.
    let memo = bench_scenario::<WARMUP_COUNT, N, _>(
        "memo",
        |db| {
            let (inputs, exp_out) = alloc_inputs(db);
            db.snapshot().query::<Sink>(&inputs);
            (inputs, exp_out)
        },
        |db, (inputs, exp_out)| {
            let out = db.snapshot().query::<Sink>(&inputs);
            assert_eq!(*out, exp_out);
        },
    );

    // Measurements for an incremental scenario.
    //
    // Starting with a fresh `Db` populated with memos for the whole graph,
    // change exactly one input, then call the sink query. This measures
    // incremental recomputation cost for the changed inputs and its
    // dependents. Everything else is memoized.
    let update = bench_scenario::<WARMUP_COUNT, N, _>(
        "update",
        |db| {
            let (inputs, exp_out) = alloc_inputs(db);
            let out = db.snapshot().query::<Sink>(&inputs);
            assert_eq!(*out, exp_out);
            inputs
        },
        |db, inputs| {
            let exp_out = update_inputs(db, &inputs, true);
            let out = db.snapshot().query::<Sink>(&inputs);
            assert_eq!(*out, exp_out);
        },
    );

    // Measurements for a parallel scenario.
    //
    // Starting with a fresh `Db`, this function calls the sink query multiple
    // times from parallel snapshots while inputs are modified from the main
    // thread. Measures ability of the `Db` to parallelize query invocations.
    let parallel = bench_scenario::<WARMUP_COUNT, N, _>(
        "parallel",
        |db| {
            let (inputs, exp_out) = alloc_inputs(db);
            let out = db.snapshot().query::<Sink>(&inputs);
            assert_eq!(*out, exp_out);
            inputs
        },
        |db, inputs| {
            let mut toggle = false;
            let mut handles = Vec::with_capacity(PARALLEL_SNAPSHOT_COUNT);
            for generation in 0..PARALLEL_GENERATION_COUNT {
                toggle = !toggle;
                let exp_out = update_inputs(db, &inputs, toggle);
                for thread in 0..PARALLEL_SNAPSHOT_COUNT {
                    handles.push({
                        thread::Builder::new()
                            .name(format!("eval_{generation}_{thread}"))
                            .spawn({
                                let inputs = inputs.clone();
                                let snapshot = db.snapshot();
                                move || {
                                    let out = snapshot.query::<Sink>(&inputs);
                                    assert_eq!(*out, exp_out);
                                }
                            })
                            .expect("failed to spawn thread")
                    });
                }
                for handle in handles.drain(..) {
                    handle.join().expect("parallel evaluation failed");
                }
            }
        },
    );

    vec![cold, memo, update, parallel]
}

fn bench_scenario<const WARMUP_COUNT: usize, const N: usize, Inp>(
    name: &'static str,
    init: impl Copy + Fn(&mut Db<DefaultStorage, PerfHandler>) -> Inp,
    bench: impl Copy + Fn(&mut Db<DefaultStorage, PerfHandler>, Inp),
) -> Scenario {
    let (_, counts) = bench_iter(init, bench);
    for _ in 0..WARMUP_COUNT {
        let (_, _) = bench_iter(init, bench);
    }
    let timings = iter::repeat_with(|| {
        let (timings, _) = bench_iter(init, bench);
        timings
    })
    .take(N)
    .collect();
    return Scenario {
        name: name.to_owned(),
        counts,
        timings,
    };

    fn bench_iter<T>(
        init: impl Fn(&mut Db<DefaultStorage, PerfHandler>) -> T,
        bench: impl Fn(&mut Db<DefaultStorage, PerfHandler>, T),
    ) -> (Timings, Counts) {
        let mut db = Db::default();
        let v = init(&mut db);
        db.event_handler().reset();
        bench(&mut db, v);
        let m = db.event_handler();
        (Timings::new(m), Counts::new(m))
    }
}

impl Counts {
    fn new(m: &PerfHandler) -> Self {
        Self {
            query: m.query_count(),
            eval: m.eval_count(),
        }
    }
}

impl Timings {
    fn new(m: &PerfHandler) -> Self {
        Self {
            query: m.query_time().as_nanos().try_into().unwrap_or(u64::MAX),
            eval: m.eval_time().as_nanos().try_into().unwrap_or(u64::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::bench;
    use core::hint::black_box;

    #[test]
    fn star10() {
        black_box(bench::star10::<0, 1, 2, 1>());
    }

    #[test]
    fn star30() {
        black_box(bench::star30::<0, 1, 2, 1>());
    }

    #[test]
    fn star100() {
        black_box(bench::star100::<0, 1, 2, 1>());
    }

    #[test]
    fn chain5() {
        black_box(bench::chain5::<0, 1, 2, 1>());
    }

    #[test]
    fn chain25() {
        black_box(bench::chain25::<0, 1, 2, 1>());
    }

    #[test]
    fn chain100() {
        black_box(bench::chain100::<0, 1, 2, 1>());
    }

    #[test]
    fn tree_k3d2() {
        black_box(bench::tree_k3d2::<0, 1, 2, 1>());
    }

    #[test]
    fn tree_k3d3() {
        black_box(bench::tree_k3d3::<0, 1, 2, 1>());
    }

    #[test]
    fn tree_k3d4() {
        black_box(bench::tree_k3d4::<0, 1, 2, 1>());
    }

    #[test]
    fn hourglass3() {
        black_box(bench::hourglass3::<0, 1, 2, 1>());
    }

    #[test]
    fn hourglass6() {
        black_box(bench::hourglass6::<0, 1, 2, 1>());
    }

    #[test]
    fn hourglass9() {
        // Increase parallel generation/snapshot count for a single test to
        // check for parallel snapshot errors.
        black_box(bench::hourglass9::<0, 1, 100, 10>());
    }
}
