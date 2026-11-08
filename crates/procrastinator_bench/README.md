# Procrastinator benchmarks

This is the benchmark runner for Procrastinator.
It builds a set of synthetic query graphs,
runs each of them through a series of scenarios,
and prints the measurements as JSON to standard output.

This crate is internal to the Procrastinator workspace and is not meant to be used as a dependency.

## Running

From the workspace root, run:

```sh
cargo xtask bench
```

This is shorthand for `cargo run --release --package procrastinator_bench`.
The release profile uses fat LTO and a single codegen unit,
so the build will be slow.

## Graphs

Every benchmark is a large graph of small queries, each of which performs trivial amounts of work,
such as adding or incrementing small integers.
Each graph ends in a single _sink_ query which the benchmark evaluates.

- **`star10`, `star30`, `star100`**:
  one input fans out to N dependent queries, which a sink sums up.
  A change to the input invalidates every dependent,
  which stress-tests wide fan-out.
- **`chain5`, `chain25`, `chain100`**:
  a linear chain of N queries,
  each depending on the previous one.
  This stress-tests propagation through long, narrow dependency paths
  and the per-level overhead.
- **`tree_k3d2`, `tree_k3d3`, `tree_k3d4`**:
  balanced ternary trees of depth 2, 3 and 4.
  These resemble hierarchical graphs such as syntax trees or module dependencies,
  where a change to an inner node invalidates a whole subtree.
- **`hourglass3`, `hourglass6`, `hourglass9`**:
  N inputs converge into a shared hub,
  then diverge again into N outputs.
  The updates change two inputs in a way that keeps the hub's output the same,
  so this measures how well unchanged intermediate results cut off re-evaluation.

## Scenarios

Each graph runs through four scenarios.
Every iteration starts from a fresh `Db`.

- **`cold`**:
  create the inputs and call the sink on an empty database,
  so every query is evaluated.
  This is the cost of an initial full build.
- **`memo`**:
  call the sink on a database where every query is already memoized.
  Every query call is a memo hit.
- **`update`**:
  starting from a fully memoized database,
  change the inputs once and call the sink.
  This is the cost of an incremental update.
- **`parallel`**:
  starting from a fully memoized database,
  run multiple generations of input changes.
  In each generation,
  multiple threads call the sink from their own snapshot.
  This measures how well query calls parallelize.

Each scenario warms up before measuring a fixed numer of iterations.
The query results are checked against the expected outputs in every iteration,
so a benchmark run doubles as a correctness test.

## Output

The report is a single JSON array with one object per graph:

```json
[
  {
    "name": "star10",
    "scenarios": [
      {
        "name": "cold",
        "counts": { "query": 21, "eval": 11 },
        "timings": [{ "query": 21314, "eval": 12459 }]
      }
    ]
  }
]
```

All measurements are recorded with Procrastinator's `perf::Handler`:

- `counts` holds the counts of a single iteration.
  `query` is the number of query calls, including nested calls from within other queries.
  `eval` is the number of times a query was actually evaluated, which excludes memo hits.
- `timings` holds one entry for each measured iteration, in nanoseconds.
  `query` is the time spent in query calls,
  including memo lookups and bookkeeping.
  `eval` is the time spent inside `Query::eval` implementations.

## Testing

`cargo test` runs every benchmark once with a minimal number of iterations to ensure benchmarks produce correct outputs.
The `hourglass9` test uses 100 generations of 10 parallel snapshots to shake out concurrency bugs.