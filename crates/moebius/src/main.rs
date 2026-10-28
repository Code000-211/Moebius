//! `moebius`: the headless server that holds the boards, schedules the work and drives the harnesses.

use std::path::PathBuf;
use std::process::ExitCode;

use std::sync::Arc;

use clap::{Parser, Subcommand};
use moebius_core::board;
use moebius_core::config::{self, Config};
use moebius_core::export;
use moebius_core::server::Server;
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
    /// Write the judgements with their contexts and outcomes as JSON Lines, one file per kind, for training.
    Export {
        /// The directory to write `<kind>.jsonl` files to.
        #[arg(long)]
        out: PathBuf,
        /// Only this kind of judgement; give the option again for more kinds.
        #[arg(long = "kind")]
        kinds: Vec<String>,
    },
    /// Run a column script once against a made-up card and print what it did, or the error (exit status 1).
    CheckScript {
        /// The script, such as `.moebius/ready.luau`.
        file: PathBuf,
        /// The column the made-up card is in.
        #[arg(long, default_value = "Backlog")]
        column: String,
        /// The card's title.
        #[arg(long, default_value = "A card to check the script with")]
        title: String,
        /// The card's author; `person` is trusted, as on a repository without a forge.
        #[arg(long, default_value = "person")]
        author: String,
    },
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
        Some(Command::CheckScript {
            file,
            column,
            title,
            author,
        }) => {
            let source = std::fs::read_to_string(&file)
                .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
            let name = file.file_name().unwrap_or(file.as_os_str());
            let did =
                board::check_script(&name.to_string_lossy(), &source, &column, &title, &author)?;
            if did.is_empty() {
                println!("The script did nothing: it did not move, wait, ask or start anything.");
            }
            for line in did {
                println!("{line}");
            }
            Ok(())
        }
        Some(Command::Export { out, kinds }) => {
            let config = Config::load(&cli.config).map_err(|e| e.to_string())?;
            if !config.data_dir.join("moebius.sqlite").is_file() {
                return Err(format!("no store in {}", config.data_dir.display()));
            }
            let store = Store::open(&config.data_dir).map_err(|e| e.to_string())?;
            let kinds = (!kinds.is_empty()).then_some(kinds);
            let summary =
                export::export(&store, &out, kinds.as_deref()).map_err(|e| e.to_string())?;
            for written in summary.kinds {
                let c = written.counts;
                println!(
                    "{}: {} judgements, {} with an outcome, {} by a person, {} held out",
                    written.file.display(),
                    c.count,
                    c.with_outcome,
                    c.by_person,
                    c.held_out
                );
            }
            Ok(())
        }
        Some(Command::Serve) => {
            let config = Config::load(&cli.config).map_err(|e| e.to_string())?;
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| format!("cannot start the runtime: {e}"))?;
            runtime.block_on(async {
                let api = tokio::net::TcpListener::bind(config.listen)
                    .await
                    .map_err(|e| format!("cannot listen on {}: {e}", config.listen))?;
                let router = tokio::net::TcpListener::bind(config.router_listen)
                    .await
                    .map_err(|e| format!("cannot listen on {}: {e}", config.router_listen))?;
                let router_addr = router.local_addr().map_err(|e| e.to_string())?;
                let server = Arc::new(Server::new(config, router_addr)?);
                println!(
                    "moebius {}: data in {}, app API on {}, router on {router_addr}",
                    moebius_core::VERSION,
                    server.config.data_dir.display(),
                    server.config.listen,
                );
                tokio::select! {
                    served = server.serve(api, router) => served.map_err(|e| e.to_string()),
                    _ = tokio::signal::ctrl_c() => Ok(()),
                }
            })
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
        let config = moebius_core::config::Config::load(&path).unwrap();
        moebius_core::server::Server::new(config, "127.0.0.1:7172".parse().unwrap()).unwrap();
        assert!(dir.path().join(".moebius/token").is_file());
        assert!(dir.path().join(".moebius/moebius.sqlite").is_file());
    }
}
