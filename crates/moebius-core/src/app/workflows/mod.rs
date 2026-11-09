//! The wiring that turns a stored workflow into a run: it stores the run before any node starts, follows the graph on a
//! thread of its own, and records and publishes each node and the run's end as they happen.
//!
//! An approval node takes the run and its node `waiting` for a person's decision (`approval`), which comes back
//! through the resume route, and a run a server stopped while it waited is driven again on the next server start.

pub mod approval;
pub mod run;

pub use approval::DecisionRequest;
pub use run::WorkflowSource;
