//! Connecting to a server over SSH, the way an editor's remote mode does: the system's `ssh` client does the login (keys,
//! the agent, `~/.ssh/config`, jump hosts and known hosts all work as in a terminal), the app sets the server up on the
//! other side, and a tunnel carries the desktop API to a port of this machine.
//!
//! `config` reads and extends the hosts of the SSH configuration, `askpass` shows the prompts of `ssh` in the app,
//! `remote` is the setup of the server over SSH as a stream of progress, and `tunnel` is the port forward that keeps
//! itself alive. None of it knows the pages; the connect screen and the app use it, and so can anything else that needs a
//! port of the server's machine.

pub mod askpass;
pub mod config;
pub mod remote;
pub mod tunnel;

use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use askpass::Askpass;

/// The `ssh` client of a connection: the program, and the app that answers its prompts.
#[derive(Debug, Clone)]
pub struct Ssh {
    program: PathBuf,
    askpass: Option<Askpass>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SshError {
    #[error("cannot start {program}: {message}")]
    Spawn { program: String, message: String },
    #[error("{0}")]
    Failed(String),
}

/// What a command on the other side printed, as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Ssh {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Ssh {
            program: program.into(),
            askpass: None,
        }
    }

    /// The client that asks the app for passwords and passphrases.
    pub fn with_askpass(mut self, askpass: Askpass) -> Self {
        self.askpass = Some(askpass);
        self
    }

    pub fn askpass(&self) -> Option<&Askpass> {
        self.askpass.as_ref()
    }

    pub fn program(&self) -> String {
        self.program.to_string_lossy().into_owned()
    }

    /// A command that runs `ssh` with `args` and the environment that makes it ask the app. It dies with the task that
    /// holds it, and opens no console window on Windows.
    pub(crate) fn command(&self, args: &[String]) -> Command {
        let mut command = Command::new(&self.program);
        command.args(args).kill_on_drop(true);
        if let Some(askpass) = &self.askpass {
            command.envs(askpass.environment());
        }
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        command
    }

    /// Runs `script` in a POSIX shell on `host`, with `stdin` for it to read, and waits for it to end. `on_prompt` is
    /// told of each password or passphrase `ssh` asks while it waits.
    pub async fn run(
        &self,
        host: &str,
        script: &str,
        stdin: Option<Vec<u8>>,
        mut on_prompt: impl FnMut(String),
    ) -> Result<Output, SshError> {
        let args = vec![
            "-T".to_string(),
            "-o".to_string(),
            "ConnectTimeout=30".to_string(),
            host.to_string(),
            format!("sh -c {}", quote(script)),
        ];
        let mut command = self.command(&args);
        command
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|e| SshError::Spawn {
            program: self.program(),
            message: e.to_string(),
        })?;
        if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
            tokio::spawn(async move {
                let _ = pipe.write_all(&bytes).await;
                let _ = pipe.shutdown().await;
            });
        }
        let mut finished = Box::pin(child.wait_with_output());
        let output = loop {
            tokio::select! {
                output = &mut finished => break output,
                Some(prompt) = next_prompt(self.askpass.as_ref()) => on_prompt(prompt),
            }
        }
        .map_err(|e| SshError::Failed(e.to_string()))?;
        Ok(Output {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// The next prompt of the app's askpass, or never when there is none.
pub(crate) async fn next_prompt(askpass: Option<&Askpass>) -> Option<String> {
    match askpass {
        Some(askpass) => askpass.prompt().await,
        None => std::future::pending().await,
    }
}

/// `text` as one word for a POSIX shell.
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_is_quoted_for_a_posix_shell() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), r"'it'\''s'");
    }
}
