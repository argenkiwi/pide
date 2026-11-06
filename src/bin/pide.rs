//! Sets up and launches a containerised Pi coding agent for a project.
//!
//! Run with `cargo run --bin pide -- [target folder]`.

use std::error::Error as _;
use std::process::ExitCode;

use ambler::walks::pide::Error;

#[tokio::main]
async fn main() -> ExitCode {
    match ambler::walks::pide::run().await {
        Ok(()) => ExitCode::SUCCESS,
        // The walk's FAIL node has already explained what went wrong.
        Err(Error::Failed) => ExitCode::FAILURE,
        Err(e) => {
            let mut message = format!("Error: {e}");
            let mut source = e.source();
            while let Some(cause) = source {
                message = format!("{message}: {cause}");
                source = cause.source();
            }
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
