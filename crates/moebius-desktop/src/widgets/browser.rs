//! Opening an address in the person's browser.

use std::process::{Command, Stdio};

/// The command that hands `url` to the system's browser. macOS's `open` and Linux's `xdg-open` take the address
/// themselves; Windows has neither, so the shell's protocol handler is asked through `rundll32`, which opens the page
/// rather than reading the address as a folder.
fn command(url: &str) -> Command {
    let mut command = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Opens `url` in the person's browser; the page shows the address as well, for when that fails.
pub fn open(url: &str) {
    let _ = command(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_command_carries_the_address() {
        let url = "https://example.test/auth";
        let command = command(url);
        let program = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(windows) {
            "rundll32"
        } else {
            "xdg-open"
        };
        assert_eq!(command.get_program().to_str(), Some(program));
        assert!(command.get_args().any(|arg| arg == url));
        #[cfg(windows)]
        assert!(
            command
                .get_args()
                .any(|arg| arg == "url.dll,FileProtocolHandler")
        );
    }
}
