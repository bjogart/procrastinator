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
    flags::Xtask::from_env()?.subcommand.run(&Shell::new()?)
}

impl flags::XtaskCmd {
    fn run(self, sh: &Shell) -> anyhow::Result<()> {
        match self {
            Self::Test(test) => test.run(sh),
            Self::Bench(bench) => bench.run(sh),
        }
    }
}
