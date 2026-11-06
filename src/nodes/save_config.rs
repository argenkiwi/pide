use crate::Node;
use crate::nodes::{HasModel, HasServer, HasTarget};
use crate::utils::{files, layout, pi_config, server_url};
use std::io;
use std::path::{Path, PathBuf};

/// Where [`SaveConfig`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Pi's configuration was written.
    Saved,
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
    /// Writes `contents` to `path`, creating parent folders.
    fn write(&self, path: &Path, contents: &str) -> io::Result<()>;
    fn print(&self, message: &str);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        files::write(path, contents)
    }

    fn print(&self, message: &str) {
        println!("{message}");
    }
}

/// Writes Pi's `models.json` and `settings.json` for the chosen server and model.
#[derive(Clone, Debug, Default)]
pub struct SaveConfig<U = SystemUtils> {
    utils: U,
}

impl SaveConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> SaveConfig<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for SaveConfig<U>
where
    S: HasTarget + HasServer + HasModel + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Error;

    async fn run(&self, state: &mut S) -> Result<Outcome, Error> {
        let container_url = server_url::for_container(state.host_url());
        let target = state.target();
        self.write(
            &layout::models_json(target),
            &pi_config::render_models(&container_url, state.model()),
        )?;
        self.write(
            &layout::settings_json(target),
            &pi_config::render_settings(state.model()),
        )?;
        self.utils.print(&format!(
            "Saved Pi's configuration to {}.",
            layout::pi_dir(target).display()
        ));
        *state.container_url_mut() = container_url;
        Ok(Outcome::Saved)
    }
}

impl<U: Utils> SaveConfig<U> {
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
        host_url: String,
        container_url: String,
        model: String,
    }

    impl HasTarget for MockState {
        fn target(&self) -> &Path {
            &self.target
        }
        fn target_mut(&mut self) -> &mut PathBuf {
            &mut self.target
        }
    }

    impl HasServer for MockState {
        fn host_url(&self) -> &str {
            &self.host_url
        }
        fn host_url_mut(&mut self) -> &mut String {
            &mut self.host_url
        }
        fn container_url(&self) -> &str {
            &self.container_url
        }
        fn container_url_mut(&mut self) -> &mut String {
            &mut self.container_url
        }
    }

    impl HasModel for MockState {
        fn model(&self) -> &str {
            &self.model
        }
        fn model_mut(&mut self) -> &mut String {
            &mut self.model
        }
    }

    #[derive(Default)]
    struct MockUtils {
        fail_writes: bool,
        written: Mutex<Vec<(PathBuf, String)>>,
        printed: Mutex<Vec<String>>,
    }

    impl Utils for &MockUtils {
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
            host_url: "http://localhost:11434".into(),
            container_url: String::new(),
            model: "qwen3:8b".into(),
        }
    }

    #[tokio::test]
    async fn writes_the_config_with_a_container_reachable_url() {
        let utils = MockUtils::default();
        let mut state = state();

        let outcome = SaveConfig::with_utils(&utils)
            .run(&mut state)
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Saved);
        assert_eq!(state.container_url, "http://host.docker.internal:11434");
        assert_eq!(
            *utils.written.lock().unwrap(),
            [
                (
                    PathBuf::from("/p/.pide/pi/models.json"),
                    pi_config::render_models("http://host.docker.internal:11434", "qwen3:8b")
                ),
                (
                    PathBuf::from("/p/.pide/pi/settings.json"),
                    pi_config::render_settings("qwen3:8b")
                ),
            ]
        );
        assert_eq!(
            *utils.printed.lock().unwrap(),
            ["Saved Pi's configuration to /p/.pide/pi."]
        );
    }

    #[tokio::test]
    async fn fails_if_the_config_cannot_be_written() {
        let utils = MockUtils {
            fail_writes: true,
            ..MockUtils::default()
        };

        let result = SaveConfig::with_utils(&utils).run(&mut state()).await;

        assert!(
            matches!(result, Err(Error::Write { path, .. }) if path == Path::new("/p/.pide/pi/models.json"))
        );
    }
}
