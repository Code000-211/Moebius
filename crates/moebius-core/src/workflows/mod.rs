//! Workflows: the graphs a person keeps in the harness configuration to say what happens from a trigger to the work it
//! starts. The folder holds the definition read from the repository and the registry of node kinds an executor builds a
//! graph from.

pub mod definition;
pub mod nodes;
pub mod run;

pub use definition::{Edge, NodeDefinition, Position, Trigger, WorkflowDefinition};
pub use nodes::{Metadata, Node, NodeError, Ports, node};
