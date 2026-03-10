use clap::ValueEnum;
use clap_complete::Shell;

/// Supported shells for completion generation.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ShellArg {
    /// Bourne Again SHell
    Bash,
    /// Z Shell
    Zsh,
    /// Friendly Interactive SHell
    Fish,
}

impl ShellArg {
    /// Convert to the corresponding `clap_complete::Shell` variant.
    fn to_shell(self) -> Shell {
        match self {
            ShellArg::Bash => Shell::Bash,
            ShellArg::Zsh => Shell::Zsh,
            ShellArg::Fish => Shell::Fish,
        }
    }
}

/// Generate shell completions and write them to stdout.
pub fn generate(shell: ShellArg, cmd: &mut clap::Command) {
    clap_complete::generate(shell.to_shell(), cmd, "devpulse", &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_arg_to_shell_bash() {
        assert_eq!(ShellArg::Bash.to_shell(), Shell::Bash);
    }

    #[test]
    fn test_shell_arg_to_shell_zsh() {
        assert_eq!(ShellArg::Zsh.to_shell(), Shell::Zsh);
    }

    #[test]
    fn test_shell_arg_to_shell_fish() {
        assert_eq!(ShellArg::Fish.to_shell(), Shell::Fish);
    }

    #[test]
    fn test_shell_arg_value_enum_variants() {
        let variants = ShellArg::value_variants();
        assert_eq!(variants.len(), 3);
    }

    #[test]
    fn test_generate_does_not_panic() {
        // Verify generation works for all shells without panicking.
        // We redirect output to a buffer to avoid printing during tests.
        for shell in [ShellArg::Bash, ShellArg::Zsh, ShellArg::Fish] {
            let mut cmd = clap::Command::new("devpulse")
                .arg(clap::Arg::new("path").default_value("."))
                .subcommand(clap::Command::new("completions").arg(clap::Arg::new("shell")));
            // Generate to a buffer instead of stdout
            let mut buf = Vec::new();
            clap_complete::generate(shell.to_shell(), &mut cmd, "devpulse", &mut buf);
            let output = String::from_utf8(buf).expect("completions should be valid UTF-8");
            assert!(
                !output.is_empty(),
                "completions output for {:?} should not be empty",
                shell
            );
            // All shells should reference the binary name
            assert!(
                output.contains("devpulse"),
                "completions should reference 'devpulse'"
            );
        }
    }
}
