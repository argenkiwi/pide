//! Reading the host's git configuration.

use std::process::{Command, Stdio};

/// Returns the value of git config `key` (e.g. `user.name`), or `None` if it is unset, empty, or
/// git is not installed.
#[must_use]
pub fn config(key: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", key])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !value.is_empty()).then_some(value)
}
