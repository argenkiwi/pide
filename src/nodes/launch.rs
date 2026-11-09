use crate::nodes::{HasError, HasImage, HasRuntime, HasTarget};
use crate::utils::{container, git, layout, server_url};
use ambler::Node;
use std::convert::Infallible;
use std::future::Future;
use std::io::{self, IsTerminal};
use std::ops::RangeInclusive;

/// Exit codes podman and docker use for their own failures, rather than the container's:
/// 125 for an engine error, 126 and 127 when the entry point cannot be run.
const ENGINE_ERROR_CODES: RangeInclusive<i32> = 125..=127;

/// Host git settings passed into the container as environment variables.
const GIT_IDENTITY: &[(&str, &str)] = &[
    ("GIT_AUTHOR_NAME", "user.name"),
    ("GIT_AUTHOR_EMAIL", "user.email"),
    ("GIT_COMMITTER_NAME", "user.name"),
    ("GIT_COMMITTER_EMAIL", "user.email"),
];

/// Environment that turns off Pi's telemetry and update check.
const PI_ENV: &[&str] = &["PI_TELEMETRY=0", "PI_SKIP_VERSION_CHECK=1"];

/// Where [`Launch`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Pi ran and exited.
    Exited,
    /// The container could not be started; the error is recorded.
    Failed,
}

pub trait Utils {
    /// Reads host git config `key`, or `None` if unset.
    fn git_config(&self, key: &str) -> Option<String>;
    /// Returns `true` if both stdin and stdout are a terminal.
    fn is_terminal(&self) -> bool;
    /// Runs `engine` with `args` attached to the terminal; returns its exit code.
    fn run(
        &self,
        engine: &str,
        args: &[String],
    ) -> impl Future<Output = io::Result<Option<i32>>> + Send;
    fn print(&self, message: &str);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn git_config(&self, key: &str) -> Option<String> {
        git::config(key)
    }

    fn is_terminal(&self) -> bool {
        io::stdin().is_terminal() && io::stdout().is_terminal()
    }

    fn run(
        &self,
        engine: &str,
        args: &[String],
    ) -> impl Future<Output = io::Result<Option<i32>>> + Send {
        container::run(engine, args)
    }

    fn print(&self, message: &str) {
        println!("{message}");
    }
}

/// Runs Pi in the project's container, with the project and Pi's configuration mounted.
#[derive(Clone, Debug, Default)]
pub struct Launch<U = SystemUtils> {
    utils: U,
}

impl Launch {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> Launch<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for Launch<U>
where
    S: HasTarget + HasRuntime + HasImage + HasError + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Infallible;

    async fn run(&self, state: &mut S) -> Result<Outcome, Infallible> {
        let engine = state.runtime().to_string();
        let args = self.args(state);
        self.utils
            .print(&format!("Starting Pi in {}...", state.image()));
        Ok(match self.utils.run(&engine, &args).await {
            Err(e) => {
                *state.error_mut() = Some(format!("Could not run {engine}: {e}"));
                Outcome::Failed
            }
            Ok(Some(code)) if ENGINE_ERROR_CODES.contains(&code) => {
                *state.error_mut() = Some(format!(
                    "{engine} could not start the container (exit code {code})."
                ));
                Outcome::Failed
            }
            // Pi's own exit code, including Ctrl-C, still means the session ran.
            Ok(_) => Outcome::Exited,
        })
    }
}

impl<U: Utils> Launch<U> {
    /// The engine's `run` arguments.
    fn args<S: HasTarget + HasImage>(&self, state: &S) -> Vec<String> {
        let target = state.target();
        let mut args: Vec<String> = vec!["run".into(), "--rm".into(), "--interactive".into()];
        if self.utils.is_terminal() {
            args.push("--tty".into());
        }
        args.extend([
            "--volume".into(),
            format!("{}:{}:z", target.display(), layout::WORKSPACE),
            "--workdir".into(),
            layout::WORKSPACE.into(),
            "--volume".into(),
            format!(
                "{}:{}:z",
                layout::pi_dir(target).display(),
                layout::PI_AGENT_DIR
            ),
            "--add-host".into(),
            format!("{}:host-gateway", server_url::CONTAINER_HOST),
        ]);
        for (var, key) in GIT_IDENTITY {
            if let Some(value) = self.utils.git_config(key) {
                args.extend(["--env".into(), format!("{var}={value}")]);
            }
        }
        for env in PI_ENV {
            args.extend(["--env".into(), (*env).to_string()]);
        }
        args.push(state.image().to_string());
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockState {
        target: PathBuf,
        runtime: String,
        image: String,
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

    impl HasRuntime for MockState {
        fn runtime(&self) -> &str {
            &self.runtime
        }
        fn runtime_mut(&mut self) -> &mut String {
            &mut self.runtime
        }
    }

    impl HasImage for MockState {
        fn image(&self) -> &str {
            &self.image
        }
        fn image_mut(&mut self) -> &mut String {
            &mut self.image
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
        git_name: Option<&'static str>,
        terminal: bool,
        exit: io::Result<Option<i32>>,
        ran: Mutex<Option<(String, Vec<String>)>>,
    }

    impl MockUtils {
        fn new(exit: io::Result<Option<i32>>) -> Self {
            Self {
                git_name: Some("Ada"),
                terminal: true,
                exit,
                ran: Mutex::default(),
            }
        }
    }

    impl Utils for &MockUtils {
        fn git_config(&self, key: &str) -> Option<String> {
            (key == "user.name").then_some(self.git_name?.to_string())
        }
        fn is_terminal(&self) -> bool {
            self.terminal
        }
        async fn run(&self, engine: &str, args: &[String]) -> io::Result<Option<i32>> {
            *self.ran.lock().unwrap() = Some((engine.to_string(), args.to_vec()));
            match &self.exit {
                Ok(code) => Ok(*code),
                Err(e) => Err(io::Error::new(e.kind(), "mock")),
            }
        }
        fn print(&self, _message: &str) {}
    }

    fn state() -> MockState {
        MockState {
            target: "/p/app".into(),
            runtime: "docker".into(),
            image: "pide-app-1234".into(),
            ..MockState::default()
        }
    }

    #[tokio::test]
    async fn runs_pi_with_the_project_and_config_mounted() {
        let utils = MockUtils::new(Ok(Some(0)));
        let mut state = state();

        let Ok(outcome) = Launch::with_utils(&utils).run(&mut state).await;

        assert_eq!(outcome, Outcome::Exited);
        let (engine, args) = utils.ran.lock().unwrap().clone().unwrap();
        assert_eq!(engine, "docker");
        assert_eq!(
            args,
            [
                "run",
                "--rm",
                "--interactive",
                "--tty",
                "--volume",
                "/p/app:/workspace:z",
                "--workdir",
                "/workspace",
                "--volume",
                "/p/app/.pide/pi:/root/.pi/agent:z",
                "--add-host",
                "host.docker.internal:host-gateway",
                "--env",
                "GIT_AUTHOR_NAME=Ada",
                "--env",
                "GIT_COMMITTER_NAME=Ada",
                "--env",
                "PI_TELEMETRY=0",
                "--env",
                "PI_SKIP_VERSION_CHECK=1",
                "pide-app-1234",
            ]
        );
    }

    #[tokio::test]
    async fn omits_the_tty_and_unset_git_identity() {
        let utils = MockUtils {
            git_name: None,
            terminal: false,
            ..MockUtils::new(Ok(Some(0)))
        };

        let Ok(_) = Launch::with_utils(&utils).run(&mut state()).await;

        let (_, args) = utils.ran.lock().unwrap().clone().unwrap();
        assert!(!args.iter().any(|a| a == "--tty" || a.starts_with("GIT_")));
    }

    #[tokio::test]
    async fn treats_a_nonzero_exit_from_pi_as_exited() {
        let utils = MockUtils::new(Ok(Some(130)));
        let mut state = state();

        let Ok(outcome) = Launch::with_utils(&utils).run(&mut state).await;

        assert_eq!(outcome, Outcome::Exited);
        assert_eq!(state.error, None);
    }

    #[tokio::test]
    async fn fails_when_the_engine_cannot_start_the_container() {
        let utils = MockUtils::new(Ok(Some(125)));
        let mut state = state();

        let Ok(outcome) = Launch::with_utils(&utils).run(&mut state).await;

        assert_eq!(outcome, Outcome::Failed);
        assert_eq!(
            state.error.as_deref(),
            Some("docker could not start the container (exit code 125).")
        );
    }

    #[tokio::test]
    async fn fails_when_the_engine_cannot_be_run() {
        let utils = MockUtils::new(Err(io::ErrorKind::NotFound.into()));
        let mut state = state();

        let Ok(outcome) = Launch::with_utils(&utils).run(&mut state).await;

        assert_eq!(outcome, Outcome::Failed);
        assert_eq!(state.error.as_deref(), Some("Could not run docker: mock"));
    }
}
