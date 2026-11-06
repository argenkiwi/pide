use std::path::{Path, PathBuf};

pub mod build_image;
pub mod check_config;
pub mod count;
pub mod detect_runtime;
pub mod ensure_containerfile;
pub mod ensure_prompt;
pub mod enter_host;
pub mod fail;
pub mod fetch_models;
pub mod finish;
pub mod launch;
pub mod resolve_target;
pub mod save_config;
pub mod select_model;
pub mod start;
pub mod stop;

/// A state that has a count field.
pub trait HasCount {
    fn count(&self) -> i32;
    fn count_mut(&mut self) -> &mut i32;
}

/// A state that targets a project folder.
pub trait HasTarget {
    fn target(&self) -> &Path;
    fn target_mut(&mut self) -> &mut PathBuf;
}

/// A state that knows which container engine (`podman` or `docker`) to drive.
pub trait HasRuntime {
    fn runtime(&self) -> &str;
    fn runtime_mut(&mut self) -> &mut String;
}

/// A state that knows the LLM server, both as the host sees it and as the container sees it.
pub trait HasServer {
    fn host_url(&self) -> &str;
    fn host_url_mut(&mut self) -> &mut String;
    fn container_url(&self) -> &str;
    fn container_url_mut(&mut self) -> &mut String;
}

/// A state that holds the models a server offers.
pub trait HasModels {
    fn models(&self) -> &[String];
    fn models_mut(&mut self) -> &mut Vec<String>;
}

/// A state that holds the selected model.
pub trait HasModel {
    fn model(&self) -> &str;
    fn model_mut(&mut self) -> &mut String;
}

/// A state that names a container image.
pub trait HasImage {
    fn image(&self) -> &str;
    fn image_mut(&mut self) -> &mut String;
}

/// A state that can record why the walk failed.
pub trait HasError {
    fn error(&self) -> Option<&str>;
    fn error_mut(&mut self) -> &mut Option<String>;
}
