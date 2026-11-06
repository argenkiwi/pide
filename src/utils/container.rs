//! Driving a container engine (`podman` or `docker`) through its command-line tool.

use std::env;
use std::io;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Engines pide can drive, in order of preference.
pub const ENGINES: &[&str] = &["podman", "docker"];

/// Returns `true` if an executable named `name` is on `PATH`.
#[must_use]
pub fn is_installed(name: &str) -> bool {
    env::var_os("PATH")
        .is_some_and(|path| env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// Returns `true` if `engine info` succeeds, meaning the engine is up and answering.
pub async fn is_running(engine: &str) -> bool {
    Command::new(engine)
        .arg("info")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
}

/// Reads label `key` from `image`.
///
/// Returns `None` if the image does not exist or does not have the label.
///
/// # Errors
///
/// Returns an error if the engine cannot be run at all.
pub async fn image_label(engine: &str, image: &str, key: &str) -> io::Result<Option<String>> {
    let output = Command::new(engine)
        .args([
            "image",
            "inspect",
            "--format",
            &format!("{{{{ index .Config.Labels \"{key}\" }}}}"),
            image,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((output.status.success() && !value.is_empty() && value != "<no value>").then_some(value))
}

/// Builds `image` from `containerfile` with `context` as the build context, adding `labels`.
/// The engine's output is shown to the user.
///
/// Returns whether the build succeeded.
///
/// # Errors
///
/// Returns an error if the engine cannot be run at all.
pub async fn build(
    engine: &str,
    image: &str,
    containerfile: &Path,
    context: &Path,
    labels: &[(&str, &str)],
) -> io::Result<bool> {
    let mut command = Command::new(engine);
    command
        .args(["build", "--tag", image, "--file"])
        .arg(containerfile);
    for (key, value) in labels {
        command.arg("--label").arg(format!("{key}={value}"));
    }
    let status = command.arg(context).stdin(Stdio::null()).status().await?;
    Ok(status.success())
}

/// Runs `engine` with `args`, attached to this terminal, and waits for it to exit.
///
/// Returns the exit code, or `None` if it was killed by a signal.
///
/// # Errors
///
/// Returns an error if the engine cannot be run at all.
pub async fn run(engine: &str, args: &[String]) -> io::Result<Option<i32>> {
    let status = Command::new(engine).args(args).status().await?;
    Ok(status.code())
}
