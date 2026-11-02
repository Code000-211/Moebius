//! Downloading what a package installs and checking it against the sha256 of the index before anything is written.

use std::time::Duration;

use sha2::{Digest, Sha256};

use super::{PackageError, Result};
use crate::blobs::redact;

/// How long a download may take.
const TIME_LIMIT: Duration = Duration::from_secs(300);

/// The most bytes a download may have, so that a registry cannot fill the disk.
const SIZE_LIMIT: u64 = 512 * 1024 * 1024;

/// The bytes at `url`, if their sha256 is `sha256` (hexadecimal, in either case). Blocking.
pub fn fetch(url: &str, sha256: &str) -> Result<Vec<u8>> {
    let failed = |message: String| PackageError::Download {
        url: redact(url),
        message: redact(&message),
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(TIME_LIMIT)
        .build()
        .map_err(|e| failed(e.to_string()))?;
    let response = client.get(url).send().map_err(|e| failed(e.to_string()))?;
    if !response.status().is_success() {
        return Err(failed(format!("the server answered {}", response.status())));
    }
    if response.content_length().is_some_and(|n| n > SIZE_LIMIT) {
        return Err(failed("the file is too large".into()));
    }
    let bytes = response.bytes().map_err(|e| failed(e.to_string()))?;
    if bytes.len() as u64 > SIZE_LIMIT {
        return Err(failed("the file is too large".into()));
    }
    let actual = hex::encode(Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(sha256) {
        return Err(PackageError::ShaMismatch {
            url: redact(url),
            expected: sha256.to_lowercase(),
            actual,
        });
    }
    Ok(bytes.to_vec())
}
