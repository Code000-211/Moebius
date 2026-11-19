//! Which start of the app is the first. The app runs once: the first start takes a lock in the app's configuration
//! directory and listens on the loopback, writing the port next to the lock; a later start finds the lock taken, tells
//! the first one through that port to bring its window forward, and ends. The lock is the operating system's, so it is
//! gone when the first app is, however it ended. The Tauri side (`window`) starts the app only on [`Start::First`] and
//! shows the main window whenever [`Instance::wait`] is called back.

use std::fs::{self, File, TryLockError};
use std::io::{self, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::Path;

const LOCK: &str = "instance.lock";
const PORT: &str = "instance.port";

/// What this start of the app is.
pub enum Start {
    /// No other app runs: this one goes on, and listens for later starts.
    First(Instance),
    /// Another app runs: it has been asked to show its window (when it could be reached), and this one ends.
    Second,
}

/// The first app's hold on the lock and the listener a later start calls on.
pub struct Instance {
    listener: TcpListener,
    // Held for the lock, which the system releases when the file is closed.
    _lock: File,
}

/// Decides, with the lock file in `dir`, whether this is the first start; a later start has already told the first
/// one to show its window when this returns.
pub fn acquire(dir: &Path) -> io::Result<Start> {
    fs::create_dir_all(dir)?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK))?;
    match lock.try_lock() {
        Ok(()) => {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
            fs::write(dir.join(PORT), listener.local_addr()?.port().to_string())?;
            Ok(Start::First(Instance {
                listener,
                _lock: lock,
            }))
        }
        Err(TryLockError::WouldBlock) => {
            // An unreachable first app (it has just started, or is closing) is not an error: this one ends anyway.
            let _ = signal(dir);
            Ok(Start::Second)
        }
        Err(TryLockError::Error(e)) => Err(e),
    }
}

/// Tells the first app to show its window.
fn signal(dir: &Path) -> io::Result<()> {
    let port: u16 = fs::read_to_string(dir.join(PORT))?
        .trim()
        .parse()
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    TcpStream::connect((Ipv4Addr::LOCALHOST, port))?.write_all(b"show\n")
}

impl Instance {
    /// Calls `on_start` for every later start, until the listener fails; run it on a thread of its own.
    pub fn wait(self, mut on_start: impl FnMut()) {
        for connection in self.listener.incoming() {
            if connection.is_err() {
                break;
            }
            on_start();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn first(dir: &Path) -> Instance {
        match acquire(dir).expect("acquire") {
            Start::First(instance) => instance,
            Start::Second => panic!("expected the first start"),
        }
    }

    #[test]
    fn the_first_start_is_first_and_a_later_one_is_second_and_shows_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let instance = first(dir.path());
        let (sender, shown) = mpsc::channel();
        std::thread::spawn(move || instance.wait(|| sender.send(()).unwrap()));

        assert!(matches!(acquire(dir.path()).unwrap(), Start::Second));
        shown
            .recv()
            .expect("the first app is asked to show its window");
        assert!(matches!(acquire(dir.path()).unwrap(), Start::Second));
        shown.recv().expect("every later start asks again");
    }

    #[test]
    fn a_start_after_the_first_ended_is_first_again() {
        let dir = tempfile::tempdir().unwrap();
        drop(first(dir.path()));
        drop(first(dir.path()));
    }

    #[test]
    fn a_second_start_ends_even_when_the_first_cannot_be_reached() {
        let dir = tempfile::tempdir().unwrap();
        let _held = first(dir.path());
        fs::remove_file(dir.path().join(PORT)).unwrap();
        assert!(matches!(acquire(dir.path()).unwrap(), Start::Second));
        fs::write(dir.path().join(PORT), "not a port").unwrap();
        assert!(matches!(acquire(dir.path()).unwrap(), Start::Second));
    }
}
