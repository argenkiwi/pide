use crate::engine::Node;
use crate::nodes::{HasModels, HasServer};
use crate::utils::llm;
use std::convert::Infallible;
use std::error::Error as _;
use std::future::Future;

/// Where [`FetchModels`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The server offers models; they are recorded.
    Found,
    /// The server could not be reached or answered with an error.
    Unreachable,
    /// The server answered but offers no models.
    Empty,
}

pub trait Utils {
    /// Lists the models served at `base_url`, or describes why that failed.
    fn list_models(
        &self,
        base_url: &str,
    ) -> impl Future<Output = Result<Vec<String>, String>> + Send;
    fn print(&self, message: &str);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    async fn list_models(&self, base_url: &str) -> Result<Vec<String>, String> {
        llm::list_models(base_url).await.map_err(|e| {
            // reqwest's own message is generic; the cause says what actually went wrong.
            let mut message = e.to_string();
            let mut source = e.source();
            while let Some(cause) = source {
                message = format!("{message}: {cause}");
                source = cause.source();
            }
            message
        })
    }

    fn print(&self, message: &str) {
        println!("{message}");
    }
}

/// Asks the LLM server which models it offers.
#[derive(Clone, Debug, Default)]
pub struct FetchModels<U = SystemUtils> {
    utils: U,
}

impl FetchModels {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> FetchModels<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for FetchModels<U>
where
    S: HasServer + HasModels + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Infallible;

    async fn run(&self, state: &mut S) -> Result<Outcome, Infallible> {
        let url = state.host_url().to_string();
        self.utils.print(&format!("Fetching models from {url}..."));
        Ok(match self.utils.list_models(&url).await {
            Err(e) => {
                self.utils.print(&format!(
                    "Error: could not get the model list from {url}: {e}"
                ));
                Outcome::Unreachable
            }
            Ok(models) if models.is_empty() => {
                self.utils.print(
                    "The server has no models. Pull one (e.g. `ollama pull qwen3`) or load one in LM Studio, then try again.",
                );
                Outcome::Empty
            }
            Ok(models) => {
                *state.models_mut() = models;
                Outcome::Found
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockState {
        host_url: String,
        container_url: String,
        models: Vec<String>,
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

    impl HasModels for MockState {
        fn models(&self) -> &[String] {
            &self.models
        }
        fn models_mut(&mut self) -> &mut Vec<String> {
            &mut self.models
        }
    }

    struct MockUtils {
        response: Result<Vec<String>, String>,
        asked: Mutex<Option<String>>,
        printed: Mutex<Vec<String>>,
    }

    impl MockUtils {
        fn new(response: Result<&[&str], &str>) -> Self {
            Self {
                response: response
                    .map(|ids| ids.iter().map(ToString::to_string).collect())
                    .map_err(str::to_string),
                asked: Mutex::default(),
                printed: Mutex::default(),
            }
        }
    }

    impl Utils for &MockUtils {
        async fn list_models(&self, base_url: &str) -> Result<Vec<String>, String> {
            *self.asked.lock().unwrap() = Some(base_url.to_string());
            self.response.clone()
        }
        fn print(&self, message: &str) {
            self.printed.lock().unwrap().push(message.to_string());
        }
    }

    async fn run(utils: &MockUtils) -> (Outcome, MockState) {
        let mut state = MockState {
            host_url: "http://localhost:11434".into(),
            ..MockState::default()
        };
        let Ok(outcome) = FetchModels::with_utils(utils).run(&mut state).await;
        (outcome, state)
    }

    #[tokio::test]
    async fn records_the_models_the_server_offers() {
        let utils = MockUtils::new(Ok(&["llama3", "qwen3:8b"]));

        let (outcome, state) = run(&utils).await;

        assert_eq!(outcome, Outcome::Found);
        assert_eq!(state.models, ["llama3", "qwen3:8b"]);
        assert_eq!(
            utils.asked.lock().unwrap().as_deref(),
            Some("http://localhost:11434")
        );
    }

    #[tokio::test]
    async fn reports_an_unreachable_server() {
        let utils = MockUtils::new(Err("connection refused"));

        let (outcome, state) = run(&utils).await;

        assert_eq!(outcome, Outcome::Unreachable);
        assert!(state.models.is_empty());
        assert_eq!(
            utils.printed.lock().unwrap().last().map(String::as_str),
            Some(
                "Error: could not get the model list from http://localhost:11434: connection refused"
            )
        );
    }

    #[tokio::test]
    async fn reports_a_server_without_models() {
        let utils = MockUtils::new(Ok(&[]));

        let (outcome, state) = run(&utils).await;

        assert_eq!(outcome, Outcome::Empty);
        assert!(state.models.is_empty());
    }
}
