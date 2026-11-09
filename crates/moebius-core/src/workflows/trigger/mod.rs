//! The triggers a workflow file may name in its `[[trigger]]` entry: what starts its runs. A trigger is not a node,
//! so its kind is resolved here and not in the registry of node kinds, which keeps refusing it as a node's kind. Each
//! trigger kind is a module of its own, holding the parameters its trigger takes; the route, schedule or column script
//! that starts runs of it lives with the part of the server that owns that door.

pub mod webhook;
