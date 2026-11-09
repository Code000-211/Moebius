//! The triggers of a workflow: what starts its runs. A trigger is the only card of a run that starts it, so the
//! triggers of a workflow run in parallel with each other and with the run's node cards. The `manual` trigger names
//! its inputs — each with a name, a type, whether it is required and a default — and a person starts each run giving
//! them.

pub mod manual;
