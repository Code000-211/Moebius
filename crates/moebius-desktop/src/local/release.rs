//! The release of `moebius` the app brings: its file for this system, downloaded from the release of the app's own
//! version, checked against that release's `SHA256SUMS`, and put in `~/.moebius/bin` through the host's shell.

use sha2::{Digest, Sha256};

use super::host::{Host, quote};
use super::{BINARY, LocalError};
use crate::server::VERSION;

/// Where the files of this version's release are, unless a test says another place.
pub fn base() -> String {
    format!(
        "{}/releases/download/v{VERSION}",
        env!("CARGO_PKG_REPOSITORY")
    )
}

/// The Rust target triple of the release for a system as `uname -sm` says it: releases are built for Linux on x86_64.
pub fn target(system: &str) -> Result<&'static str, String> {
    match system.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["Linux", "x86_64"] => Ok("x86_64-unknown-linux-gnu"),
        [] => Err("cannot tell the system of this machine".to_string()),
        _ => Err(format!(
            "no release of Moebius for {}: releases are built for Linux x86_64 (on Windows, inside WSL 2)",
            system.trim()
        )),
    }
}

/// The hash of the file `name` in the text of a `SHA256SUMS` file.
fn sum_of(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let (hash, file) = (words.next()?, words.next()?);
        let hash_ok = hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit());
        (hash_ok && file.trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, LocalError> {
    let failed = |reason: String| LocalError::Download {
        url: url.to_string(),
        reason,
    };
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| failed(e.without_url().to_string()))?;
    if !response.status().is_success() {
        return Err(failed(format!("answered {}", response.status())));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| failed(e.without_url().to_string()))?;
    Ok(bytes.to_vec())
}

/// Downloads `moebius-<target>` and `SHA256SUMS` from `base`, checks the one against the other, and puts the program in
/// `~/.moebius/bin/moebius` of `host` (a new file, replaced at once, so a failed install leaves the old program). The
/// program's path on the host is returned.
pub async fn install(host: &Host, base: &str, target: &str) -> Result<String, LocalError> {
    let name = format!("moebius-{target}");
    let url = format!("{base}/{name}");
    let sums_url = format!("{base}/SHA256SUMS");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| LocalError::Step(e.to_string()))?;
    let program = download(&client, &url).await?;
    let sums = download(&client, &sums_url).await?;
    let expected =
        sum_of(&String::from_utf8_lossy(&sums), &name).ok_or_else(|| LocalError::Download {
            url: sums_url,
            reason: format!("it does not list {name}"),
        })?;
    let actual: String = Sha256::digest(&program)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected {
        return Err(LocalError::Checksum { url });
    }
    let target_path = format!("$HOME/{BINARY}");
    let script = format!(
        "set -e; umask 077; mkdir -p \"$HOME/.moebius/bin\"; t=\"{target_path}\"; \
         trap 'rm -f \"$t.part\"' EXIT; cat > \"$t.part\"; chmod +x \"$t.part\"; mv \"$t.part\" \"$t\"; \
         echo \"MOEBIUS-PATH $t\""
    );
    let out = host.run(&script, Some(&program)).await?;
    out.lines()
        .find_map(|line| line.strip_prefix("MOEBIUS-PATH "))
        .map(|path| path.trim().to_string())
        .ok_or_else(|| {
            LocalError::Step(format!(
                "the install did not say where it put the program: {}",
                quote(&out)
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_linux_x86_64_has_a_release() {
        assert_eq!(target("Linux x86_64\n"), Ok("x86_64-unknown-linux-gnu"));
        assert!(
            target("Linux aarch64")
                .unwrap_err()
                .contains("Linux x86_64")
        );
        assert!(target("").is_err());
    }

    #[test]
    fn a_hash_is_found_by_the_name_of_the_file() {
        let hash = "a".repeat(64);
        let sums = format!("{hash}  moebius-x\n{}  *other\n", "b".repeat(64));
        assert_eq!(sum_of(&sums, "moebius-x"), Some(hash));
        assert_eq!(sum_of(&sums, "other"), Some("b".repeat(64)));
        assert_eq!(sum_of(&sums, "missing"), None);
        assert_eq!(sum_of("short  moebius-x", "moebius-x"), None);
    }
}
