//! What differs between the systems the server builds on: Linux (and WSL 2), where it is documented and tested, and
//! Windows, where it builds and starts. Every other module calls these functions instead of `std::os::unix`, so the
//! differences stay in one place:
//!
//! - A program that may start others runs in its own process group on Linux, and the group is killed with it; on
//!   Windows it gets a new process group and its tree of processes is killed with `taskkill /T` (a process whose
//!   parent has already ended is not reached).
//!   A terminal's shell leads a session, whose every process (background jobs have groups of their own) is killed with it
//!   on Linux.
//! - A file that holds a secret is created readable by its owner only (mode `0600`) on Linux; on Windows it keeps the
//!   permissions of its directory, which under the person's profile is readable by them and the administrators.
//! - A skill is linked into a run's configuration on Linux and copied on Windows.
//! - The name of the computer comes from the environment, `/etc/hostname` or the `hostname` program.
//! - A person's terminal runs their shell (`$SHELL`) on Linux and PowerShell on Windows, both on a pseudo-terminal
//!   (ConPTY on Windows, through `portable-pty`).
//! - The repository's own checks, replays and research measurements run `sh` and `bash`; on Windows, where they are
//!   usually missing, the error says that this needs Linux or WSL.

use std::fs::OpenOptions;
use std::path::Path;
use std::process::Command;

/// What a feature that needs a Unix shell says on Windows when the shell is not found.
pub const NEEDS_UNIX: &str = "this needs Linux or WSL; run the Moebius server there";

/// Starts `command` as the leader of its own process group, so that [`kill_tree`] stops what it started too.
pub fn own_group(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP)
    }
}

/// Kills the process `pid` alone.
pub fn kill_process(pid: u32) {
    #[cfg(unix)]
    let mut kill = {
        let mut kill = Command::new("kill");
        kill.args(["-KILL", &pid.to_string()]);
        kill
    };
    #[cfg(windows)]
    let mut kill = {
        let mut kill = Command::new("taskkill");
        kill.args(["/F", "/PID", &pid.to_string()]);
        kill
    };
    let _ = kill
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Kills the process `pid`, which leads its own process group (see [`own_group`]; a shell on a pseudo-terminal does
/// too), and every process it started.
pub fn kill_tree(pid: u32) {
    #[cfg(unix)]
    let mut kill = {
        let mut kill = Command::new("kill");
        kill.args(["-KILL", "--", &format!("-{pid}")]);
        kill
    };
    #[cfg(windows)]
    let mut kill = {
        let mut kill = Command::new("taskkill");
        kill.args(["/T", "/F", "/PID", &pid.to_string()]);
        kill
    };
    let _ = kill
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Kills every process of the session that the process `pid` leads, which is what a shell on a pseudo-terminal is:
/// its background jobs have groups of their own in the same session, so [`kill_tree`] alone leaves them running. On
/// Windows it is [`kill_tree`].
pub fn kill_session(pid: u32) {
    // A member can fork between the scan and its own kill, so scan again until none is left (bounded: a zombie that
    // is not yet collected is not counted, but a fork bomb must not hold this forever).
    #[cfg(unix)]
    for _ in 0..20 {
        let members = session_members(pid);
        if members.is_empty() {
            break;
        }
        let _ = Command::new("kill")
            .args(["-KILL", "--"])
            .args(members.iter().map(u32::to_string))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    kill_tree(pid);
}

/// The processes whose session is `sid` and that still run (no zombies), from `/proc`; none where there is no `/proc`.
#[cfg(unix)]
fn session_members(sid: u32) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            // `pid (name) state ppid pgrp session ...`, the session being the fourth field after the name; the name may hold spaces and parentheses.
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return false;
            };
            let Some((_, rest)) = stat.rsplit_once(')') else {
                return false;
            };
            let mut fields = rest.split_whitespace();
            let running = fields.next() != Some("Z");
            running
                && fields
                    .nth(2)
                    .is_some_and(|session| session.parse() == Ok(sid))
        })
        .collect()
}

/// Whether the process `pid` is running: it exists and is no zombie waiting to be collected.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn is_running(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        let after_name = stat.rsplit(')').next().unwrap_or("");
        !after_name.trim_start().starts_with('Z')
    })
}

/// Makes `options` create a file only its owner can read and write (see the module's note for Windows).
pub fn owner_only(options: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600)
    }
    #[cfg(windows)]
    {
        options
    }
}

/// Lets the owner run the existing file `path`, which a downloaded program needs.
pub fn make_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
    }
    #[cfg(windows)]
    {
        let _ = path;
        Ok(())
    }
}

/// Gives the existing file `path` the permissions of [`owner_only`].
pub fn make_owner_only(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(windows)]
    {
        let _ = path;
        Ok(())
    }
}

/// Puts the directory `from` at `to`: a symbolic link on Linux, a copy on Windows, where links need a privilege.
pub fn link_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(from, to)
    }
    #[cfg(windows)]
    {
        copy_dir(from, to)
    }
}

#[cfg(windows)]
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// `error`, from starting the Unix program `program`, with [`NEEDS_UNIX`] added on Windows when it was not found.
pub fn unix_program_error(program: &str, error: std::io::Error) -> std::io::Error {
    if cfg!(windows) && error.kind() == std::io::ErrorKind::NotFound {
        std::io::Error::new(
            error.kind(),
            format!("`{program}` was not found: {NEEDS_UNIX}"),
        )
    } else {
        error
    }
}

/// The shell a person gets on a terminal page: `$SHELL`, else `/bin/sh` on Linux; PowerShell on Windows.
pub fn shell() -> String {
    if cfg!(windows) {
        return "powershell.exe".to_string();
    }
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string())
}

/// The name of this computer: `COMPUTERNAME` or `HOSTNAME`, else `/etc/hostname`, else what `hostname` prints; "unknown"
/// when none of them tells.
pub fn host_name() -> String {
    let from_env = ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok());
    let found = from_env
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .or_else(|| {
            let out = Command::new("hostname").output().ok()?;
            String::from_utf8(out.stdout).ok()
        })
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    found.unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_private_file_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret");
        owner_only(OpenOptions::new().write(true).create(true))
            .open(&path)
            .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        make_owner_only(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_linked_directory_shows_its_files() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("skill");
        std::fs::create_dir_all(from.join("refs")).unwrap();
        std::fs::write(from.join("SKILL.md"), "a skill").unwrap();
        std::fs::write(from.join("refs/one.md"), "one").unwrap();
        let to = dir.path().join("linked");
        link_dir(&from, &to).unwrap();
        assert_eq!(
            std::fs::read_to_string(to.join("SKILL.md")).unwrap(),
            "a skill"
        );
        assert_eq!(
            std::fs::read_to_string(to.join("refs/one.md")).unwrap(),
            "one"
        );
    }

    #[test]
    fn a_missing_unix_program_says_what_it_needs_on_windows() {
        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        let error = unix_program_error("sh", missing).to_string();
        assert_eq!(error.contains(NEEDS_UNIX), cfg!(windows), "{error}");
    }

    #[test]
    fn a_program_in_its_own_group_is_killed_with_what_it_started() {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-Command", "Start-Sleep 600"]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 600 & wait"]);
            command
        };
        let mut child = own_group(&mut command).spawn().unwrap();
        kill_tree(child.id());
        assert!(!child.wait().unwrap().success());
    }

    // Linux only: the session's members are read from `/proc`, and `setsid` starts the session.
    #[cfg(target_os = "linux")]
    #[test]
    fn killing_a_session_leaves_none_of_its_processes() {
        let mut command = Command::new("setsid");
        command.args(["sh", "-c", "sleep 600 & sleep 601 & wait"]);
        let mut child = command.spawn().unwrap();
        let pid = child.id();
        // The shell and its two jobs are in the session.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while session_members(pid).len() < 3 {
            assert!(std::time::Instant::now() < deadline, "the jobs start");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        kill_session(pid);
        let _ = child.wait();
        assert_eq!(session_members(pid), Vec::<u32>::new());
    }
}
