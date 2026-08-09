//! `DevCage` — easy isolation for AI agent development.
//!
//! Runs AI agents and commands inside profiled disposable environments
//! (container backend for `agent`/`offline`, break-glass `host`).

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

mod backend;
mod banner;
mod cli;
mod config;
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

fn main() -> ExitCode {
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
            workspace,
            args,
        } => orchestrate::agent(cfg, agent, offline, workspace, args),
        Commands::Run {
            profile,
            workspace,
            wipe,
            keep,
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
            },
        ),
        Commands::Shell {
            profile,
            workspace,
            wipe,
            keep,
        } => orchestrate::shell(cfg, profile, workspace, wipe, keep),
        Commands::Stop { session, wipe } => {
            orchestrate::stop(cfg, session, wipe)?;
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
    }
}
