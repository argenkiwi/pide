use crate::engine::Node;
use crate::nodes::{HasError, HasRuntime};
use crate::utils::container;
use std::convert::Infallible;
use std::future::Future;

/// Where [`DetectRuntime`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// An engine is installed and running; the runtime is recorded.
    Ready,
    /// No engine is installed, or the preferred one is not running; the error is recorded.
    Unavailable,
}

pub trait Utils {
    /// Returns `true` if the `engine` command is installed.
    fn is_installed(&self, engine: &str) -> bool;
    /// Returns `true` if `engine` is running and answering.
    fn is_running(&self, engine: &str) -> impl Future<Output = bool> + Send;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn is_installed(&self, engine: &str) -> bool {
        container::is_installed(engine)
    }

    fn is_running(&self, engine: &str) -> impl Future<Output = bool> + Send {
        container::is_running(engine)
    }
}

/// Picks the container engine: podman if installed, otherwise docker, and checks it is running.
#[derive(Clone, Debug, Default)]
pub struct DetectRuntime<U = SystemUtils> {
    utils: U,
}

impl DetectRuntime {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> DetectRuntime<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for DetectRuntime<U>
where
    S: HasRuntime + HasError + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Infallible;

    async fn run(&self, state: &mut S) -> Result<Outcome, Infallible> {
        let Some(&engine) = container::ENGINES
            .iter()
            .find(|engine| self.utils.is_installed(engine))
        else {
            *state.error_mut() = Some(
                "Neither podman nor docker is installed. Install Podman Desktop or Docker Desktop and try again."
                    .to_string(),
            );
            return Ok(Outcome::Unavailable);
        };
        if !self.utils.is_running(engine).await {
            let hint = if engine == "podman" {
                "start it with `podman machine start`"
            } else {
                "start Docker Desktop (or the Docker daemon)"
            };
            *state.error_mut() = Some(format!(
                "{engine} is installed but not running: {hint} and try again."
            ));
            return Ok(Outcome::Unavailable);
        }
        *state.runtime_mut() = engine.to_string();
        Ok(Outcome::Ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockState {
        runtime: String,
        error: Option<String>,
    }

    impl HasRuntime for MockState {
        fn runtime(&self) -> &str {
            &self.runtime
        }
        fn runtime_mut(&mut self) -> &mut String {
            &mut self.runtime
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
        installed: &'static [&'static str],
        running: &'static [&'static str],
    }

    impl Utils for MockUtils {
        fn is_installed(&self, engine: &str) -> bool {
            self.installed.contains(&engine)
        }
        async fn is_running(&self, engine: &str) -> bool {
            self.running.contains(&engine)
        }
    }

    async fn run(utils: MockUtils) -> (Outcome, MockState) {
        let mut state = MockState::default();
        let Ok(outcome) = DetectRuntime::with_utils(utils).run(&mut state).await;
        (outcome, state)
    }

    #[tokio::test]
    async fn prefers_podman_when_both_are_available() {
        let (outcome, state) = run(MockUtils {
            installed: &["podman", "docker"],
            running: &["podman", "docker"],
        })
        .await;

        assert_eq!(outcome, Outcome::Ready);
        assert_eq!(state.runtime, "podman");
    }

    #[tokio::test]
    async fn falls_back_to_docker() {
        let (outcome, state) = run(MockUtils {
            installed: &["docker"],
            running: &["docker"],
        })
        .await;

        assert_eq!(outcome, Outcome::Ready);
        assert_eq!(state.runtime, "docker");
    }

    #[tokio::test]
    async fn is_unavailable_if_no_engine_is_installed() {
        let (outcome, state) = run(MockUtils {
            installed: &[],
            running: &[],
        })
        .await;

        assert_eq!(outcome, Outcome::Unavailable);
        assert_eq!(state.runtime, "");
        assert!(state.error.unwrap().contains("Neither podman nor docker"));
    }

    #[tokio::test]
    async fn is_unavailable_with_a_hint_if_podman_is_not_running() {
        let (outcome, state) = run(MockUtils {
            installed: &["podman", "docker"],
            running: &["docker"],
        })
        .await;

        assert_eq!(outcome, Outcome::Unavailable);
        assert!(state.error.unwrap().contains("podman machine start"));
    }

    #[tokio::test]
    async fn is_unavailable_with_a_hint_if_docker_is_not_running() {
        let (outcome, state) = run(MockUtils {
            installed: &["docker"],
            running: &[],
        })
        .await;

        assert_eq!(outcome, Outcome::Unavailable);
        assert!(state.error.unwrap().contains("Docker Desktop"));
    }
}
