use crate::Node;
use crate::nodes::{HasModel, HasServer, HasTarget};
use crate::utils::{files, layout, pi_config};
use std::convert::Infallible;
use std::io;
use std::path::Path;

/// Where [`CheckConfig`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Pi already has a server and model; both are recorded.
    Configured,
    /// The configuration is missing, unreadable or incomplete.
    NeedsSetup,
}

pub trait Utils {
    fn read(&self, path: &Path) -> io::Result<String>;
    fn print(&self, message: &str);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn read(&self, path: &Path) -> io::Result<String> {
        files::read(path)
    }

    fn print(&self, message: &str) {
        println!("{message}");
    }
}

/// Reads the project's Pi configuration to find out whether setup is needed.
#[derive(Clone, Debug, Default)]
pub struct CheckConfig<U = SystemUtils> {
    utils: U,
}

impl CheckConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> CheckConfig<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for CheckConfig<U>
where
    S: HasTarget + HasServer + HasModel + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Infallible;

    async fn run(&self, state: &mut S) -> Result<Outcome, Infallible> {
        let models = self.utils.read(&layout::models_json(state.target()));
        let settings = self.utils.read(&layout::settings_json(state.target()));
        let config = match (models, settings) {
            (Ok(models), Ok(settings)) => pi_config::parse(&models, &settings),
            _ => None,
        };
        let Some(config) = config else {
            self.utils
                .print("Pi isn't configured for this project yet. Let's set it up.");
            return Ok(Outcome::NeedsSetup);
        };
        self.utils.print(&format!(
            "Using model {} from {}.",
            config.model, config.base_url
        ));
        // Only the container's view of the URL is saved; use it for both.
        state.host_url_mut().clone_from(&config.base_url);
        *state.container_url_mut() = config.base_url;
        *state.model_mut() = config.model;
        Ok(Outcome::Configured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
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
        files: HashMap<PathBuf, String>,
        printed: Mutex<Vec<String>>,
    }

    impl Utils for &MockUtils {
        fn read(&self, path: &Path) -> io::Result<String> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| io::ErrorKind::NotFound.into())
        }
        fn print(&self, message: &str) {
            self.printed.lock().unwrap().push(message.to_string());
        }
    }

    fn state() -> MockState {
        MockState {
            target: "/p".into(),
            host_url: String::new(),
            container_url: String::new(),
            model: String::new(),
        }
    }

    #[tokio::test]
    async fn records_the_server_and_model_when_configured() {
        let utils = MockUtils {
            files: HashMap::from([
                (
                    PathBuf::from("/p/.pico/pi/models.json"),
                    pi_config::render_models("http://host.docker.internal:11434", "qwen3:8b"),
                ),
                (
                    PathBuf::from("/p/.pico/pi/settings.json"),
                    pi_config::render_settings("qwen3:8b"),
                ),
            ]),
            ..MockUtils::default()
        };
        let mut state = state();

        let Ok(outcome) = CheckConfig::with_utils(&utils).run(&mut state).await;

        assert_eq!(outcome, Outcome::Configured);
        assert_eq!(state.model, "qwen3:8b");
        assert_eq!(state.host_url, "http://host.docker.internal:11434");
        assert_eq!(state.container_url, "http://host.docker.internal:11434");
        assert_eq!(
            *utils.printed.lock().unwrap(),
            ["Using model qwen3:8b from http://host.docker.internal:11434."]
        );
    }

    #[tokio::test]
    async fn needs_setup_when_the_files_are_missing() {
        let utils = MockUtils::default();
        let mut state = state();

        let Ok(outcome) = CheckConfig::with_utils(&utils).run(&mut state).await;

        assert_eq!(outcome, Outcome::NeedsSetup);
        assert_eq!(state.model, "");
        assert_eq!(
            *utils.printed.lock().unwrap(),
            ["Pi isn't configured for this project yet. Let's set it up."]
        );
    }

    #[tokio::test]
    async fn needs_setup_when_the_files_are_incomplete() {
        let utils = MockUtils {
            files: HashMap::from([
                (PathBuf::from("/p/.pico/pi/models.json"), "{}".to_string()),
                (
                    PathBuf::from("/p/.pico/pi/settings.json"),
                    pi_config::render_settings("qwen3:8b"),
                ),
            ]),
            ..MockUtils::default()
        };
        let mut state = state();

        let Ok(outcome) = CheckConfig::with_utils(&utils).run(&mut state).await;

        assert_eq!(outcome, Outcome::NeedsSetup);
        assert_eq!(state.model, "");
    }
}
