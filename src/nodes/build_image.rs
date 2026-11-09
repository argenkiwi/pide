use crate::engine::Node;
use crate::nodes::{HasError, HasImage, HasRuntime, HasTarget};
use crate::utils::{container, files, layout};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};

/// Where [`BuildImage`] leaves the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The image matches the Containerfile, either already or after a build.
    Ready,
    /// The build failed; the error is recorded.
    Failed,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not run {engine}")]
    Engine {
        engine: String,
        #[source]
        source: io::Error,
    },
}

pub trait Utils {
    fn read(&self, path: &Path) -> io::Result<String>;
    /// Reads label `key` from `image`, or `None` if the image or label does not exist.
    fn image_label(
        &self,
        engine: &str,
        image: &str,
        key: &str,
    ) -> impl Future<Output = io::Result<Option<String>>> + Send;
    /// Builds `image` from `containerfile` in `context`, labelled with `hash`, showing the
    /// engine's output. Returns whether the build succeeded.
    fn build(
        &self,
        engine: &str,
        image: &str,
        containerfile: &Path,
        context: &Path,
        hash: &str,
    ) -> impl Future<Output = io::Result<bool>> + Send;
    fn print(&self, message: &str);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemUtils;

impl Utils for SystemUtils {
    fn read(&self, path: &Path) -> io::Result<String> {
        files::read(path)
    }

    fn image_label(
        &self,
        engine: &str,
        image: &str,
        key: &str,
    ) -> impl Future<Output = io::Result<Option<String>>> + Send {
        container::image_label(engine, image, key)
    }

    async fn build(
        &self,
        engine: &str,
        image: &str,
        containerfile: &Path,
        context: &Path,
        hash: &str,
    ) -> io::Result<bool> {
        container::build(
            engine,
            image,
            containerfile,
            context,
            &[(layout::HASH_LABEL, hash)],
        )
        .await
    }

    fn print(&self, message: &str) {
        println!("{message}");
    }
}

/// Builds the project's image unless one built from the current Containerfile already exists.
#[derive(Clone, Debug, Default)]
pub struct BuildImage<U = SystemUtils> {
    utils: U,
}

impl BuildImage {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<U> BuildImage<U> {
    #[must_use]
    pub fn with_utils(utils: U) -> Self {
        Self { utils }
    }
}

impl<S, U> Node<S> for BuildImage<U>
where
    S: HasTarget + HasRuntime + HasImage + HasError + Send,
    U: Utils + Sync,
{
    type Outcome = Outcome;
    type Error = Error;

    async fn run(&self, state: &mut S) -> Result<Outcome, Error> {
        let containerfile = layout::containerfile(state.target());
        let contents = self
            .utils
            .read(&containerfile)
            .map_err(|source| Error::Read {
                path: containerfile.clone(),
                source,
            })?;
        let hash = layout::content_hash(&contents);
        let image = layout::image_name(state.target());
        let engine = state.runtime().to_string();
        let engine_error = |source| Error::Engine {
            engine: engine.clone(),
            source,
        };
        *state.image_mut() = image.clone();

        let label = self
            .utils
            .image_label(&engine, &image, layout::HASH_LABEL)
            .await
            .map_err(engine_error)?;
        if label.as_deref() == Some(hash.as_str()) {
            self.utils.print(&format!("Image {image} is up to date."));
            return Ok(Outcome::Ready);
        }

        self.utils.print(&format!("Building image {image}..."));
        let context = layout::pide_dir(state.target());
        let built = self
            .utils
            .build(&engine, &image, &containerfile, &context, &hash)
            .await
            .map_err(engine_error)?;
        if built {
            return Ok(Outcome::Ready);
        }
        *state.error_mut() = Some(format!(
            "Building the image from {} failed. Fix the Containerfile and run pide again.",
            containerfile.display()
        ));
        Ok(Outcome::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    const CONTAINERFILE: &str = "FROM scratch\n";

    #[derive(Default)]
    struct MockUtils {
        unreadable: bool,
        label: Option<String>,
        build_fails: bool,
        built: Mutex<Vec<String>>,
        printed: Mutex<Vec<String>>,
    }

    impl Utils for &MockUtils {
        fn read(&self, _path: &Path) -> io::Result<String> {
            if self.unreadable {
                return Err(io::ErrorKind::NotFound.into());
            }
            Ok(CONTAINERFILE.to_string())
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
            engine: &str,
            image: &str,
            containerfile: &Path,
            context: &Path,
            hash: &str,
        ) -> io::Result<bool> {
            self.built.lock().unwrap().push(format!(
                "{engine} {image} {} {} {hash}",
                containerfile.display(),
                context.display()
            ));
            Ok(!self.build_fails)
        }
        fn print(&self, message: &str) {
            self.printed.lock().unwrap().push(message.to_string());
        }
    }

    fn state() -> MockState {
        MockState {
            target: "/p/app".into(),
            runtime: "podman".into(),
            ..MockState::default()
        }
    }

    #[tokio::test]
    async fn skips_the_build_when_the_image_is_up_to_date() {
        let utils = MockUtils {
            label: Some(layout::content_hash(CONTAINERFILE)),
            ..MockUtils::default()
        };
        let mut state = state();

        let outcome = BuildImage::with_utils(&utils)
            .run(&mut state)
            .await
            .unwrap();

        let image = layout::image_name(Path::new("/p/app"));
        assert_eq!(outcome, Outcome::Ready);
        assert_eq!(state.image, image);
        assert!(utils.built.lock().unwrap().is_empty());
        assert_eq!(
            *utils.printed.lock().unwrap(),
            [format!("Image {image} is up to date.")]
        );
    }

    #[tokio::test]
    async fn builds_when_the_containerfile_changed() {
        let utils = MockUtils {
            label: Some("stale".into()),
            ..MockUtils::default()
        };
        let mut state = state();

        let outcome = BuildImage::with_utils(&utils)
            .run(&mut state)
            .await
            .unwrap();

        let image = layout::image_name(Path::new("/p/app"));
        assert_eq!(outcome, Outcome::Ready);
        assert_eq!(
            *utils.built.lock().unwrap(),
            [format!(
                "podman {image} /p/app/.pide/Containerfile /p/app/.pide {}",
                layout::content_hash(CONTAINERFILE)
            )]
        );
    }

    #[tokio::test]
    async fn records_an_error_when_the_build_fails() {
        let utils = MockUtils {
            build_fails: true,
            ..MockUtils::default()
        };
        let mut state = state();

        let outcome = BuildImage::with_utils(&utils)
            .run(&mut state)
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Failed);
        assert_eq!(
            state.error.as_deref(),
            Some(
                "Building the image from /p/app/.pide/Containerfile failed. Fix the Containerfile and run pide again."
            )
        );
    }

    #[tokio::test]
    async fn fails_if_the_containerfile_cannot_be_read() {
        let utils = MockUtils {
            unreadable: true,
            ..MockUtils::default()
        };

        let result = BuildImage::with_utils(&utils).run(&mut state()).await;

        assert!(matches!(result, Err(Error::Read { .. })));
    }
}
