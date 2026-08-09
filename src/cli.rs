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
    /// Run an AI agent CLI inside a cage (happy path).
    Agent {
        /// Agent command name (default: `DEVCAGE_DEFAULT_AGENT` or `claude`).
        #[arg(env = "DEVCAGE_DEFAULT_AGENT")]
        agent: Option<String>,
        /// Use offline profile (no network) instead of agent (requires container).
        #[arg(long)]
        offline: bool,
        /// Force host-restricted mode (ephemeral HOME; not a container).
        #[arg(long)]
        host_restricted: bool,
        /// Workspace directory to project into the cage (default: cwd).
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
        /// Extra args passed to the agent binary.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run a command inside a profile.
    Run {
        /// Profile name: `agent` | `offline` | `untrusted` | `host`.
        #[arg(
            long,
            short = 'p',
            default_value = "agent",
            env = "DEVCAGE_DEFAULT_PROFILE"
        )]
        profile: String,
        /// Workspace directory to project into the cage (default: cwd or git root).
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
        /// Discard ephemeral session home after the command exits.
        #[arg(long)]
        wipe: bool,
        /// Keep session home even if the profile would wipe by default.
        #[arg(long)]
        keep: bool,
        /// Command and args after `--`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Interactive shell inside a profile.
    Shell {
        /// Profile name: `agent` | `offline` | `untrusted` | `host`.
        #[arg(
            long,
            short = 'p',
            default_value = "agent",
            env = "DEVCAGE_DEFAULT_PROFILE"
        )]
        profile: String,
        /// Workspace directory to project into the cage (default: cwd or git root).
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
        /// Discard ephemeral session home after the shell exits.
        #[arg(long)]
        wipe: bool,
        /// Keep session home even if the profile would wipe by default.
        #[arg(long)]
        keep: bool,
    },
    /// Stop a session and optionally wipe ephemeral data.
    Stop {
        /// Session id (when runtime exists).
        session: Option<String>,
        /// Discard ephemeral volumes for the session.
        #[arg(long)]
        wipe: bool,
    },
    /// Copy a path from a session home or workspace into a host destination.
    Export {
        /// Session id.
        session: String,
        /// Path relative to session home, or absolute under workspace/home.
        source: PathBuf,
        /// Destination path on the host.
        dest: PathBuf,
    },
    /// Isolation smoke checks (workspace safety, runtime, optional container probe).
    Verify,
    /// Pull the configured container image.
    Pull,
    /// Wipe exited/stopped sessions from the data directory.
    Clean {
        /// Also wipe sessions still marked running.
        #[arg(long)]
        all: bool,
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
        let Commands::Agent {
            agent,
            offline,
            host_restricted,
            ..
        } = cli.command
        else {
            unreachable!("expected Agent");
        };
        assert!(offline);
        assert!(!host_restricted);
        assert_eq!(agent.as_deref(), Some("claude"));
    }

    #[test]
    fn parses_agent_host_restricted() {
        let cli =
            Cli::try_parse_from(["devcage", "agent", "--host-restricted", "grok"]).expect("parse");
        let Commands::Agent {
            agent,
            host_restricted,
            ..
        } = cli.command
        else {
            unreachable!("expected Agent");
        };
        assert!(host_restricted);
        assert_eq!(agent.as_deref(), Some("grok"));
    }

    #[test]
    fn parses_run_default_profile_agent() {
        let cli = Cli::try_parse_from(["devcage", "run", "--", "echo", "hi"]).expect("parse");
        let Commands::Run {
            profile,
            command,
            wipe,
            keep,
            ..
        } = cli.command
        else {
            unreachable!("expected Run");
        };
        assert_eq!(profile, "agent");
        assert!(!wipe);
        assert!(!keep);
        assert_eq!(command, vec!["echo", "hi"]);
    }

    #[test]
    fn parses_run_wipe() {
        let cli = Cli::try_parse_from(["devcage", "run", "--wipe", "-p", "offline", "--", "true"])
            .expect("parse");
        let Commands::Run { wipe, profile, .. } = cli.command else {
            unreachable!("expected Run");
        };
        assert!(wipe);
        assert_eq!(profile, "offline");
    }
}
