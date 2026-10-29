//! How Moebius gets better from what it recorded: the nightly Retro (`retro`), the trials of what it proposes
//! (`trial`), what it proposes for Moebius's own configuration (`own`), replays of recorded runs (`replay`),
//! autoresearch (`research`), the training set (`export`) and the lessons the Retro learns from earlier cards
//! (`lessons`).

pub mod export;
pub mod lessons;
pub mod own;
pub mod replay;
pub mod research;
pub mod retro;
pub mod trial;
