//! `DevCage` — easy isolation for AI agent development.
//!
//! Scaffold CLI: real isolation backends are not wired yet. Subcommands exist
//! so the layout, docs, and binary entrypoint are stable for implementation.

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

mod cli;
mod profiles;
mod util;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use cli::{Cli, Commands, ProfileCmd};
use profiles::{profile_list_json, profile_show_json};
use util::{not_implemented_message, which};

fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    dispatch(cli)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

fn dispatch(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Doctor => doctor(),
        Commands::Status => status(),
        Commands::Profile { action } => profile_cmd(action),
        Commands::Agent {
            agent,
            offline,
            workspace,
            args,
        } => not_implemented(
            "agent",
            Some(&format!(
                "agent={agent:?} offline={offline} workspace={workspace:?} args={args:?}"
            )),
        ),
        Commands::Run {
            profile,
            workspace,
            command,
        } => not_implemented(
            "run",
            Some(&format!(
                "profile={profile} workspace={workspace:?} cmd={command:?}"
            )),
        ),
        Commands::Shell { profile, workspace } => not_implemented(
            "shell",
            Some(&format!("profile={profile} workspace={workspace:?}")),
        ),
        Commands::Stop { session, wipe } => {
            not_implemented("stop", Some(&format!("session={session:?} wipe={wipe}")))
        },
    }
}

fn doctor() -> anyhow::Result<()> {
    let report = serde_json::json!({
        "product": "devcage",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "scaffold",
        "focus": "AI agent development isolation",
        "core_profiles": ["agent", "offline", "host"],
        "backends": {
            "container": {
                "mvp": true,
                "docker": which("docker"),
                "podman": which("podman"),
            },
            "bwrap": { "mvp": false, "planned": true, "available": which("bwrap") },
        },
        "notes": [
            "Isolation backends are not implemented yet — see docs/product_requirements.md",
            "Happy path (planned): devcage agent",
            "Configure via local.env (see local.env.example); no hardcoded machine paths",
        ],
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn status() -> anyhow::Result<()> {
    let report = serde_json::json!({
        "sessions": [],
        "message": "No session runtime yet (scaffold). Use doctor for environment probes.",
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn profile_cmd(action: ProfileCmd) -> anyhow::Result<()> {
    match action {
        ProfileCmd::List => {
            println!("{}", serde_json::to_string_pretty(&profile_list_json())?);
        },
        ProfileCmd::Show { name } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&profile_show_json(&name))?
            );
        },
    }
    Ok(())
}

fn not_implemented(cmd: &str, detail: Option<&str>) -> anyhow::Result<()> {
    anyhow::bail!("{}", not_implemented_message(cmd, detail))
}
