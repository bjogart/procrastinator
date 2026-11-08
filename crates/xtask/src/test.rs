use crate::flags;
use anyhow::anyhow;
use xshell::Shell;
use xshell::cmd;

impl flags::Test {
    pub(crate) fn run(self, sh: &Shell) -> anyhow::Result<()> {
        let Self {} = self;

        cmd!(sh, "cargo test --workspace --all-targets").run()?;
        cmd!(sh, "cargo clippy --workspace --all-targets -- -D warnings").run()?;
        cmd!(sh, "cargo fmt --all --check").run()?;
        cmd!(sh, "cargo test --workspace --doc").run()?;
        cmd!(sh, "cargo doc --workspace --no-deps").run()?;

        const NON_ASCII_RE: &str = r"[^\x{00}-\x{ff}\p{L}]";
        let ascii_check = sh.cmd("rg").arg(NON_ASCII_RE);
        eprintln!("$ {ascii_check}");
        match ascii_check.read() {
            Ok(out) => return Err(anyhow!("Non-ASCII, non-Latin characters:\n{out}")),
            Err(err) => {
                let err = format!("{err}");
                if err != format!("command exited with non-zero code `rg {NON_ASCII_RE}`: 1") {
                    return Err(anyhow!("rg failed with error: {err}"));
                }
            }
        }

        cmd!(sh, "taplo fmt --check").run()?;

        Ok(())
    }
}
