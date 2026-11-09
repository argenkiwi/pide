//! Sets up a containerised Pi coding agent for a project, backed by a local LLM server, and
//! launches it.
//!
//! See `specs/pide.md`.

use crate::engine::{Node, Transition, Walk};
use crate::nodes::build_image::{self, BuildImage};
use crate::nodes::check_config::{self, CheckConfig};
use crate::nodes::detect_runtime::{self, DetectRuntime};
use crate::nodes::ensure_containerfile::{self, EnsureContainerfile};
use crate::nodes::ensure_prompt::{self, EnsurePrompt};
use crate::nodes::enter_host::{self, EnterHost};
use crate::nodes::fail::{self, Fail};
use crate::nodes::fetch_models::{self, FetchModels};
use crate::nodes::finish::{self, Finish};
use crate::nodes::launch::{self, Launch};
use crate::nodes::resolve_target::{self, ResolveTarget};
use crate::nodes::save_config::{self, SaveConfig};
use crate::nodes::select_model::{self, SelectModel};
use crate::nodes::{HasError, HasImage, HasModel, HasModels, HasRuntime, HasServer, HasTarget};
use std::env;
use std::path::{Path, PathBuf};

/// What can stop the walk early.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    EnsureContainerfile(#[from] ensure_containerfile::Error),
    #[error(transparent)]
    EnterHost(#[from] enter_host::Error),
    #[error(transparent)]
    SelectModel(#[from] select_model::Error),
    #[error(transparent)]
    SaveConfig(#[from] save_config::Error),
    #[error(transparent)]
    EnsurePrompt(#[from] ensure_prompt::Error),
    #[error(transparent)]
    BuildImage(#[from] build_image::Error),
    /// The walk ended at `FAIL`, which has already reported why.
    #[error("pide stopped before Pi could run")]
    Failed,
}

#[derive(Clone, Debug, Default)]
struct State {
    target: PathBuf,
    runtime: String,
    host_url: String,
    container_url: String,
    models: Vec<String>,
    model: String,
    image: String,
    error: Option<String>,
}

impl HasTarget for State {
    fn target(&self) -> &Path {
        &self.target
    }
    fn target_mut(&mut self) -> &mut PathBuf {
        &mut self.target
    }
}

impl HasRuntime for State {
    fn runtime(&self) -> &str {
        &self.runtime
    }
    fn runtime_mut(&mut self) -> &mut String {
        &mut self.runtime
    }
}

impl HasServer for State {
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

impl HasModels for State {
    fn models(&self) -> &[String] {
        &self.models
    }
    fn models_mut(&mut self) -> &mut Vec<String> {
        &mut self.models
    }
}

impl HasModel for State {
    fn model(&self) -> &str {
        &self.model
    }
    fn model_mut(&mut self) -> &mut String {
        &mut self.model
    }
}

impl HasImage for State {
    fn image(&self) -> &str {
        &self.image
    }
    fn image_mut(&mut self) -> &mut String {
        &mut self.image
    }
}

impl HasError for State {
    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    fn error_mut(&mut self) -> &mut Option<String> {
        &mut self.error
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Id {
    ResolveTarget,
    DetectRuntime,
    EnsureContainerfile,
    CheckConfig,
    EnterHost,
    FetchModels,
    SelectModel,
    SaveConfig,
    EnsurePrompt,
    BuildImage,
    Launch,
    Finish,
    Fail,
}

#[derive(Default)]
struct Pide<
    A = resolve_target::SystemUtils,
    B = detect_runtime::SystemUtils,
    C = ensure_containerfile::SystemUtils,
    D = check_config::SystemUtils,
    E = enter_host::SystemUtils,
    F = fetch_models::SystemUtils,
    G = select_model::SystemUtils,
    H = save_config::SystemUtils,
    I = ensure_prompt::SystemUtils,
    J = build_image::SystemUtils,
    K = launch::SystemUtils,
    L = finish::SystemUtils,
    M = fail::SystemUtils,
> {
    resolve_target: ResolveTarget<A>,
    detect_runtime: DetectRuntime<B>,
    ensure_containerfile: EnsureContainerfile<C>,
    check_config: CheckConfig<D>,
    enter_host: EnterHost<E>,
    fetch_models: FetchModels<F>,
    select_model: SelectModel<G>,
    save_config: SaveConfig<H>,
    ensure_prompt: EnsurePrompt<I>,
    build_image: BuildImage<J>,
    launch: Launch<K>,
    finish: Finish<L>,
    fail: Fail<M>,
}

impl Pide {
    /// Production wiring: every node uses its `SystemUtils`.
    fn new() -> Self {
        Self::default()
    }
}

impl<A, B, C, D, E, F, G, H, I, J, K, L, M> Walk for Pide<A, B, C, D, E, F, G, H, I, J, K, L, M>
where
    A: resolve_target::Utils + Sync,
    B: detect_runtime::Utils + Sync,
    C: ensure_containerfile::Utils + Sync,
    D: check_config::Utils + Sync,
    E: enter_host::Utils + Sync,
    F: fetch_models::Utils + Sync,
    G: select_model::Utils + Sync,
    H: save_config::Utils + Sync,
    I: ensure_prompt::Utils + Sync,
    J: build_image::Utils + Sync,
    K: launch::Utils + Sync,
    L: finish::Utils + Sync,
    M: fail::Utils + Sync,
{
    type Id = Id;
    type State = State;
    type Error = Error;

    async fn step(&self, id: Id, state: &mut State) -> Result<Transition<Id>, Error> {
        Ok(match id {
            Id::ResolveTarget => {
                let Ok(outcome) = self.resolve_target.run(state).await;
                match outcome {
                    resolve_target::Outcome::Found => Transition::Next(Id::DetectRuntime),
                    resolve_target::Outcome::Missing => Transition::Next(Id::Fail),
                }
            }
            Id::DetectRuntime => {
                let Ok(outcome) = self.detect_runtime.run(state).await;
                match outcome {
                    detect_runtime::Outcome::Ready => Transition::Next(Id::EnsureContainerfile),
                    detect_runtime::Outcome::Unavailable => Transition::Next(Id::Fail),
                }
            }
            Id::EnsureContainerfile => match self.ensure_containerfile.run(state).await? {
                ensure_containerfile::Outcome::Ready => Transition::Next(Id::CheckConfig),
            },
            Id::CheckConfig => {
                let Ok(outcome) = self.check_config.run(state).await;
                match outcome {
                    check_config::Outcome::Configured => Transition::Next(Id::EnsurePrompt),
                    check_config::Outcome::NeedsSetup => Transition::Next(Id::EnterHost),
                }
            }
            Id::EnterHost => match self.enter_host.run(state).await? {
                enter_host::Outcome::Entered => Transition::Next(Id::FetchModels),
                enter_host::Outcome::Invalid => Transition::Next(Id::EnterHost),
                enter_host::Outcome::Cancelled => Transition::Next(Id::Fail),
            },
            Id::FetchModels => {
                let Ok(outcome) = self.fetch_models.run(state).await;
                match outcome {
                    fetch_models::Outcome::Found => Transition::Next(Id::SelectModel),
                    fetch_models::Outcome::Unreachable | fetch_models::Outcome::Empty => {
                        Transition::Next(Id::EnterHost)
                    }
                }
            }
            Id::SelectModel => match self.select_model.run(state).await? {
                select_model::Outcome::Selected => Transition::Next(Id::SaveConfig),
                select_model::Outcome::Invalid => Transition::Next(Id::SelectModel),
                select_model::Outcome::Cancelled => Transition::Next(Id::Fail),
            },
            Id::SaveConfig => match self.save_config.run(state).await? {
                save_config::Outcome::Saved => Transition::Next(Id::EnsurePrompt),
            },
            Id::EnsurePrompt => match self.ensure_prompt.run(state).await? {
                ensure_prompt::Outcome::Ready => Transition::Next(Id::BuildImage),
            },
            Id::BuildImage => match self.build_image.run(state).await? {
                build_image::Outcome::Ready => Transition::Next(Id::Launch),
                build_image::Outcome::Failed => Transition::Next(Id::Fail),
            },
            Id::Launch => {
                let Ok(outcome) = self.launch.run(state).await;
                match outcome {
                    launch::Outcome::Exited => Transition::Next(Id::Finish),
                    launch::Outcome::Failed => Transition::Next(Id::Fail),
                }
            }
            Id::Finish => {
                let Ok(()) = self.finish.run(state).await;
                Transition::Halt
            }
            Id::Fail => {
                let Ok(()) = self.fail.run(state).await;
                Transition::Halt
            }
        })
    }
}

/// Runs pide against the folder given as the first argument, or the current folder.
///
/// # Errors
///
/// Returns [`Error::Failed`] if the walk ended at `FAIL` (which has already printed why), or
/// another [`Error`] if reading input or writing pide's files failed.
pub async fn run() -> Result<(), Error> {
    let target = env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let mut state = State {
        target,
        ..State::default()
    };
    Pide::new().run(Id::ResolveTarget, &mut state).await?;
    match state.error {
        Some(_) => Err(Error::Failed),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{layout, pi_config};
    use std::collections::{HashMap, VecDeque};
    use std::io;
    use std::sync::Mutex;

    const TARGET: &str = "/home/me/app";

    /// Plays back a scripted environment: which folders and engines exist, what the user types,
    /// what the server answers, and the file system; records what was printed, built and run.
    struct Script {
        target_exists: bool,
        engines: &'static [&'static str],
        lines: Mutex<VecDeque<&'static str>>,
        model_lists: Mutex<VecDeque<Result<Vec<String>, String>>>,
        files: Mutex<HashMap<PathBuf, String>>,
        label: Option<String>,
        build_ok: bool,
        exit: Option<i32>,
        builds: Mutex<usize>,
        launches: Mutex<usize>,
        printed: Mutex<Vec<String>>,
    }

    impl Default for Script {
        fn default() -> Self {
            Self {
                target_exists: true,
                engines: &["podman"],
                lines: Mutex::default(),
                model_lists: Mutex::default(),
                files: Mutex::default(),
                label: None,
                build_ok: true,
                exit: Some(0),
                builds: Mutex::default(),
                launches: Mutex::default(),
                printed: Mutex::default(),
            }
        }
    }

    impl Script {
        fn with_lines(mut self, lines: &[&'static str]) -> Self {
            self.lines = Mutex::new(lines.iter().copied().collect());
            self
        }

        fn with_model_lists(mut self, lists: Vec<Result<&[&str], &str>>) -> Self {
            self.model_lists = Mutex::new(
                lists
                    .into_iter()
                    .map(|list| {
                        list.map(|ids| ids.iter().map(ToString::to_string).collect())
                            .map_err(str::to_string)
                    })
                    .collect(),
            );
            self
        }

        fn with_file(self, path: PathBuf, contents: &str) -> Self {
            self.files
                .lock()
                .unwrap()
                .insert(path, contents.to_string());
            self
        }

        fn record(&self, message: &str) {
            self.printed.lock().unwrap().push(message.to_string());
        }

        fn read_line(&self) -> Option<String> {
            self.lines.lock().unwrap().pop_front().map(str::to_string)
        }

        fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap().contains_key(path)
        }

        fn read(&self, path: &Path) -> io::Result<String> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| io::ErrorKind::NotFound.into())
        }

        fn write(&self, path: &Path, contents: &str) {
            self.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), contents.to_string());
        }

        fn file(&self, path: PathBuf) -> Option<String> {
            self.files.lock().unwrap().get(&path).cloned()
        }
    }

    impl resolve_target::Utils for &Script {
        fn resolve_dir(&self, _path: &Path) -> Option<PathBuf> {
            self.target_exists.then(|| PathBuf::from(TARGET))
        }
    }

    impl detect_runtime::Utils for &Script {
        fn is_installed(&self, engine: &str) -> bool {
            self.engines.contains(&engine)
        }
        async fn is_running(&self, engine: &str) -> bool {
            self.engines.contains(&engine)
        }
    }

    impl ensure_containerfile::Utils for &Script {
        fn exists(&self, path: &Path) -> bool {
            Script::exists(self, path)
        }
        fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
            Script::write(self, path, contents);
            Ok(())
        }
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl check_config::Utils for &Script {
        fn read(&self, path: &Path) -> io::Result<String> {
            Script::read(self, path)
        }
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl enter_host::Utils for &Script {
        fn read_line(&self, _message: &str) -> io::Result<Option<String>> {
            Ok(Script::read_line(self))
        }
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl fetch_models::Utils for &Script {
        async fn list_models(&self, _base_url: &str) -> Result<Vec<String>, String> {
            self.model_lists.lock().unwrap().pop_front().unwrap()
        }
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl select_model::Utils for &Script {
        fn read_line(&self, _message: &str) -> io::Result<Option<String>> {
            Ok(Script::read_line(self))
        }
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl save_config::Utils for &Script {
        fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
            Script::write(self, path, contents);
            Ok(())
        }
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl ensure_prompt::Utils for &Script {
        fn exists(&self, path: &Path) -> bool {
            Script::exists(self, path)
        }
        fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
            Script::write(self, path, contents);
            Ok(())
        }
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl build_image::Utils for &Script {
        fn read(&self, path: &Path) -> io::Result<String> {
            Script::read(self, path)
        }
        async fn image_label(
            &self,
            _engine: &str,
            _image: &str,
            _key: &str,
        ) -> io::Result<Option<String>> {
            Ok(self.label.clone())
        }
        async fn build(
            &self,
            _engine: &str,
            _image: &str,
            _containerfile: &Path,
            _context: &Path,
            _hash: &str,
        ) -> io::Result<bool> {
            *self.builds.lock().unwrap() += 1;
            Ok(self.build_ok)
        }
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl launch::Utils for &Script {
        fn git_config(&self, _key: &str) -> Option<String> {
            None
        }
        fn is_terminal(&self) -> bool {
            true
        }
        async fn run(&self, _engine: &str, _args: &[String]) -> io::Result<Option<i32>> {
            *self.launches.lock().unwrap() += 1;
            Ok(self.exit)
        }
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl finish::Utils for &Script {
        fn print(&self, message: &str) {
            self.record(message);
        }
    }

    impl fail::Utils for &Script {
        fn print_error(&self, message: &str) {
            self.record(message);
        }
    }

    type Scripted<'a> = Pide<
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
        &'a Script,
    >;

    fn pide(script: &Script) -> Scripted<'_> {
        Pide {
            resolve_target: ResolveTarget::with_utils(script),
            detect_runtime: DetectRuntime::with_utils(script),
            ensure_containerfile: EnsureContainerfile::with_utils(script),
            check_config: CheckConfig::with_utils(script),
            enter_host: EnterHost::with_utils(script),
            fetch_models: FetchModels::with_utils(script),
            select_model: SelectModel::with_utils(script),
            save_config: SaveConfig::with_utils(script),
            ensure_prompt: EnsurePrompt::with_utils(script),
            build_image: BuildImage::with_utils(script),
            launch: Launch::with_utils(script),
            finish: Finish::with_utils(script),
            fail: Fail::with_utils(script),
        }
    }

    async fn walk(script: &Script) -> State {
        let mut state = State {
            target: ".".into(),
            ..State::default()
        };
        pide(script)
            .run(Id::ResolveTarget, &mut state)
            .await
            .unwrap();
        state
    }

    fn target() -> &'static Path {
        Path::new(TARGET)
    }

    fn last_printed(script: &Script) -> String {
        script.printed.lock().unwrap().last().cloned().unwrap()
    }

    /// A project pide has already set up: Containerfile, config and prompt all exist.
    fn configured() -> Script {
        Script::default()
            .with_file(
                layout::containerfile(target()),
                ensure_containerfile::BASE_CONTAINERFILE,
            )
            .with_file(layout::gitignore(target()), ensure_containerfile::GITIGNORE)
            .with_file(
                layout::models_json(target()),
                &pi_config::render_models("http://host.docker.internal:11434", "qwen3:8b"),
            )
            .with_file(
                layout::settings_json(target()),
                &pi_config::render_settings("qwen3:8b"),
            )
            .with_file(layout::containerfile_prompt(target()), "custom prompt")
    }

    #[tokio::test]
    async fn sets_up_a_new_project_retrying_bad_input_then_launches() {
        let script = Script::default()
            .with_lines(&["ftp://nope", "", "localhost:1234", "9", "2"])
            .with_model_lists(vec![Err("connection refused"), Ok(&["llama3", "qwen3:8b"])]);

        let state = walk(&script).await;

        assert_eq!(state.error, None);
        assert_eq!(state.runtime, "podman");
        assert_eq!(state.host_url, "http://localhost:1234");
        assert_eq!(state.container_url, "http://host.docker.internal:1234");
        assert_eq!(state.model, "qwen3:8b");
        assert_eq!(state.image, layout::image_name(target()));
        assert_eq!(
            script.file(layout::containerfile(target())).as_deref(),
            Some(ensure_containerfile::BASE_CONTAINERFILE)
        );
        assert_eq!(
            script.file(layout::models_json(target())),
            Some(pi_config::render_models(
                "http://host.docker.internal:1234",
                "qwen3:8b"
            ))
        );
        assert_eq!(
            script
                .file(layout::containerfile_prompt(target()))
                .as_deref(),
            Some(ensure_prompt::PROMPT)
        );
        assert_eq!(*script.builds.lock().unwrap(), 1);
        assert_eq!(*script.launches.lock().unwrap(), 1);
        assert_eq!(
            last_printed(&script),
            "Changes made with /containerfile take effect the next time you run pide."
        );
    }

    #[tokio::test]
    async fn goes_straight_to_launch_for_a_configured_project_with_a_current_image() {
        let mut script = configured();
        script.label = Some(layout::content_hash(
            ensure_containerfile::BASE_CONTAINERFILE,
        ));

        let state = walk(&script).await;

        assert_eq!(state.error, None);
        assert_eq!(state.model, "qwen3:8b");
        assert_eq!(*script.builds.lock().unwrap(), 0);
        assert_eq!(*script.launches.lock().unwrap(), 1);
        assert_eq!(
            script
                .file(layout::containerfile_prompt(target()))
                .as_deref(),
            Some("custom prompt")
        );
    }

    #[tokio::test]
    async fn retries_the_host_when_the_server_has_no_models() {
        let script = Script::default()
            .with_lines(&["", "", "1"])
            .with_model_lists(vec![Ok(&[]), Ok(&["llama3"])]);

        let state = walk(&script).await;

        assert_eq!(state.error, None);
        assert_eq!(state.model, "llama3");
    }

    #[tokio::test]
    async fn fails_if_the_target_is_missing() {
        let script = Script {
            target_exists: false,
            ..Script::default()
        };

        let state = walk(&script).await;

        assert!(state.error.is_some());
        assert_eq!(
            last_printed(&script),
            "Error: The target folder . does not exist or is not a folder."
        );
        assert_eq!(*script.launches.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn fails_without_a_container_engine() {
        let script = Script {
            engines: &[],
            ..Script::default()
        };

        let state = walk(&script).await;

        assert!(state.error.is_some());
        assert!(last_printed(&script).contains("Neither podman nor docker"));
    }

    #[tokio::test]
    async fn fails_if_input_ends_during_setup() {
        let script = Script::default().with_lines(&[]);

        let state = walk(&script).await;

        assert_eq!(state.error.as_deref(), Some("No server URL was entered."));
        assert_eq!(*script.launches.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn fails_if_input_ends_during_model_selection() {
        let script = Script::default()
            .with_lines(&[""])
            .with_model_lists(vec![Ok(&["llama3"])]);

        let state = walk(&script).await;

        assert_eq!(state.error.as_deref(), Some("No model was selected."));
    }

    #[tokio::test]
    async fn fails_if_the_build_fails() {
        let script = Script {
            build_ok: false,
            ..configured()
        };

        let state = walk(&script).await;

        assert!(state.error.unwrap().starts_with("Building the image"));
        assert_eq!(*script.launches.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn fails_if_the_container_cannot_start() {
        let script = Script {
            exit: Some(125),
            ..configured()
        };

        let state = walk(&script).await;

        assert_eq!(
            state.error.as_deref(),
            Some("podman could not start the container (exit code 125).")
        );
    }
}
