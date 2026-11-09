use crate::engine::Node;
use crate::nodes::HasTarget;
use crate::utils::{files, layout};
use std::io;
use std::path::{Path, PathBuf};

/// The `/containerfile` slash command: Pi adds the project's dependencies to the Containerfile.
pub const PROMPT: &str = r#"---
description: Add this project's toolchain and dependencies to .pide/Containerfile
---
Update `.pide/Containerfile` so this container has everything needed to build, test and run this project.

1. Work out what the project needs. Look at its manifests and lockfiles (for example `Cargo.toml`, `package.json`, `pyproject.toml`, `requirements.txt`, `go.mod`, `Gemfile`, `pom.xml`, `build.gradle`), its toolchain files (`rust-toolchain.toml`, `.nvmrc`, `.python-version`, `.tool-versions`), and its build scripts, Makefiles and CI configuration.
2. Edit `.pide/Containerfile` to install the toolchains, system packages and command-line tools the project needs. Where the project declares a version, use that version.
3. Put your changes between the `Project dependencies` markers. Keep the base layers that Pi depends on: the Node runtime, the Pi coding agent, the git `safe.directory` setting, `WORKDIR /workspace` and `ENTRYPOINT ["pi"]`.
4. Don't `COPY` project files into the image. The project is mounted at `/workspace` when the container runs, and the build context contains only `.pide/`.
5. Briefly explain what you added and why.

You can't rebuild the image from inside the container. When you're done, tell the user to exit Pi and run pide again: it rebuilds the image because the Containerfile changed.
"#;

/// Where [`EnsurePrompt`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The `/containerfile` prompt exists.
    Ready,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not write {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

pub trait Utils {
    fn exists(&self, path: &Path) -> bool;
    /// Writes `contents` to `path`, creating parent folders.
    fn write(&self, path: &Path, contents: &str) -> io::Result<()>;
    fn print(&self, message: &str);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn exists(&self, path: &Path) -> bool {
        files::exists(path)
    }

    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        files::write(path, contents)
    }

    fn print(&self, message: &str) {
        println!("{message}");
    }
}

/// Installs the `/containerfile` slash command in the project's Pi configuration if missing.
#[derive(Clone, Debug, Default)]
pub struct EnsurePrompt<U = SystemUtils> {
    utils: U,
}

impl EnsurePrompt {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> EnsurePrompt<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for EnsurePrompt<U>
where
    S: HasTarget + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Error;

    async fn run(&self, state: &mut S) -> Result<Outcome, Error> {
        let path = layout::containerfile_prompt(state.target());
        if !self.utils.exists(&path) {
            self.utils
                .write(&path, PROMPT)
                .map_err(|source| Error::Write {
                    path: path.clone(),
                    source,
                })?;
            self.utils.print("Added the /containerfile command to Pi.");
        }
        Ok(Outcome::Ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockState {
        target: PathBuf,
    }

    impl HasTarget for MockState {
        fn target(&self) -> &Path {
            &self.target
        }
        fn target_mut(&mut self) -> &mut PathBuf {
            &mut self.target
        }
    }

    #[derive(Default)]
    struct MockUtils {
        exists: bool,
        fail_writes: bool,
        written: Mutex<Vec<(PathBuf, String)>>,
        printed: Mutex<Vec<String>>,
    }

    impl Utils for &MockUtils {
        fn exists(&self, _path: &Path) -> bool {
            self.exists
        }
        fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
            if self.fail_writes {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            self.written
                .lock()
                .unwrap()
                .push((path.to_path_buf(), contents.to_string()));
            Ok(())
        }
        fn print(&self, message: &str) {
            self.printed.lock().unwrap().push(message.to_string());
        }
    }

    fn state() -> MockState {
        MockState {
            target: "/p".into(),
        }
    }

    #[tokio::test]
    async fn writes_the_prompt_when_missing() {
        let utils = MockUtils::default();

        let outcome = EnsurePrompt::with_utils(&utils)
            .run(&mut state())
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Ready);
        assert_eq!(
            *utils.written.lock().unwrap(),
            [(
                PathBuf::from("/p/.pide/pi/prompts/containerfile.md"),
                PROMPT.to_string()
            )]
        );
        assert_eq!(
            *utils.printed.lock().unwrap(),
            ["Added the /containerfile command to Pi."]
        );
    }

    #[tokio::test]
    async fn keeps_an_existing_prompt() {
        let utils = MockUtils {
            exists: true,
            ..MockUtils::default()
        };

        let outcome = EnsurePrompt::with_utils(&utils)
            .run(&mut state())
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Ready);
        assert!(utils.written.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn fails_if_the_prompt_cannot_be_written() {
        let utils = MockUtils {
            fail_writes: true,
            ..MockUtils::default()
        };

        let result = EnsurePrompt::with_utils(&utils).run(&mut state()).await;

        assert!(matches!(result, Err(Error::Write { .. })));
    }
}
