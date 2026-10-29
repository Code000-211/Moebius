//! The plugin's side of the JSON Lines protocol, for a plugin written in Rust: [`serve`] runs a [`Plugin`] as its own
//! program, the way `examples/allow_reads_process.rs` does.

use std::io::{BufRead, Write};

use crate::{Envelope, Plugin};

/// Writes `plugin`'s manifest to `output`, then answers every call read from `input`, one at a time, until `input`
/// ends. A line that is not a call is skipped with a line on standard error.
pub fn serve(
    plugin: &dyn Plugin,
    input: impl BufRead,
    mut output: impl Write,
) -> std::io::Result<()> {
    let mut send = |envelope: &Envelope| -> std::io::Result<()> {
        let line = serde_json::to_string(envelope)?;
        writeln!(output, "{line}")?;
        output.flush()
    };
    send(&Envelope::Manifest {
        manifest: plugin.manifest(),
    })?;
    for line in input.lines() {
        let line = line?;
        let Ok(Envelope::Call { id, call }) = serde_json::from_str(&line) else {
            eprintln!("{}: not a call: {line}", plugin.manifest().name);
            continue;
        };
        send(&match plugin.call(call) {
            Ok(result) => Envelope::Reply { id, result },
            Err(error) => Envelope::Error {
                id,
                message: error.to_string(),
            },
        })?;
    }
    Ok(())
}
