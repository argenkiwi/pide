use crate::Node;
use crate::nodes::HasError;
use std::convert::Infallible;

/// Shown if the walk reaches [`Fail`] without an error recorded.
pub const UNKNOWN_ERROR: &str = "pide stopped for an unknown reason.";

pub trait Utils {
    /// Prints `message` to standard error.
    fn print_error(&self, message: &str);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn print_error(&self, message: &str) {
        eprintln!("{message}");
    }
}

/// Reports the recorded error. Terminal: it has no outgoing edges.
#[derive(Clone, Debug, Default)]
pub struct Fail<U = SystemUtils> {
    utils: U,
}

impl Fail {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> Fail<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for Fail<U>
where
    S: HasError + Send,
    U: Utils + Sync,
{
    type Outcome = ();
    type Error = Infallible;

    async fn run(&self, state: &mut S) -> Result<(), Infallible> {
        self.utils.print_error(&format!(
            "Error: {}",
            state.error().unwrap_or(UNKNOWN_ERROR)
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockState {
        error: Option<String>,
    }

    impl HasError for MockState {
        fn error(&self) -> Option<&str> {
            self.error.as_deref()
        }
        fn error_mut(&mut self) -> &mut Option<String> {
            &mut self.error
        }
    }

    #[derive(Default)]
    struct MockUtils {
        printed: Mutex<Vec<String>>,
    }

    impl Utils for &MockUtils {
        fn print_error(&self, message: &str) {
            self.printed.lock().unwrap().push(message.to_string());
        }
    }

    #[tokio::test]
    async fn prints_the_recorded_error() {
        let utils = MockUtils::default();
        let mut state = MockState {
            error: Some("Docker isn't running.".into()),
        };

        let Ok(()) = Fail::with_utils(&utils).run(&mut state).await;

        assert_eq!(
            *utils.printed.lock().unwrap(),
            ["Error: Docker isn't running."]
        );
    }

    #[tokio::test]
    async fn prints_a_fallback_without_a_recorded_error() {
        let utils = MockUtils::default();
        let mut state = MockState { error: None };

        let Ok(()) = Fail::with_utils(&utils).run(&mut state).await;

        assert_eq!(
            *utils.printed.lock().unwrap(),
            [format!("Error: {UNKNOWN_ERROR}")]
        );
    }
}
