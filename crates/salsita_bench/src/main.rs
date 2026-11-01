#![expect(
    clippy::arithmetic_side_effects,
    reason = "Release profile (ie. wrapping) arithmetic is preferred so benchmarks represent Salsita performance instead of arithmetic performance."
)]
mod bench;
mod macros;

fn main() {
    let report = bench::Benches::new::<3, 50, 6, 6>();
    match serde_json::to_value(report) {
        #[expect(clippy::print_stdout, reason = "Report should be printed to stdout.")]
        Ok(json) => println!("{json}"),
        Err(err) => panic!("serialization error: {err}"),
    }
}
