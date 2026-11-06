//! Where pico keeps its files inside a target project, and how it names images.

use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// Folder, inside the target project, that holds everything pico creates.
pub const PICO_DIR: &str = ".pico";
/// Prefix of every image pico builds.
pub const IMAGE_PREFIX: &str = "pico";
/// Image label holding the hash of the Containerfile the image was built from.
pub const HASH_LABEL: &str = "pico.containerfile-hash";
/// Where the target project is mounted inside the container.
pub const WORKSPACE: &str = "/workspace";
/// Pi's agent configuration folder inside the container.
pub const PI_AGENT_DIR: &str = "/root/.pi/agent";

/// `<target>/.pico`.
#[must_use]
pub fn pico_dir(target: &Path) -> PathBuf {
    target.join(PICO_DIR)
}

/// `<target>/.pico/Containerfile`.
#[must_use]
pub fn containerfile(target: &Path) -> PathBuf {
    pico_dir(target).join("Containerfile")
}

/// `<target>/.pico/.gitignore`.
#[must_use]
pub fn gitignore(target: &Path) -> PathBuf {
    pico_dir(target).join(".gitignore")
}

/// `<target>/.pico/pi`, mounted as Pi's agent configuration folder.
#[must_use]
pub fn pi_dir(target: &Path) -> PathBuf {
    pico_dir(target).join("pi")
}

/// `<target>/.pico/pi/models.json`.
#[must_use]
pub fn models_json(target: &Path) -> PathBuf {
    pi_dir(target).join("models.json")
}

/// `<target>/.pico/pi/settings.json`.
#[must_use]
pub fn settings_json(target: &Path) -> PathBuf {
    pi_dir(target).join("settings.json")
}

/// `<target>/.pico/pi/prompts/containerfile.md`, the `/containerfile` slash command.
#[must_use]
pub fn containerfile_prompt(target: &Path) -> PathBuf {
    pi_dir(target).join("prompts").join("containerfile.md")
}

/// Names the image for `target`: `pico-<folder>-<path hash>`.
///
/// The folder name is lowercased and reduced to `[a-z0-9-]`; the hash of the full path keeps
/// projects with the same folder name apart.
#[must_use]
pub fn image_name(target: &Path) -> String {
    let folder = target
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let folder: String = folder
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let folder = folder.trim_matches('-');
    let hash = &content_hash(&target.to_string_lossy())[..8];
    if folder.is_empty() {
        format!("{IMAGE_PREFIX}-{hash}")
    } else {
        format!("{IMAGE_PREFIX}-{folder}-{hash}")
    }
}

/// A short, stable hash of `contents`: the first 16 hex digits of its SHA-256.
#[must_use]
pub fn content_hash(contents: &str) -> String {
    let digest = Sha256::digest(contents.as_bytes());
    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_name_uses_a_clean_folder_name_and_a_path_hash() {
        let name = image_name(Path::new("/home/me/My Project_2"));

        assert!(name.starts_with("pico-my-project-2-"), "{name}");
        assert_eq!(name.len(), "pico-my-project-2-".len() + 8);
    }

    #[test]
    fn image_name_differs_for_same_folder_in_different_places() {
        assert_ne!(
            image_name(Path::new("/a/app")),
            image_name(Path::new("/b/app"))
        );
    }

    #[test]
    fn image_name_falls_back_to_the_hash_alone() {
        let name = image_name(Path::new("/"));

        assert!(
            name.starts_with("pico-") && name.len() == "pico-".len() + 8,
            "{name}"
        );
    }

    #[test]
    fn content_hash_is_stable_and_short() {
        assert_eq!(content_hash("abc"), "ba7816bf8f01cfea");
    }
}
