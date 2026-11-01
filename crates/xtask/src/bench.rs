use crate::flags;
use xshell::Shell;
use xshell::cmd;

impl flags::Bench {
    pub(crate) fn run(self, sh: &Shell) -> anyhow::Result<()> {
        let Self {} = self;

        cmd!(sh, "cargo run --release --package salsita_bench").run()?;

        Ok(())
    }
}
