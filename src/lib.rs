//! PiDE (Pi Development Environment): sets up and launches a containerised Pi coding agent.
//!
//! Built as a walk (see [`engine`]): small nodes wired together by [`walks::pide`].

pub mod engine;
pub mod nodes;
pub mod utils;
pub mod walks;
