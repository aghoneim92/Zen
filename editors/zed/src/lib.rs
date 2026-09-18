use zed_extension_api::{self as zed, LanguageServerId, Result};
struct Zen;
impl zed::Extension for Zen {
    fn new() -> Self {
        Self
    }
    fn language_server_command(
        &mut self,
        _: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let command = worktree.which("zen").ok_or("Zen CLI was not found in PATH. Run cargo install --path compiler/crates/zen-cli from the Zen repository, then restart Zed.")?;
        Ok(zed::Command {
            command,
            args: vec!["lsp".into()],
            env: worktree.shell_env(),
        })
    }
}
zed::register_extension!(Zen);
