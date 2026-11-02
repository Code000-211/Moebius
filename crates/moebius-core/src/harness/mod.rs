//! The coding-agent harnesses and what they are given: the run lifecycle every harness shares (`run`), Pi-Bolt runs and chats (`pibolt`), Claude Code runs and chats
//! (`claude`), the harness of each agent picked by its `harness` (`pick`), shells in a worktree and on the server
//! (`terminal`), the harness configuration repository with the agents, skills, MCP servers and rules (`repo`, `skill`,
//! `mcp`), the MCP gateway through which a run reaches its MCP servers (`gateway`), the chats that end in issues
//! (`grilling`), the outputs of tools kept small where they are made (`output`), fast lint and type feedback after an agent's edits (`lens`), and the machines that run a card's harness over SSH (`remote`).

pub mod claude;
pub mod gateway;
pub mod grilling;
pub mod lens;
pub mod mcp;
pub mod output;
pub mod pibolt;
pub mod pick;
pub mod remote;
pub mod repo;
pub mod run;
pub mod skill;
pub mod terminal;

/// The variables of the server's environment a harness process and the MCP servers of the gateway keep: where programs
/// are, who runs them and in which language and terminal, how to reach the network behind a proxy and which
/// certificates to trust, where the toolchains are, and what Windows needs to start anything. Every other one,
/// such as a provider key named by `api_key_env`, is left out, so that an agent cannot read what only the router may
/// hold.
pub(crate) const KEPT_VARIABLES: [&str; 38] = [
    "PATH",
    "HOME",
    "USER",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TMPDIR",
    "TEMP",
    "TMP",
    "SYSTEMROOT",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PATHEXT",
    "COMSPEC",
    "SHELL",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "all_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "WINDIR",
    "SystemDrive",
    "ProgramData",
    "ProgramFiles",
    "USERNAME",
    "HOMEDRIVE",
    "HOMEPATH",
    "CLAUDE_CODE_GIT_BASH_PATH",
];

/// Has `command` start with the environment cleared but for [`KEPT_VARIABLES`]; the variables Moebius sets go on top,
/// so this comes before any `.env(..)`.
pub(crate) fn clear_environment(command: &mut std::process::Command) {
    command.env_clear();
    for name in KEPT_VARIABLES {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}

// Unix only: the test starts `env`.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_cleared_environment_keeps_only_what_a_program_needs_and_what_is_set_after() {
        // Cargo sets this variable for the test, as a key named by `api_key_env` is set for the server.
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
        let mut command = std::process::Command::new("env");
        clear_environment(&mut command);
        let out = command.env("MOEBIUS_RUN_TOKEN", "t").output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        let names: Vec<_> = text.lines().filter_map(|l| l.split_once('=')).collect();
        assert!(names.iter().any(|(n, _)| *n == "PATH"), "{text}");
        assert!(
            names
                .iter()
                .any(|(n, v)| *n == "MOEBIUS_RUN_TOKEN" && *v == "t")
        );
        assert!(
            !names.iter().any(|(n, _)| *n == "CARGO_MANIFEST_DIR"),
            "{text}"
        );
    }
}
