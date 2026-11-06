use crate::Node;
use crate::nodes::{HasModel, HasTarget};
use std::convert::Infallible;

pub trait Utils {
    fn print(&self, message: &str);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn print(&self, message: &str) {
        println!("{message}");
    }
}

/// Sums up the session after Pi exits. Terminal: it has no outgoing edges.
#[derive(Clone, Debug, Default)]
pub struct Finish<U = SystemUtils> {
    utils: U,
}

impl Finish {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> Finish<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for Finish<U>
where
    S: HasTarget + HasModel + Send,
    U: Utils + Sync,
{
    type Outcome = ();
    type Error = Infallible;

    async fn run(&self, state: &mut S) -> Result<(), Infallible> {
        self.utils.print(&format!(
            "Pi has exited (project {}, model {}).",
            state.target().display(),
            state.model()
        ));
        self.utils
            .print("Changes made with /containerfile take effect the next time you run pide.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    struct MockState {
        target: PathBuf,
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
        printed: Mutex<Vec<String>>,
    }

    impl Utils for &MockUtils {
        fn print(&self, message: &str) {
            self.printed.lock().unwrap().push(message.to_string());
        }
    }

    #[tokio::test]
    async fn summarises_the_session() {
        let utils = MockUtils::default();
        let mut state = MockState {
            target: "/p/app".into(),
            model: "qwen3:8b".into(),
        };

        let Ok(()) = Finish::with_utils(&utils).run(&mut state).await;

        assert_eq!(
            *utils.printed.lock().unwrap(),
            [
                "Pi has exited (project /p/app, model qwen3:8b).",
                "Changes made with /containerfile take effect the next time you run pide.",
            ]
        );
    }
}
