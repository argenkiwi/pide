//! Reading and writing the Pi agent's `models.json` and `settings.json`.

use serde_json::{Value, json};

/// Name of the single provider pico configures.
pub const PROVIDER: &str = "local";
/// Path the OpenAI-compatible API lives under on the server.
pub const API_PATH: &str = "/v1";

/// The server and model a Pi configuration points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Server URL without the [`API_PATH`] suffix, e.g. `http://host.docker.internal:11434`.
    pub base_url: String,
    pub model: String,
}

/// Reads the default provider's server URL and the default model.
///
/// Returns `None` unless both files are valid JSON, `settings.json` names a default provider and
/// model, and that provider has a `baseUrl` and lists the model.
#[must_use]
pub fn parse(models_json: &str, settings_json: &str) -> Option<Config> {
    let models: Value = serde_json::from_str(models_json).ok()?;
    let settings: Value = serde_json::from_str(settings_json).ok()?;
    let provider = settings.get("defaultProvider")?.as_str()?;
    let model = settings.get("defaultModel")?.as_str()?;
    let entry = models.get("providers")?.get(provider)?;
    let base_url = entry.get("baseUrl")?.as_str()?.trim_end_matches('/');
    let listed = entry
        .get("models")?
        .as_array()?
        .iter()
        .any(|m| m.get("id").and_then(Value::as_str) == Some(model));
    listed.then(|| Config {
        base_url: base_url
            .strip_suffix(API_PATH)
            .unwrap_or(base_url)
            .to_string(),
        model: model.to_string(),
    })
}

/// Renders `models.json` with one OpenAI-compatible provider serving `model` from `base_url`.
#[must_use]
pub fn render_models(base_url: &str, model: &str) -> String {
    let value = json!({
        "$schema": "https://pi.dev/schemas/models.schema.json",
        "providers": {
            PROVIDER: {
                "baseUrl": format!("{base_url}{API_PATH}"),
                "api": "openai-completions",
                "apiKey": PROVIDER,
                "compat": { "supportsDeveloperRole": false },
                "models": [{
                    "id": model,
                    "name": model,
                    "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 }
                }]
            }
        }
    });
    pretty(&value)
}

/// Renders `settings.json` making [`PROVIDER`] and `model` the defaults.
#[must_use]
pub fn render_settings(model: &str) -> String {
    let value = json!({
        "$schema": "https://pi.dev/schemas/settings.schema.json",
        "defaultProvider": PROVIDER,
        "defaultModel": model
    });
    pretty(&value)
}

fn pretty(value: &Value) -> String {
    // Serialising a `Value` cannot fail.
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_files_parse_back() {
        let models = render_models("http://host.docker.internal:11434", "qwen3:8b");
        let settings = render_settings("qwen3:8b");

        assert_eq!(
            parse(&models, &settings),
            Some(Config {
                base_url: "http://host.docker.internal:11434".into(),
                model: "qwen3:8b".into(),
            })
        );
    }

    #[test]
    fn rendered_models_use_the_v1_api() {
        let models: Value = serde_json::from_str(&render_models("http://h:1", "m")).unwrap();

        assert_eq!(models["providers"]["local"]["baseUrl"], "http://h:1/v1");
        assert_eq!(
            models["providers"]["local"]["compat"]["supportsDeveloperRole"],
            false
        );
    }

    #[test]
    fn parse_rejects_a_model_the_provider_does_not_list() {
        let models = render_models("http://h:1", "a");
        let settings = render_settings("b");

        assert_eq!(parse(&models, &settings), None);
    }

    #[test]
    fn parse_rejects_missing_fields_and_bad_json() {
        assert_eq!(parse("{}", &render_settings("m")), None);
        assert_eq!(parse(&render_models("http://h:1", "m"), "{}"), None);
        assert_eq!(parse("not json", "{}"), None);
    }
}
