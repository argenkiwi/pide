//! Driving a container engine through its command-line tool.
//!
//! pide supports `podman` and `docker` (which share one CLI) and Apple's `container`
//! (<https://github.com/apple/container>), a macOS-only tool whose CLI differs in a few ways:
//!
//! - it reports health with `system status`, not `info`;
//! - `image inspect` prints a JSON document with no `--format` template, so labels are read out of
//!   the JSON rather than via a template;
//! - `run` has no `--add-host` flag, so the host cannot be mapped the podman/docker way.
//!
//! `build` and most of `run` (`-i`, `-t`, `--rm`, `-v`, `-w`, `-e`) use the same flags across all
//! three, so those paths stay shared.

use std::env;
use std::io;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Engines pide can drive, in order of preference.
///
/// `podman` and `docker` come first because they work on every platform; Apple's `container` is
/// last because it only runs on macOS with Apple silicon.
pub const ENGINES: &[&str] = &["podman", "docker", "container"];

/// The name of Apple's `container` tool, the one engine whose CLI pide special-cases.
const APPLE_CONTAINER: &str = "container";

/// Returns `true` if `engine` is Apple's `container` tool.
#[must_use]
pub fn is_apple_container(engine: &str) -> bool {
    engine == APPLE_CONTAINER
}

/// Returns `true` if an executable named `name` is on `PATH`.
#[must_use]
pub fn is_installed(name: &str) -> bool {
    env::var_os("PATH")
        .is_some_and(|path| env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// Returns `true` if the engine is up and answering.
///
/// `podman` and `docker` answer to `info`; Apple's `container` answers to `system status`.
pub async fn is_running(engine: &str) -> bool {
    let mut command = Command::new(engine);
    if is_apple_container(engine) {
        command.args(["system", "status"]);
    } else {
        command.arg("info");
    }
    command
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
    if is_apple_container(engine) {
        return apple_container_image_label(image, key).await;
    }

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

/// Reads a label out of Apple's `container image inspect` JSON, which has no `--format` option and
/// so must be parsed. Apple nests the labels at `variants[].config.config.Labels`; the search walks
/// the whole document (case-insensitively on `Labels`) rather than hard-coding that path, so it
/// also matches a podman/docker-style `Config.Labels`.
///
/// # Errors
///
/// Returns an error if `container` cannot be run at all.
async fn apple_container_image_label(image: &str, key: &str) -> io::Result<Option<String>> {
    let output = Command::new(APPLE_CONTAINER)
        .args(["image", "inspect", image])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await?;
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8_lossy(&output.stdout);
    let Some(parsed) = serde_json::from_str::<serde_json::Value>(&value).ok() else {
        return Ok(None);
    };
    let mut found = None;
    collect_label(&parsed, key, &mut found);
    Ok(found)
}

/// Walks a parsed inspection document looking for the first `Labels`/`labels` object that carries
/// `key`, returning a non-empty value for it.
fn collect_label(value: &serde_json::Value, key: &str, found: &mut Option<String>) {
    if found.is_some() {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for (name, item) in map {
                if found.is_some() {
                    return;
                }
                if name.eq_ignore_ascii_case("labels") {
                    if let Some(label) = item.get(key).and_then(serde_json::Value::as_str) {
                        *found = Some(label.to_string());
                        return;
                    }
                }
                collect_label(item, key, found);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_label(item, key, found);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
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

/// The extra `run` flags that map the host machine into a container's DNS, so a container can
/// reach a service on the host (e.g. the LLM server).
///
/// `podman` and `docker` do this with `--add-host host.docker.internal:host-gateway`. Apple's
/// `container` has no such flag: a container reaches the host through its network gateway, which
/// the user must arrange separately (see the README), so the mapping is left to them and this
/// returns no flags for it.
#[must_use]
pub fn host_args(engine: &str, host: &str) -> Vec<String> {
    if is_apple_container(engine) {
        return Vec::new();
    }
    vec!["--add-host".to_string(), format!("{host}:host-gateway")]
}

/// The option suffix appended to a bind-mount spec (`host:container`).
///
/// `podman` and `docker` take `:z` so shared SELinux labels permit container writes. Apple's
/// `container` fails to start with it (`mount failed with errno 22 ... data=z`), so it gets none.
#[must_use]
pub fn volume_options(engine: &str) -> &'static str {
    if is_apple_container(engine) { "" } else { ":z" }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_is_the_third_engine() {
        assert_eq!(ENGINES, ["podman", "docker", "container"]);
    }

    #[test]
    fn only_apple_container_is_special_cased() {
        assert!(is_apple_container("container"));
        assert!(!is_apple_container("podman"));
        assert!(!is_apple_container("docker"));
    }

    #[test]
    fn podman_and_docker_map_the_host_with_add_host() {
        assert_eq!(
            host_args("podman", "host.docker.internal"),
            ["--add-host", "host.docker.internal:host-gateway"]
        );
        assert_eq!(
            host_args("docker", "host.docker.internal"),
            ["--add-host", "host.docker.internal:host-gateway"]
        );
    }

    #[test]
    fn volume_options_skip_selinux_label_only_for_apple_container() {
        assert_eq!(volume_options("podman"), ":z");
        assert_eq!(volume_options("docker"), ":z");
        assert_eq!(volume_options("container"), "");
    }

    #[test]
    fn apple_container_maps_the_host_with_no_flags() {
        assert!(host_args("container", "host.docker.internal").is_empty());
    }

    #[test]
    fn reads_a_label_from_podman_style_inspection() {
        let json = r#"[{"Config": {"Labels": {"pide.containerfile-hash": "abc123"}}}]"#;
        let mut found = None;
        collect_label(
            &serde_json::from_str::<serde_json::Value>(json).unwrap(),
            "pide.containerfile-hash",
            &mut found,
        );
        assert_eq!(found.as_deref(), Some("abc123"));
    }

    #[test]
    fn reads_a_label_from_apple_style_nested_inspection() {
        // Trimmed from real `container image inspect` output (container 0.12.3): the labels sit
        // under `variants[].config.config.Labels`.
        let json = r#"[
            {
              "index": { "mediaType": "application/vnd.oci.image.index.v1+json" },
              "variants": [
                {
                  "config": {
                    "os": "linux",
                    "architecture": "arm64",
                    "config": {
                      "Cmd": ["/bin/sh"],
                      "Labels": { "pide.containerfile-hash": "deadbeef" }
                    }
                  },
                  "platform": { "os": "linux", "architecture": "arm64" }
                }
              ],
              "name": "pide-fixture:test"
            }
        ]"#;
        let mut found = None;
        collect_label(
            &serde_json::from_str::<serde_json::Value>(json).unwrap(),
            "pide.containerfile-hash",
            &mut found,
        );
        assert_eq!(found.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn missing_label_yields_none() {
        let json = r#"[{"Config": {"Labels": {"other": "x"}}}]"#;
        let mut found = None;
        collect_label(
            &serde_json::from_str::<serde_json::Value>(json).unwrap(),
            "pide.containerfile-hash",
            &mut found,
        );
        assert_eq!(found, None);
    }
}
