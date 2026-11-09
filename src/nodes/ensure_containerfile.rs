use crate::engine::Node;
use crate::nodes::HasTarget;
use crate::utils::{files, layout};
use std::io;
use std::path::{Path, PathBuf};

/// The Containerfile pide starts every project with: just enough to run Pi.
pub const BASE_CONTAINERFILE: &str = r#"# Development container for the Pi coding agent, created by pide.
# Run /containerfile in Pi to add this project's toolchain, then run pide again to rebuild.
FROM docker.io/library/node:24-bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
     bash ca-certificates curl fd-find git ripgrep unzip \
  && ln -s /usr/bin/fdfind /usr/local/bin/fd \
  && rm -rf /var/lib/apt/lists/*

ARG PI_VERSION=1.5.0
RUN npm install -g --ignore-scripts @earendil-works/pi-coding-agent@${PI_VERSION}

# The mounted project is owned by the host user, not root.
RUN git config --system --add safe.directory /workspace

# --- Project dependencies (managed by /containerfile) ---

# --- End of project dependencies ---

WORKDIR /workspace
ENTRYPOINT ["pi"]
"#;

/// Keeps Pi's session history out of version control.
pub const GITIGNORE: &str = "# Pi session history\npi/sessions/\n";

/// Where [`EnsureContainerfile`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The Containerfile and `.gitignore` exist.
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

/// Writes the base `.pide/Containerfile` and `.pide/.gitignore` if they are missing.
#[derive(Clone, Debug, Default)]
pub struct EnsureContainerfile<U = SystemUtils> {
    utils: U,
}

impl EnsureContainerfile {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> EnsureContainerfile<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for EnsureContainerfile<U>
where
    S: HasTarget + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Error;

    async fn run(&self, state: &mut S) -> Result<Outcome, Error> {
        let containerfile = layout::containerfile(state.target());
        if !self.utils.exists(&containerfile) {
            self.write(&containerfile, BASE_CONTAINERFILE)?;
            self.utils.print(&format!(
                "Created {} with the base image for Pi.",
                containerfile.display()
            ));
        }
        let gitignore = layout::gitignore(state.target());
        if !self.utils.exists(&gitignore) {
            self.write(&gitignore, GITIGNORE)?;
        }
        Ok(Outcome::Ready)
    }
}

impl<U: Utils> EnsureContainerfile<U> {
    fn write(&self, path: &Path, contents: &str) -> Result<(), Error> {
        self.utils
            .write(path, contents)
            .map_err(|source| Error::Write {
                path: path.to_path_buf(),
                source,
            })
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
        existing: Vec<PathBuf>,
        fail_writes: bool,
        written: Mutex<Vec<(PathBuf, String)>>,
        printed: Mutex<Vec<String>>,
    }

    impl Utils for &MockUtils {
        fn exists(&self, path: &Path) -> bool {
            self.existing.iter().any(|p| p == path)
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
    async fn writes_the_base_files_when_missing() {
        let utils = MockUtils::default();

        let outcome = EnsureContainerfile::with_utils(&utils)
            .run(&mut state())
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Ready);
        assert_eq!(
            *utils.written.lock().unwrap(),
            [
                (
                    PathBuf::from("/p/.pide/Containerfile"),
                    BASE_CONTAINERFILE.to_string()
                ),
                (PathBuf::from("/p/.pide/.gitignore"), GITIGNORE.to_string()),
            ]
        );
        assert_eq!(
            *utils.printed.lock().unwrap(),
            ["Created /p/.pide/Containerfile with the base image for Pi."]
        );
    }

    #[tokio::test]
    async fn leaves_existing_files_untouched() {
        let utils = MockUtils {
            existing: vec![
                "/p/.pide/Containerfile".into(),
                "/p/.pide/.gitignore".into(),
            ],
            ..MockUtils::default()
        };

        let outcome = EnsureContainerfile::with_utils(&utils)
            .run(&mut state())
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Ready);
        assert!(utils.written.lock().unwrap().is_empty());
        assert!(utils.printed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn fails_if_the_containerfile_cannot_be_written() {
        let utils = MockUtils {
            fail_writes: true,
            ..MockUtils::default()
        };

        let result = EnsureContainerfile::with_utils(&utils)
            .run(&mut state())
            .await;

        assert!(
            matches!(result, Err(Error::Write { path, .. }) if path == Path::new("/p/.pide/Containerfile"))
        );
    }
}
