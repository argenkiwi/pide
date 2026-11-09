use crate::nodes::{HasError, HasTarget};
use crate::utils::files;
use ambler::Node;
use std::convert::Infallible;
use std::path::{Path, PathBuf};

/// Where [`ResolveTarget`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The target is an existing folder; it now holds its absolute path.
    Found,
    /// The target does not exist or is not a folder; the error is recorded.
    Missing,
}

pub trait Utils {
    /// Returns the absolute path of `path` if it is an existing folder.
    fn resolve_dir(&self, path: &Path) -> Option<PathBuf>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn resolve_dir(&self, path: &Path) -> Option<PathBuf> {
        files::resolve_dir(path)
    }
}

/// Checks that the target folder exists and makes its path absolute.
#[derive(Clone, Debug, Default)]
pub struct ResolveTarget<U = SystemUtils> {
    utils: U,
}

impl ResolveTarget {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> ResolveTarget<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for ResolveTarget<U>
where
    S: HasTarget + HasError + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Infallible;

    async fn run(&self, state: &mut S) -> Result<Outcome, Infallible> {
        Ok(match self.utils.resolve_dir(state.target()) {
            Some(path) => {
                *state.target_mut() = path;
                Outcome::Found
            }
            None => {
                *state.error_mut() = Some(format!(
                    "The target folder {} does not exist or is not a folder.",
                    state.target().display()
                ));
                Outcome::Missing
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockState {
        target: PathBuf,
        error: Option<String>,
    }

    impl HasTarget for MockState {
        fn target(&self) -> &Path {
            &self.target
        }
        fn target_mut(&mut self) -> &mut PathBuf {
            &mut self.target
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
        resolved: Option<&'static str>,
    }

    impl Utils for MockUtils {
        fn resolve_dir(&self, _path: &Path) -> Option<PathBuf> {
            self.resolved.map(PathBuf::from)
        }
    }

    #[tokio::test]
    async fn finds_an_existing_folder_and_makes_it_absolute() {
        let utils = MockUtils {
            resolved: Some("/home/me/project"),
        };
        let mut state = MockState {
            target: ".".into(),
            ..MockState::default()
        };

        let Ok(outcome) = ResolveTarget::with_utils(utils).run(&mut state).await;

        assert_eq!(outcome, Outcome::Found);
        assert_eq!(state.target, Path::new("/home/me/project"));
        assert_eq!(state.error, None);
    }

    #[tokio::test]
    async fn records_an_error_if_the_folder_is_missing() {
        let utils = MockUtils { resolved: None };
        let mut state = MockState {
            target: "nowhere".into(),
            ..MockState::default()
        };

        let Ok(outcome) = ResolveTarget::with_utils(utils).run(&mut state).await;

        assert_eq!(outcome, Outcome::Missing);
        assert_eq!(state.target, Path::new("nowhere"));
        assert_eq!(
            state.error.as_deref(),
            Some("The target folder nowhere does not exist or is not a folder.")
        );
    }
}
