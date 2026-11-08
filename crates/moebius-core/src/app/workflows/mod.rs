//! The wiring that turns a stored workflow into a run: it stores the run before any node starts, follows the graph on a
//! thread of its own, and records and publishes each node and the run's end as they happen.

pub mod run;

pub use run::WorkflowSource;
