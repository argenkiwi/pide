use crate::nodes::{HasError, HasModel, HasModels};
use crate::utils::terminal;
use ambler::Node;
use std::io;

/// Where [`SelectModel`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A model was picked and recorded.
    Selected,
    /// The input was not a number in range.
    Invalid,
    /// Input ended without a choice; the error is recorded.
    Cancelled,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read the model choice")]
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

/// Lists the available models and asks the user to pick one by number.
#[derive(Clone, Debug, Default)]
pub struct SelectModel<U = SystemUtils> {
    utils: U,
}

impl SelectModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> SelectModel<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for SelectModel<U>
where
    S: HasModels + HasModel + HasError + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Error;

    async fn run(&self, state: &mut S) -> Result<Outcome, Error> {
        let count = state.models().len();
        self.utils.print("Available models:");
        for (i, model) in state.models().iter().enumerate() {
            self.utils.print(&format!("  {}. {model}", i + 1));
        }
        let Some(input) = self
            .utils
            .read_line(&format!("Select a model [1-{count}]: "))?
        else {
            *state.error_mut() = Some("No model was selected.".to_string());
            return Ok(Outcome::Cancelled);
        };
        match input.trim().parse::<usize>() {
            Ok(choice) if (1..=count).contains(&choice) => {
                *state.model_mut() = state.models()[choice - 1].clone();
                Ok(Outcome::Selected)
            }
            _ => {
                self.utils
                    .print(&format!("Error: enter a number from 1 to {count}."));
                Ok(Outcome::Invalid)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockState {
        models: Vec<String>,
        model: String,
        error: Option<String>,
    }

    impl HasModels for MockState {
        fn models(&self) -> &[String] {
            &self.models
        }
        fn models_mut(&mut self) -> &mut Vec<String> {
            &mut self.models
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
        prompt: Mutex<Option<String>>,
        printed: Mutex<Vec<String>>,
    }

    impl MockUtils {
        fn new(input: io::Result<Option<&'static str>>) -> Self {
            Self {
                input,
                prompt: Mutex::default(),
                printed: Mutex::default(),
            }
        }
    }

    impl Utils for &MockUtils {
        fn read_line(&self, message: &str) -> io::Result<Option<String>> {
            *self.prompt.lock().unwrap() = Some(message.to_string());
            match &self.input {
                Ok(input) => Ok(input.map(str::to_string)),
                Err(e) => Err(io::Error::new(e.kind(), "mock")),
            }
        }
        fn print(&self, message: &str) {
            self.printed.lock().unwrap().push(message.to_string());
        }
    }

    fn state() -> MockState {
        MockState {
            models: vec!["llama3".into(), "qwen3:8b".into()],
            ..MockState::default()
        }
    }

    #[tokio::test]
    async fn lists_the_models_and_records_the_choice() {
        let utils = MockUtils::new(Ok(Some(" 2 ")));
        let mut state = state();

        let outcome = SelectModel::with_utils(&utils)
            .run(&mut state)
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Selected);
        assert_eq!(state.model, "qwen3:8b");
        assert_eq!(
            *utils.printed.lock().unwrap(),
            ["Available models:", "  1. llama3", "  2. qwen3:8b"]
        );
        assert_eq!(
            utils.prompt.lock().unwrap().as_deref(),
            Some("Select a model [1-2]: ")
        );
    }

    #[tokio::test]
    async fn reports_a_number_out_of_range() {
        let utils = MockUtils::new(Ok(Some("3")));
        let mut state = state();

        let outcome = SelectModel::with_utils(&utils)
            .run(&mut state)
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Invalid);
        assert_eq!(state.model, "");
        assert_eq!(
            utils.printed.lock().unwrap().last().map(String::as_str),
            Some("Error: enter a number from 1 to 2.")
        );
    }

    #[tokio::test]
    async fn reports_input_that_is_not_a_number() {
        let utils = MockUtils::new(Ok(Some("qwen")));
        let mut state = state();

        let outcome = SelectModel::with_utils(&utils)
            .run(&mut state)
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Invalid);
    }

    #[tokio::test]
    async fn is_cancelled_at_end_of_input() {
        let utils = MockUtils::new(Ok(None));
        let mut state = state();

        let outcome = SelectModel::with_utils(&utils)
            .run(&mut state)
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Cancelled);
        assert_eq!(state.error.as_deref(), Some("No model was selected."));
    }

    #[tokio::test]
    async fn fails_if_input_cannot_be_read() {
        let utils = MockUtils::new(Err(io::ErrorKind::BrokenPipe.into()));

        let result = SelectModel::with_utils(&utils).run(&mut state()).await;

        assert!(matches!(result, Err(Error::Io(_))));
    }
}
