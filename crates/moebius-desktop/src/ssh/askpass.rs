//! Passwords and passphrases for `ssh` without a terminal: `ssh` runs the program named by `SSH_ASKPASS` for every
//! prompt, and that program is this app, started again. It forwards the prompt over the loopback to the running app,
//! which shows it and answers; the answer goes to `ssh` on standard output and is kept nowhere.

use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

/// The variable that tells a started copy of the app to act as the askpass program, with the address to ask at.
pub const ADDRESS_VARIABLE: &str = "MOEBIUS_ASKPASS_ADDR";
/// The variable with the secret that proves the asker was started by this app.
pub const NONCE_VARIABLE: &str = "MOEBIUS_ASKPASS_NONCE";

struct Inner {
    address: SocketAddr,
    nonce: String,
    program: PathBuf,
    /// Who waits for the answer to the prompt that is shown.
    pending: Mutex<Option<oneshot::Sender<Option<String>>>>,
    prompts: tokio::sync::Mutex<mpsc::UnboundedReceiver<String>>,
    listener: tokio::task::JoinHandle<()>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

/// The listener that the askpass program asks, shared by everything that starts `ssh` for one connection.
#[derive(Clone)]
pub struct Askpass(Arc<Inner>);

impl std::fmt::Debug for Askpass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Askpass")
    }
}

impl Askpass {
    /// Listens on a free port of the loopback. `program` is what `ssh` is to run: the app's own executable.
    pub async fn start(program: PathBuf) -> std::io::Result<Askpass> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let nonce = nonce();
        let (prompts_in, prompts) = mpsc::unbounded_channel();
        let inner = Arc::new_cyclic(|weak: &std::sync::Weak<Inner>| {
            let (weak, task_nonce) = (weak.clone(), nonce.clone());
            let listener = tokio::spawn(async move {
                loop {
                    let Ok((socket, _)) = listener.accept().await else {
                        return;
                    };
                    let (weak, nonce, prompts_in) =
                        (weak.clone(), task_nonce.clone(), prompts_in.clone());
                    tokio::spawn(async move {
                        let _ = serve(socket, weak, &nonce, prompts_in).await;
                    });
                }
            });
            Inner {
                address,
                nonce,
                program,
                pending: Mutex::new(None),
                prompts: tokio::sync::Mutex::new(prompts),
                listener,
            }
        });
        Ok(Askpass(inner))
    }

    /// The environment that makes `ssh` ask this app.
    pub fn environment(&self) -> Vec<(String, String)> {
        let mut env = vec![
            (
                "SSH_ASKPASS".to_string(),
                self.0.program.to_string_lossy().into_owned(),
            ),
            ("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string()),
            (ADDRESS_VARIABLE.to_string(), self.0.address.to_string()),
            (NONCE_VARIABLE.to_string(), self.0.nonce.clone()),
        ];
        // Older clients use the program only when a display is set.
        if std::env::var_os("DISPLAY").is_none() {
            env.push(("DISPLAY".to_string(), "moebius".to_string()));
        }
        env
    }

    /// The text of the next prompt `ssh` asks, waiting for it.
    pub async fn prompt(&self) -> Option<String> {
        self.0.prompts.lock().await.recv().await
    }

    /// Answers the prompt that is shown; `None` cancels it, which fails the login.
    pub fn answer(&self, answer: Option<String>) {
        if let Some(waiting) = self.0.pending.lock().expect("askpass lock").take() {
            let _ = waiting.send(answer);
        }
    }
}

/// A secret that is different on every start, from the clock and the address of a fresh allocation.
fn nonce() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    format!("{:016x}", hasher.finish())
}

/// One question from the program: the nonce, then the prompt, then the answer or a refusal back.
async fn serve(
    mut socket: tokio::net::TcpStream,
    inner: std::sync::Weak<Inner>,
    nonce: &str,
    prompts: mpsc::UnboundedSender<String>,
) -> std::io::Result<()> {
    let mut request = String::new();
    socket.read_to_string(&mut request).await?;
    let Some((sent, prompt)) = request.split_once('\n') else {
        return Ok(());
    };
    if sent != nonce {
        return Ok(());
    }
    let Some(inner) = inner.upgrade() else {
        return Ok(());
    };
    let (answer_in, answer) = oneshot::channel();
    *inner.pending.lock().expect("askpass lock") = Some(answer_in);
    drop(inner);
    let _ = prompts.send(prompt.trim_end().to_string());
    let reply = match answer.await {
        Ok(Some(answer)) => format!("Y{answer}"),
        _ => "N".to_string(),
    };
    socket.write_all(reply.as_bytes()).await?;
    socket.shutdown().await
}

/// What the app does when `ssh` started it as the askpass program: ask the running app and print the answer. Returns
/// the exit status; failing tells `ssh` the prompt was cancelled.
pub fn helper(prompt: &str) -> i32 {
    let (Ok(address), Ok(nonce)) = (
        std::env::var(ADDRESS_VARIABLE),
        std::env::var(NONCE_VARIABLE),
    ) else {
        return 1;
    };
    match ask(&address, &nonce, prompt) {
        Ok(Some(answer)) => {
            let mut out = std::io::stdout();
            if writeln!(out, "{answer}").and_then(|()| out.flush()).is_ok() {
                0
            } else {
                1
            }
        }
        _ => 1,
    }
}

fn ask(address: &str, nonce: &str, prompt: &str) -> std::io::Result<Option<String>> {
    let mut socket = std::net::TcpStream::connect(address)?;
    socket.write_all(format!("{nonce}\n{prompt}").as_bytes())?;
    socket.shutdown(std::net::Shutdown::Write)?;
    let mut reply = String::new();
    socket.read_to_string(&mut reply)?;
    Ok(reply.strip_prefix('Y').map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_prompt_reaches_the_app_and_the_answer_comes_back() {
        let askpass = Askpass::start("askpass".into()).await.unwrap();
        let env = askpass.environment();
        let get = |name: &str| env.iter().find(|(k, _)| k == name).unwrap().1.clone();
        let (address, nonce) = (get(ADDRESS_VARIABLE), get(NONCE_VARIABLE));
        let asked = tokio::task::spawn_blocking({
            let (address, nonce) = (address.clone(), nonce.clone());
            move || ask(&address, &nonce, "leon@host's password: ")
        });
        assert_eq!(
            askpass.prompt().await.as_deref(),
            Some("leon@host's password:")
        );
        askpass.answer(Some("s3cret".into()));
        assert_eq!(asked.await.unwrap().unwrap().as_deref(), Some("s3cret"));

        let cancelled = tokio::task::spawn_blocking({
            let (address, nonce) = (address.clone(), nonce.clone());
            move || ask(&address, &nonce, "Passphrase: ")
        });
        askpass.prompt().await.unwrap();
        askpass.answer(None);
        assert_eq!(cancelled.await.unwrap().unwrap(), None);

        // A question without the nonce is not shown.
        let stranger = tokio::task::spawn_blocking(move || ask(&address, "wrong", "Password: "));
        assert_eq!(stranger.await.unwrap().unwrap(), None);
    }
}
