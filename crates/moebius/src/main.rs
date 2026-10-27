//! `moebius`: the headless server that holds the boards, schedules the work and drives the harnesses.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use moebius_core::config::{self, Config};

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
            std::fs::create_dir_all(&config.data_dir)
                .map_err(|e| format!("cannot create {}: {e}", config.data_dir.display()))?;
            let store = moebius_core::store::Store::open(&config.data_dir)
                .map_err(|e| format!("cannot open the store: {e}"))?;
            println!(
                "moebius {}: store opened at schema version {}",
                moebius_core::VERSION,
                store.schema_version().map_err(|e| e.to_string())?
            );
            println!(
                "moebius {} configured: data in {}, app API on {}",
                moebius_core::VERSION,
                config.data_dir.display(),
                config.listen
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command, run};
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
    fn init_writes_the_example_and_refuses_to_overwrite_it() {
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
        run(cli(Command::Serve)).unwrap();
        assert!(dir.path().join(".moebius").is_dir());
        assert!(dir.path().join(".moebius/moebius.sqlite").is_file());
    }
}
