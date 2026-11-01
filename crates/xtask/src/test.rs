use crate::flags;
use xshell::Shell;
use xshell::cmd;

impl flags::Test {
    pub(crate) fn run(self, sh: &Shell) -> anyhow::Result<()> {
        let Self {} = self;

        cmd!(sh, "cargo test --workspace").run()?;
        cmd!(sh, "cargo clippy --workspace -- -D warnings").run()?;
        cmd!(sh, "cargo fmt --all --check").run()?;
        cmd!(sh, "taplo fmt --check").run()?;

        Ok(())
    }
}
