#![doc = include_str!("../README.md")]
#![expect(
    clippy::print_stderr,
    reason = "`eprintln` is needed to communication with users"
)]

use std::path::Path;
use xshell::Shell;

mod bench;
mod test;

mod flags {
    xflags::xflags! {
        cmd xtask {
            cmd test {}
            cmd bench {}
        }
    }
}

fn main() -> anyhow::Result<()> {
    let flags = flags::Xtask::from_env()?;
    let sh = Shell::new()?;
    sh.change_dir(workspace_root());
    flags.subcommand.run(&sh)
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("bug: xtask is expected to live at `crates/xtask`")
}

impl flags::XtaskCmd {
    fn run(self, sh: &Shell) -> anyhow::Result<()> {
        match self {
            Self::Test(test) => test.run(sh),
            Self::Bench(bench) => bench.run(sh),
        }
    }
}
