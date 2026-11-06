//! Talking to an OpenAI-compatible LLM server (Ollama, LM Studio and others).

use serde::Deserialize;
use std::sync::OnceLock;
use std::time::Duration;

/// How long to wait for the server before giving up.
const TIMEOUT: Duration = Duration::from_secs(5);
/// Endpoint listing the models a server offers, relative to its base URL.
const MODELS_PATH: &str = "/v1/models";

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

/// Lists the model ids served at `base_url` (a URL without `/v1`), sorted alphabetically.
///
/// # Errors
///
/// Returns an error if the server cannot be reached, answers with an error status, or the
/// response is not an OpenAI-style model list.
pub async fn list_models(base_url: &str) -> Result<Vec<String>, reqwest::Error> {
    let list: ModelList = client()
        .get(format!("{base_url}{MODELS_PATH}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut ids: Vec<String> = list.data.into_iter().map(|m| m.id).collect();
    ids.sort();
    Ok(ids)
}
