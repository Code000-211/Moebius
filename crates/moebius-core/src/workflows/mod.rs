//! Workflows: the graphs a person keeps in the harness configuration to say what happens from a trigger to the work it
//! starts. The folder holds the definition read from the repository, the expressions that fill a node's parameters from
//! earlier nodes, the triggers that start runs (the `manual` one with its typed inputs and the `webhook` one with its path) and the registry of
//! node kinds an executor builds a graph from.

pub mod decorate;
pub mod definition;
pub mod nodes;
pub mod run;
pub mod trigger;

pub use definition::{Edge, NodeDefinition, Position, Trigger, WorkflowDefinition};
pub use nodes::{Metadata, Node, NodeError, Ports, node};
