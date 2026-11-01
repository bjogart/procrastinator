use crate::flags;
use anyhow::anyhow;
use xshell::Shell;
use xshell::cmd;

impl flags::Test {
    pub(crate) fn run(self, sh: &Shell) -> anyhow::Result<()> {
        let Self {} = self;

        cmd!(sh, "cargo test --workspace").run()?;
        cmd!(sh, "cargo clippy --workspace -- -D warnings").run()?;
        cmd!(sh, "cargo fmt --all --check").run()?;
        cmd!(sh, "taplo fmt --check").run()?;
        if let Ok(out) = sh.cmd("rg").arg(r"[^\x{00}-\x{ff}\p{L}]").read() {
            return Err(anyhow!("Non-ASCII, non-Latin characters:\n{out}"));
        }

        Ok(())
    }
}
