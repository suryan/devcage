//! Clap CLI surface for `DevCage` (scaffold).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// `DevCage` — easy isolation for AI agent development.
#[derive(Debug, Parser)]
#[command(name = "devcage", version, about, long_about = None)]
pub struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
pub enum Commands {
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
        /// Agent command name (default: `DEVCAGE_DEFAULT_AGENT` or `grok`).
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
        /// Allow profile `host` even when `DEVCAGE_DENY_HOST=1` (e.g. under `devcage grok`).
        #[arg(long)]
        allow_host_profile: bool,
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
        /// Allow profile `host` even when `DEVCAGE_DENY_HOST=1`.
        #[arg(long)]
        allow_host_profile: bool,
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
    /// Show session event log (JSON lines).
    Logs {
        /// Session id.
        session: String,
    },
    /// Write a starter net-allowlist file.
    InitAllowlist {
        /// Output path (e.g. .devcage/net-allowlist.txt).
        #[arg(default_value = ".devcage/net-allowlist.txt")]
        path: PathBuf,
    },
    /// Print the isolation plan for a profile without starting a session.
    Explain {
        /// Profile name.
        #[arg(
            long,
            short = 'p',
            default_value = "agent",
            env = "DEVCAGE_DEFAULT_PROFILE"
        )]
        profile: String,
        /// Workspace override.
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
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
    /// Probe isolation limits (sandbox + nested `DevCage`) as JSON.
    Limits,
    /// Combined mode: Grok OS sandbox + DevCage-ready env (recommended daily).
    ///
    /// Seeds `.grok/` recipes if missing, sets `DEVCAGE_DATA_DIR` for nested
    /// `devcage run`, scrubs ambient secrets from the child env, denies profile
    /// `host` by default, then execs `grok --sandbox <profile> …`.
    Grok {
        /// Grok sandbox profile (`keys-first`, `workspace`, `strict`, …).
        #[arg(long, default_value = "keys-first", env = "DEVCAGE_GROK_SANDBOX")]
        sandbox: String,
        /// Shortcut for `--sandbox strict` (maximum Grok FS sandbox).
        #[arg(long)]
        strict: bool,
        /// Do not seed `.grok/sandbox.toml` / `.grok/config.toml`.
        #[arg(long)]
        no_init: bool,
        /// Overwrite existing `.grok` recipes when seeding.
        #[arg(long)]
        force_init: bool,
        /// Allow nested `devcage run -p host` (sets `DEVCAGE_DENY_HOST=0`).
        #[arg(long)]
        allow_host_profile: bool,
        /// Print the planned command and env; do not start Grok.
        #[arg(long)]
        print: bool,
        /// Project workspace (default: cwd or git root).
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
        /// Extra args forwarded to `grok` (e.g. `-p "…"`, `--always-approve`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Write example Grok sandbox + permission recipes into `.grok/`.
    InitGrok {
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
        /// Project workspace (default: cwd or git root).
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
    },
    /// Connector isolation mode: box one MCP server. The agent stays on the host.
    ///
    /// Chain it like the sister tools, only for connectors you choose:
    ///
    ///   mcp-guard → mcp-secret-launcher → devcage mcp → uvx|npx → server
    ///
    /// The server keeps the secret injected for its profile. It does not get
    /// Keychain, the SSH agent, or the rest of the host home.
    ///
    /// Examples:
    ///   devcage mcp -- uvx my-server==1.2.3
    ///   devcage mcp --net none --share-workspace ro -- npx -y some-server@1.0.0
    ///   devcage mcp --print -- uvx my-server==1.2.3
    ///   devcage mcp --open-urls -- npx -y mcp-remote@0.16.0 <https://mcp.atlassian.com/v1/mcp/authv2>
    Mcp {
        /// Print the plan and do not start the connector.
        #[arg(long)]
        print: bool,
        /// Explain the cage on stderr. Stdout stays the connector's stream.
        #[arg(long)]
        verbose: bool,
        /// Linux container instead of a light wrap (Seatbelt / bwrap).
        ///
        /// On macOS the guest is Linux, so a Darwin `uvx`/`npx` binary cannot run there.
        #[arg(long)]
        container: bool,
        /// Start even when `sandbox-exec` and `bwrap` are missing.
        ///
        /// Env-only does not limit filesystem or Keychain access.
        #[arg(long)]
        allow_env_only: bool,
        /// Forward connector sign-in links to the host.
        ///
        /// `--open-urls` prints the link and opens it when a graphical session
        /// exists. `--open-urls=print` only prints the link. Off by default.
        /// The display socket, `DBus` session bus, and `/run/user` stay outside the cage.
        #[arg(
            long,
            require_equals = true,
            num_args = 0..=1,
            default_missing_value = "open",
            value_name = "MODE",
            value_parser = ["open", "print"]
        )]
        open_urls: Option<String>,
        /// Process network: `allowed` (default) or `none`.
        #[arg(long, default_value = "allowed")]
        net: String,
        /// Workspace shared into the cage: `none` (default), `ro`, or `rw`.
        #[arg(long, default_value = "none")]
        share_workspace: String,
        /// Project directory when `--share-workspace` is `ro` or `rw`.
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
        /// Connector command and args (use `--` before the command).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        command: Vec<String>,
    },
    /// Run **any** agent/command under `DevCage` process wrap (no agent sandbox required).
    ///
    /// On macOS: `sandbox-exec` Seatbelt around the process. Always: scrub env,
    /// sandbox-safe `DEVCAGE_DATA_DIR`, deny nested `host` profile by default.
    ///
    /// Examples:
    ///   devcage wrap -- claude
    ///   devcage wrap -- cursor-agent
    ///   devcage wrap -- codex
    ///   devcage wrap --print -- true
    Wrap {
        /// Print plan only; do not run the command.
        #[arg(long)]
        print: bool,
        /// Allow nested `devcage run -p host`.
        #[arg(long)]
        allow_host_profile: bool,
        /// Do not apply Seatbelt (env wiring only).
        #[arg(long)]
        no_seatbelt: bool,
        /// Remap HOME to ephemeral session home (may break agent login/skills).
        #[arg(long)]
        ephemeral_home: bool,
        /// Process network: `allowed` (default) or `none` (Seatbelt deny; macOS).
        #[arg(long, default_value = "allowed")]
        net: String,
        /// Project workspace (default: cwd or git root).
        #[arg(long, short = 'w', env = "DEVCAGE_WORKSPACE")]
        workspace: Option<PathBuf>,
        /// Command and args (use `--` before the command).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        command: Vec<String>,
    },
}

/// Profile subcommands.
#[derive(Debug, Subcommand)]
pub enum ProfileCmd {
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
        let cli = Cli::try_parse_from(["devcage", "agent", "--offline", "grok"]).expect("parse");
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
        assert_eq!(agent.as_deref(), Some("grok"));
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

    #[test]
    fn parses_grok_mode() {
        let cli = Cli::try_parse_from([
            "devcage",
            "grok",
            "--sandbox",
            "workspace",
            "--print",
            "-p",
            "hi",
        ])
        .expect("parse");
        let Commands::Grok {
            sandbox,
            print,
            args,
            no_init,
            strict,
            allow_host_profile,
            ..
        } = cli.command
        else {
            unreachable!("expected Grok");
        };
        assert_eq!(sandbox, "workspace");
        assert!(print);
        assert!(!no_init);
        assert!(!strict);
        assert!(!allow_host_profile);
        assert_eq!(args, vec!["-p", "hi"]);
    }

    #[test]
    fn parses_limits() {
        let cli = Cli::try_parse_from(["devcage", "limits"]).expect("parse");
        assert!(matches!(cli.command, Commands::Limits));
    }

    #[test]
    fn parses_wrap() {
        let cli = Cli::try_parse_from(["devcage", "wrap", "--print", "--", "claude", "-p", "hi"])
            .expect("parse");
        let Commands::Wrap {
            print,
            command,
            no_seatbelt,
            ephemeral_home,
            ..
        } = cli.command
        else {
            unreachable!("expected Wrap");
        };
        assert!(print);
        assert!(!no_seatbelt);
        assert!(!ephemeral_home);
        assert_eq!(command, vec!["claude", "-p", "hi"]);
    }

    #[test]
    fn parses_mcp_defaults() {
        let cli = Cli::try_parse_from(["devcage", "mcp", "--print", "--", "uvx", "server==1.2.3"])
            .expect("parse");
        let Commands::Mcp {
            print,
            verbose,
            container,
            allow_env_only,
            open_urls,
            net,
            share_workspace,
            command,
            ..
        } = cli.command
        else {
            unreachable!("expected Mcp");
        };
        assert!(print);
        assert!(!verbose);
        assert!(!container);
        assert!(!allow_env_only);
        assert!(open_urls.is_none());
        assert_eq!(net, "allowed");
        assert_eq!(share_workspace, "none");
        assert_eq!(command, vec!["uvx", "server==1.2.3"]);
    }

    #[test]
    fn parses_open_urls_flag() {
        let open =
            Cli::try_parse_from(["devcage", "mcp", "--open-urls", "--", "true"]).expect("open");
        let Commands::Mcp { open_urls, .. } = open.command else {
            unreachable!("expected Mcp");
        };
        assert_eq!(open_urls.as_deref(), Some("open"));

        let print = Cli::try_parse_from(["devcage", "mcp", "--open-urls=print", "--", "true"])
            .expect("print");
        let Commands::Mcp { open_urls, .. } = print.command else {
            unreachable!("expected Mcp");
        };
        assert_eq!(open_urls.as_deref(), Some("print"));

        assert!(Cli::try_parse_from(["devcage", "mcp", "--open-urls=file", "--", "true"]).is_err());
    }

    #[test]
    fn parses_init_grok() {
        let cli = Cli::try_parse_from(["devcage", "init-grok", "--force"]).expect("parse");
        assert!(matches!(
            cli.command,
            Commands::InitGrok { force: true, .. }
        ));
    }
}
