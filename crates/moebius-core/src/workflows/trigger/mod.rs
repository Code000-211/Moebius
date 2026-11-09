//! The triggers of a workflow: what starts its runs. A trigger is the only card of a run that starts it, so the
//! triggers of a workflow run in parallel with each other and with the run's node cards. A workflow file names one
//! kind in `[[trigger]]`, and each kind has a module of its own here: `schedule` starts a run when a cron comes due,
//! and `manual` names the typed inputs a person starts each of its runs with.

pub mod manual;
pub mod schedule;
