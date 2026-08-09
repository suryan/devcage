//! Clap CLI surface for `DevCage` (scaffold).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// `DevCage` — easy isolation for AI agent development.
#[derive(Debug, Parser)]
#[command(name = "devcage", version, about, long_about = None)]
pub(crate) struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Probe host tooling and print scaffold status (JSON).
    Doctor,
    /// List active sessions (none until runtime exists).
    Status,
    /// Inspect built-in / configured profiles.
    Profile {
        /// Profile subcommand.
        #[command(subcommand)]
        action: ProfileCmd,
    },
    /// Run an AI agent CLI inside a cage (happy path; not implemented yet).
    Agent {
        /// Agent command name (default: `DEVCAGE_DEFAULT_AGENT` or `claude`).
        #[arg(env = "DEVCAGE_DEFAULT_AGENT")]
        agent: Option<String>,
        /// Use offline profile (no network) instead of agent.
        #[arg(long)]
        offline: bool,
        /// Workspace directory to project into the cage (default: cwd).
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
        /// Extra args passed to the agent binary.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run a command inside a profile (not implemented yet).
    Run {
        /// Profile name: `agent` | `offline` | `host`.
        #[arg(
            long,
            short = 'p',
            default_value = "agent",
            env = "DEVCAGE_DEFAULT_PROFILE"
        )]
        profile: String,
        /// Workspace directory to project into the cage (default: cwd).
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
        /// Command and args after `--`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Interactive shell inside a profile (not implemented yet).
    Shell {
        /// Profile name: `agent` | `offline` | `host`.
        #[arg(
            long,
            short = 'p',
            default_value = "agent",
            env = "DEVCAGE_DEFAULT_PROFILE"
        )]
        profile: String,
        /// Workspace directory to project into the cage (default: cwd).
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
    },
    /// Stop a session (not implemented yet).
    Stop {
        /// Session id (when runtime exists).
        session: Option<String>,
        /// Discard ephemeral volumes for the session.
        #[arg(long)]
        wipe: bool,
    },
}

/// Profile subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum ProfileCmd {
    /// List known profiles.
    List,
    /// Show one profile (schema TBD).
    Show {
        /// Profile name.
        name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands};
    use clap::Parser;

    #[test]
    fn parses_doctor() {
        let cli = Cli::try_parse_from(["devcage", "doctor"]).expect("parse");
        assert!(matches!(cli.command, Commands::Doctor));
    }

    #[test]
    fn parses_agent_offline() {
        let cli = Cli::try_parse_from(["devcage", "agent", "--offline", "claude"]).expect("parse");
        let Commands::Agent { agent, offline, .. } = cli.command else {
            unreachable!("expected Agent");
        };
        assert!(offline);
        assert_eq!(agent.as_deref(), Some("claude"));
    }

    #[test]
    fn parses_run_default_profile_agent() {
        let cli = Cli::try_parse_from(["devcage", "run", "--", "echo", "hi"]).expect("parse");
        let Commands::Run {
            profile, command, ..
        } = cli.command
        else {
            unreachable!("expected Run");
        };
        assert_eq!(profile, "agent");
        assert_eq!(command, vec!["echo", "hi"]);
    }
}
