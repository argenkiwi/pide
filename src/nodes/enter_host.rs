use crate::Node;
use crate::nodes::{HasError, HasServer};
use crate::utils::{server_url, terminal};
use std::io;

/// Used when the user just presses Enter: Ollama's default address.
pub const DEFAULT_URL: &str = "http://localhost:11434";

/// Where [`EnterHost`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A valid URL was entered (or the default taken) and recorded.
    Entered,
    /// The input was not a valid http(s) URL.
    Invalid,
    /// Input ended without a URL; the error is recorded.
    Cancelled,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read the server URL")]
    Io(#[from] io::Error),
}

pub trait Utils {
    /// Prints `message` and reads a line, or `None` at end of input.
    fn read_line(&self, message: &str) -> io::Result<Option<String>>;
    fn print(&self, message: &str);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn read_line(&self, message: &str) -> io::Result<Option<String>> {
        terminal::prompt(message)
    }

    fn print(&self, message: &str) {
        println!("{message}");
    }
}

/// Asks for the URL of the LLM server.
#[derive(Clone, Debug, Default)]
pub struct EnterHost<U = SystemUtils> {
    utils: U,
}

impl EnterHost {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> EnterHost<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for EnterHost<U>
where
    S: HasServer + HasError + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Error;

    async fn run(&self, state: &mut S) -> Result<Outcome, Error> {
        self.utils.print(
            "Enter the URL of your LLM server, e.g. http://localhost:11434 (Ollama) or http://localhost:1234 (LM Studio).",
        );
        let Some(input) = self
            .utils
            .read_line(&format!("Server URL [{DEFAULT_URL}]: "))?
        else {
            *state.error_mut() = Some("No server URL was entered.".to_string());
            return Ok(Outcome::Cancelled);
        };
        let input = match input.trim() {
            "" => DEFAULT_URL,
            input => input,
        };
        Ok(match server_url::normalise(input) {
            Some(url) => {
                *state.host_url_mut() = url;
                Outcome::Entered
            }
            None => {
                self.utils
                    .print(&format!("Error: \"{input}\" is not a valid http(s) URL."));
                Outcome::Invalid
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
        error: Option<String>,
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

    impl HasError for MockState {
        fn error(&self) -> Option<&str> {
            self.error.as_deref()
        }
        fn error_mut(&mut self) -> &mut Option<String> {
            &mut self.error
        }
    }

    struct MockUtils {
        input: io::Result<Option<&'static str>>,
        printed: Mutex<Vec<String>>,
    }

    impl MockUtils {
        fn new(input: io::Result<Option<&'static str>>) -> Self {
            Self {
                input,
                printed: Mutex::default(),
            }
        }
    }

    impl Utils for &MockUtils {
        fn read_line(&self, _message: &str) -> io::Result<Option<String>> {
            match &self.input {
                Ok(input) => Ok(input.map(str::to_string)),
                Err(e) => Err(io::Error::new(e.kind(), "mock")),
            }
        }
        fn print(&self, message: &str) {
            self.printed.lock().unwrap().push(message.to_string());
        }
    }

    async fn run(utils: &MockUtils) -> (Outcome, MockState) {
        let mut state = MockState::default();
        let outcome = EnterHost::with_utils(utils).run(&mut state).await.unwrap();
        (outcome, state)
    }

    #[tokio::test]
    async fn uses_the_ollama_default_if_input_is_empty() {
        let utils = MockUtils::new(Ok(Some("  ")));

        let (outcome, state) = run(&utils).await;

        assert_eq!(outcome, Outcome::Entered);
        assert_eq!(state.host_url, DEFAULT_URL);
    }

    #[tokio::test]
    async fn records_a_cleaned_up_url() {
        let utils = MockUtils::new(Ok(Some("localhost:1234/v1/")));

        let (outcome, state) = run(&utils).await;

        assert_eq!(outcome, Outcome::Entered);
        assert_eq!(state.host_url, "http://localhost:1234");
    }

    #[tokio::test]
    async fn reports_an_invalid_url() {
        let utils = MockUtils::new(Ok(Some("ftp://nope")));

        let (outcome, state) = run(&utils).await;

        assert_eq!(outcome, Outcome::Invalid);
        assert_eq!(state.host_url, "");
        assert_eq!(
            utils.printed.lock().unwrap().last().map(String::as_str),
            Some("Error: \"ftp://nope\" is not a valid http(s) URL.")
        );
    }

    #[tokio::test]
    async fn is_cancelled_at_end_of_input() {
        let utils = MockUtils::new(Ok(None));

        let (outcome, state) = run(&utils).await;

        assert_eq!(outcome, Outcome::Cancelled);
        assert_eq!(state.error.as_deref(), Some("No server URL was entered."));
    }

    #[tokio::test]
    async fn fails_if_input_cannot_be_read() {
        let utils = MockUtils::new(Err(io::ErrorKind::BrokenPipe.into()));
        let mut state = MockState::default();

        let result = EnterHost::with_utils(&utils).run(&mut state).await;

        assert!(matches!(result, Err(Error::Io(_))));
    }
}
