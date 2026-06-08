//! `quay` — the command-line entry point.
//!
//! The CLI is deliberately thin: it parses args, wires the crates together,
//! and prints results. All real logic lives in the library crates so it can be
//! tested and reused (e.g. by a future daemon or LSP).

use anyhow::Result;
use clap::{Parser, Subcommand};
use quay_core::Manifest;
use quay_lock::Lockfile;
use quay_registry::RegistryClient;
use quay_store::Store;

#[derive(Parser)]
#[command(name = "quay", version, about = "An AI-native, Rust-based alternative to npm")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install all dependencies from package.json.
    Install,
    /// Add one or more packages to package.json and install them.
    Add {
        /// Package specs, e.g. `lodash` or `react@18`.
        packages: Vec<String>,
    },
    /// Run a script defined in package.json.
    Run {
        /// The script name from the "scripts" field.
        script: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "quay=info".into()),
        )
        .without_time()
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Install => install().await,
        Command::Add { packages } => add(packages).await,
        Command::Run { script } => run_script(&script),
    }
}

async fn install() -> Result<()> {
    let manifest = Manifest::load("package.json")?;
    let registry = RegistryClient::npm();
    let _store = Store::default_location()?;

    let direct: Vec<(String, String)> = manifest
        .all_dependencies()
        .map(|(n, v)| (n.clone(), v.clone()))
        .collect();

    println!("Resolving {} direct dependencies…", direct.len());
    let resolution = quay_resolver::resolve(&registry, &direct).await?;

    let lockfile = Lockfile::from_resolution(&resolution);
    lockfile.save(".")?;
    println!(
        "Wrote {} ({} packages locked).",
        quay_lock::LOCKFILE_NAME,
        lockfile.packages.len()
    );
    // TODO(quay) M2: extract resolved tarballs into the store + link node_modules.
    Ok(())
}

async fn add(packages: Vec<String>) -> Result<()> {
    // TODO(quay) M3: mutate package.json, then re-run install().
    anyhow::bail!("`quay add {}` not yet implemented (ROADMAP M3)", packages.join(" "))
}

fn run_script(script: &str) -> Result<()> {
    let manifest = Manifest::load("package.json")?;
    let Some(cmd) = manifest.scripts.get(script) else {
        anyhow::bail!("no script named `{script}` in package.json");
    };
    println!("> {cmd}");
    // TODO(quay) M3: run via the system shell with node_modules/.bin on PATH.
    anyhow::bail!("script execution not yet implemented (ROADMAP M3)")
}
