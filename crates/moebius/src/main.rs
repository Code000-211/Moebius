//! `moebius`: the headless server that holds the boards, schedules the work and drives the harnesses.

use std::path::PathBuf;
use std::process::ExitCode;

use std::sync::Arc;

use clap::{Parser, Subcommand};
use moebius_core::api::{self, ApiState};
use moebius_core::bus::Bus;
use moebius_core::config::{self, Config};
use moebius_core::store::Store;

/// Moebius runs coding-agent harnesses from a repository's issues to changes that are ready to merge.
#[derive(Debug, Parser)]
#[command(name = "moebius", version = moebius_core::VERSION, about)]
struct Cli {
    /// The configuration file.
    #[arg(long, global = true, default_value = "moebius.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write a commented example configuration with every default.
    Init,
    /// Start the server.
    Serve,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("moebius: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        None => {
            println!("moebius {}: see `moebius --help`", moebius_core::VERSION);
            Ok(())
        }
        Some(Command::Init) => {
            if cli.config.exists() {
                return Err(format!(
                    "{} already exists; not overwriting it",
                    cli.config.display()
                ));
            }
            std::fs::write(&cli.config, config::EXAMPLE)
                .map_err(|e| format!("cannot write {}: {e}", cli.config.display()))?;
            println!("wrote {}", cli.config.display());
            Ok(())
        }
        Some(Command::Serve) => {
            let config = Config::load(&cli.config).map_err(|e| e.to_string())?;
            let state = prepare(&config)?;
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| format!("cannot start the runtime: {e}"))?;
            runtime.block_on(async {
                let listener = tokio::net::TcpListener::bind(config.listen)
                    .await
                    .map_err(|e| format!("cannot listen on {}: {e}", config.listen))?;
                println!(
                    "moebius {}: data in {}, app API on {}",
                    moebius_core::VERSION,
                    config.data_dir.display(),
                    config.listen
                );
                tokio::select! {
                    served = api::serve(listener, state) => served.map_err(|e| e.to_string()),
                    _ = tokio::signal::ctrl_c() => Ok(()),
                }
            })
        }
    }
}

/// Creates the data directory, opens the store and reads (or creates) the app's token.
fn prepare(config: &Config) -> Result<ApiState, String> {
    std::fs::create_dir_all(&config.data_dir)
        .map_err(|e| format!("cannot create {}: {e}", config.data_dir.display()))?;
    let store = Store::open(&config.data_dir).map_err(|e| format!("cannot open the store: {e}"))?;
    let token = api::load_or_create_token(&config.token_file)
        .map_err(|e| format!("cannot read {}: {e}", config.token_file.display()))?;
    Ok(ApiState {
        store: Arc::new(store),
        bus: Bus::new(),
        token: token.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command, prepare, run};
    use clap::{CommandFactory, Parser};

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn the_version_flag_prints_the_crate_version() {
        let err = Cli::try_parse_from(["moebius", "--version"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
        assert_eq!(
            err.to_string().trim(),
            format!("moebius {}", moebius_core::VERSION)
        );
    }

    #[test]
    fn init_writes_the_example_and_the_server_prepares_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("moebius.toml");
        let cli = |c| Cli {
            config: path.clone(),
            command: Some(c),
        };
        run(cli(Command::Init)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            moebius_core::config::EXAMPLE
        );
        assert!(run(cli(Command::Init)).is_err());
        prepare(&moebius_core::config::Config::load(&path).unwrap()).unwrap();
        assert!(dir.path().join(".moebius/token").is_file());
        assert!(dir.path().join(".moebius/moebius.sqlite").is_file());
    }
}
