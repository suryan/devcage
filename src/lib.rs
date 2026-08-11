//! `DevCage` library — profiled disposable environments for AI agent work.
//!
//! The CLI binary is a thin wrapper around [`run_cli`].

// Tests use expect/unwrap for concise fixtures; production code still denies them.
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::wildcard_enum_match_arm,
        clippy::assertions_on_constants,
    )
)]

mod auth_seed;
mod backend;
mod banner;
mod cli;
mod config;
mod events;
mod grok_mode;
mod limits;
mod net_policy;
mod orchestrate;
mod plan;
mod profiles;
mod session;
mod util;

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use cli::{Cli, Commands};
use config::Config;

/// Parse CLI args and run the selected command.
///
/// # Errors
///
/// Command failures are returned as `ExitCode::FAILURE` after printing the error;
/// this function only returns `Err` if initialization itself fails (unused today).
#[must_use]
pub fn run_cli() -> ExitCode {
    init_tracing();
    let cfg = Config::from_env();
    let cli = Cli::parse();
    match dispatch(&cfg, cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("devcage error: {err:#}");
            ExitCode::FAILURE
        },
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

#[allow(clippy::too_many_lines)]
fn dispatch(cfg: &Config, cli: Cli) -> anyhow::Result<ExitCode> {
    match cli.command {
        Commands::Doctor => {
            orchestrate::doctor(cfg)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::Status => {
            orchestrate::status(cfg)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::Profile { action } => {
            orchestrate::profile_cmd(action)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::Agent {
            agent,
            offline,
            host_restricted,
            workspace,
            args,
        } => orchestrate::agent(cfg, agent, offline, host_restricted, workspace, args),
        Commands::Run {
            profile,
            workspace,
            wipe,
            keep,
            allow_host_profile,
            command,
        } => orchestrate::run(
            cfg,
            orchestrate::RunParams {
                profile,
                workspace,
                command,
                tty: false,
                wipe_after: wipe,
                keep,
                allow_host_profile,
            },
        ),
        Commands::Shell {
            profile,
            workspace,
            wipe,
            keep,
            allow_host_profile,
        } => orchestrate::shell(cfg, profile, workspace, wipe, keep, allow_host_profile),
        Commands::Stop { session, wipe } => {
            orchestrate::stop(cfg, session, wipe)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::Export {
            session,
            source,
            dest,
        } => {
            orchestrate::export(cfg, &session, &source, &dest)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::Logs { session } => {
            orchestrate::logs(cfg, &session)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::InitAllowlist { path } => {
            orchestrate::init_allowlist(&path)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::Explain { profile, workspace } => {
            orchestrate::explain(cfg, &profile, workspace)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::Verify => orchestrate::verify(cfg),
        Commands::Pull => {
            orchestrate::pull(cfg)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::Clean { all } => {
            orchestrate::clean(cfg, all)?;
            Ok(ExitCode::SUCCESS)
        },
        Commands::Limits => limits::run_limits(cfg),
        Commands::Grok {
            sandbox,
            strict,
            no_init,
            force_init,
            allow_host_profile,
            print,
            workspace,
            args,
        } => grok_mode::run_grok_mode(
            cfg,
            grok_mode::GrokModeParams {
                sandbox: if strict { "strict".to_owned() } else { sandbox },
                no_init,
                force_init,
                allow_host_profile,
                print_only: print,
                workspace,
                args,
            },
        ),
        Commands::InitGrok { force, workspace } => {
            let ws = config::resolve_workspace(workspace)?;
            let written = grok_mode::init_grok_recipes(&ws, force)?;
            if written.is_empty() {
                println!(
                    "devcage: .grok recipes already present under {} (use --force to overwrite)",
                    ws.display()
                );
            } else {
                println!("devcage: wrote {}", written.join(", "));
            }
            Ok(ExitCode::SUCCESS)
        },
    }
}
