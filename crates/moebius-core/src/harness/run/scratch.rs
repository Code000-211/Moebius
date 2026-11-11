//! The scratch directory of a run, `<data_dir>/runs/<id>/scratch`: the one place a run may put what it does not need
//! again. Every run on this machine starts with it as its `TMPDIR`, `TEMP` and `TMP` and as `$MOEBIUS_RUN_SCRATCH`, so
//! that a build tree, a `mktemp -d` and the temporary files of a test land in it instead of the shared temporary
//! directory, and its own commands may delete what is in it, which the built-in rule `no-destructive-commands` refuses
//! anywhere else outside the worktree (see `crate::decide::guard`). Moebius removes the whole directory when the run
//! ends, so that nothing a run leaves behind outlives it. A run on a runner has none: it keeps the temporary directory of
//! the runner, which the server neither made nor removes.

use std::path::{Path, PathBuf};

/// The scratch directory of the run `run_id`.
pub(crate) fn dir(data_dir: &Path, run_id: i64) -> PathBuf {
    data_dir
        .join("runs")
        .join(run_id.to_string())
        .join("scratch")
}

/// Makes the scratch directory of the run `run_id` and returns it.
pub(crate) fn make(data_dir: &Path, run_id: i64) -> Result<PathBuf, String> {
    let scratch = dir(data_dir, run_id);
    std::fs::create_dir_all(&scratch)
        .map_err(|e| format!("cannot make the scratch directory of the run: {e}"))?;
    Ok(scratch)
}

/// Removes the scratch directory of the run `run_id` with what is in it. A run that made none leaves nothing to do.
pub(crate) fn remove(data_dir: &Path, run_id: i64) {
    let _ = std::fs::remove_dir_all(dir(data_dir, run_id));
}
