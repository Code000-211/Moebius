//! The system a local server runs on, reached through a POSIX shell: `sh` on Linux, and on Windows `wsl.exe -e sh`,
//! which runs it inside WSL 2 (the program, its directory and its token are in the home directory of the WSL
//! distribution). A script gets the shell's `-c`; what it prints is returned.

use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::LocalError;

/// How a shell is started: the program and its arguments before `-c`.
#[derive(Debug, Clone)]
pub struct Host {
    shell: Vec<String>,
}

impl Host {
    /// A host whose shell is `shell` (the program, then arguments that come before `-c`).
    pub fn new(shell: &[&str]) -> Host {
        Host {
            shell: shell.iter().map(|word| word.to_string()).collect(),
        }
    }

    /// The shell of this machine's system, or why there is none.
    pub fn this_machine() -> Result<Host, String> {
        if cfg!(target_os = "linux") {
            Ok(Host::new(&["sh"]))
        } else if cfg!(windows) {
            Ok(Host::new(&["wsl.exe", "-e", "sh"]))
        } else {
            Err("a server of its own runs on Linux, and on Windows inside WSL 2".to_string())
        }
    }

    fn command(&self, script: &str) -> Command {
        let mut command = Command::new(&self.shell[0]);
        command.args(&self.shell[1..]).arg("-c").arg(script);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console window beside the app's
        command
    }

    /// Runs `script` with `stdin` as its input and returns what it printed; a script that fails is an error with what
    /// it printed on its error output.
    pub async fn run(&self, script: &str, stdin: Option<&[u8]>) -> Result<String, LocalError> {
        let mut child = self
            .command(script)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| self.cannot_start(e))?;
        if let (Some(bytes), Some(mut input)) = (stdin, child.stdin.take()) {
            input
                .write_all(bytes)
                .await
                .map_err(|e| LocalError::Step(format!("cannot write to the shell: {e}")))?;
        }
        let out = child
            .wait_with_output()
            .await
            .map_err(|e| LocalError::Step(format!("the shell failed: {e}")))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            let said = String::from_utf8_lossy(&out.stderr);
            Err(LocalError::Step(format!(
                "a step failed ({}): {}",
                out.status,
                said.trim()
            )))
        }
    }

    /// Starts `script` in the background, outliving the app on Unix (a process group of its own, so that the app's
    /// terminal does not end it); the app does not wait for it. On Windows the `wsl.exe` that holds it open is the
    /// app's own child, and keeps the WSL distribution (and with it the server) alive while the app runs.
    pub fn spawn(&self, script: &str) -> Result<(), LocalError> {
        let mut command = self.command(script);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        command.process_group(0);
        let child = command.spawn().map_err(|e| self.cannot_start(e))?;
        // Reaped when it ends, by the runtime; it is not killed when dropped.
        tokio::spawn(async move {
            let mut child = child;
            let _ = child.wait().await;
        });
        Ok(())
    }

    fn cannot_start(&self, error: std::io::Error) -> LocalError {
        LocalError::Unavailable(if cfg!(windows) {
            format!(
                "WSL 2 is not available ({error}): install it with `wsl --install` in a terminal, then start the app again"
            )
        } else {
            format!("cannot start {}: {error}", self.shell[0])
        })
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
    fn a_word_is_quoted_for_the_shell() {
        assert_eq!(quote("it's"), r"'it'\''s'");
    }

    #[cfg(unix)] // runs `sh`
    #[tokio::test]
    async fn a_script_prints_and_a_failing_one_says_why() {
        let host = Host::new(&["sh"]);
        assert_eq!(host.run("echo hi", None).await.unwrap(), "hi\n");
        assert_eq!(host.run("cat", Some(b"in")).await.unwrap(), "in");
        let error = host.run("echo no >&2; exit 3", None).await.unwrap_err();
        assert!(error.to_string().contains("no"), "{error}");
    }

    #[tokio::test]
    async fn a_missing_shell_is_unavailable() {
        let error = Host::new(&["/nonexistent/shell"])
            .run("true", None)
            .await
            .unwrap_err();
        assert!(matches!(error, LocalError::Unavailable(_)), "{error}");
    }
}
