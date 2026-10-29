//! The call a refused judgement is about, read back from the judgement's context: its tool, its input, the run's
//! worktree and repository, and only when the call is reversible.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::app::{App, AppError, Result};
use crate::decide::guard;
use crate::store::{Judgement, Repository, Run};

/// A refused call, read from its judgement.
pub(super) struct Call {
    pub tool: String,
    pub input: Value,
    /// Where the run's worktree was (or is).
    pub worktree: PathBuf,
    pub run: Run,
    pub repository: Repository,
}

/// The call the judgement refused, when it can be replayed; else why not.
pub(super) fn call_of(app: &App, judgement: &Judgement) -> Result<Call> {
    let not = |why: &str| AppError::Invalid(format!("judgement {} {why}", judgement.id));
    if judgement.kind != "tool_call" || judgement.choice != "refuse" {
        return Err(not("is not a refused tool call"));
    }
    let run = judgement
        .run_id
        .and_then(|id| app.store.run(id).transpose())
        .transpose()?
        .ok_or_else(|| not("has no run"))?;
    let repository = match run.card_id {
        Some(card) => app
            .store
            .card(card)?
            .map(|card| card.repository_id)
            .or(run.repository_id),
        None => run.repository_id,
    };
    let repository = repository
        .and_then(|id| app.store.repository(id).transpose())
        .transpose()?
        .filter(|repository| Path::new(&repository.path).is_dir())
        .ok_or_else(|| not("is about a run whose repository is gone"))?;
    let context = judgement
        .context
        .first()
        .ok_or_else(|| not("has no context"))?;
    let context: BTreeMap<String, String> = serde_json::from_str(&app.store.blobs().get(context)?)
        .map_err(|_| not("has a context that cannot be read"))?;
    let tool = context.get("tool").ok_or_else(|| not("names no tool"))?;
    let input: Value = context
        .get("input")
        .and_then(|input| serde_json::from_str(input).ok())
        .ok_or_else(|| not("has no input to the tool"))?;
    let worktree = context
        .get("worktree")
        .ok_or_else(|| not("is about a run without a worktree"))?;
    guard::reversible(tool, &input, Path::new(worktree))
        .map_err(|why| not(&format!("is not reversible: {why}")))?;
    Ok(Call {
        tool: tool.clone(),
        input,
        worktree: PathBuf::from(worktree),
        run,
        repository,
    })
}
