//! Cleaning up LLM server URLs and translating them for use inside a container.

use reqwest::Url;

/// Host name a container uses to reach the machine it runs on.
pub const CONTAINER_HOST: &str = "host.docker.internal";

/// Host names that mean "this machine" and so must be rewritten inside a container.
const LOOPBACK_HOSTS: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

/// Cleans up a server URL typed by the user.
///
/// Adds `http://` when no scheme is given, and drops trailing slashes, a trailing `/v1`, any
/// query and any fragment. Returns `None` unless the result is an `http` or `https` URL with a
/// host.
#[must_use]
pub fn normalise(input: &str) -> Option<String> {
    let input = input.trim();
    let with_scheme = if input.contains("://") {
        input.to_string()
    } else {
        format!("http://{input}")
    };
    let mut url = Url::parse(&with_scheme).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return None;
    }
    let path = url.path().trim_end_matches('/');
    let path = path.strip_suffix("/v1").unwrap_or(path).to_string();
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Some(url.as_str().trim_end_matches('/').to_string())
}

/// Rewrites a loopback host (`localhost`, `127.0.0.1`, `::1`) to [`CONTAINER_HOST`], so the
/// container reaches the server on the host. Other URLs are returned unchanged.
#[must_use]
pub fn for_container(url: &str) -> String {
    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_string();
    };
    if parsed
        .host_str()
        .is_some_and(|host| LOOPBACK_HOSTS.contains(&host))
        && parsed.set_host(Some(CONTAINER_HOST)).is_ok()
    {
        return parsed.as_str().trim_end_matches('/').to_string();
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_keeps_a_clean_url() {
        assert_eq!(
            normalise("http://localhost:11434").as_deref(),
            Some("http://localhost:11434")
        );
    }

    #[test]
    fn normalise_adds_a_missing_scheme() {
        assert_eq!(
            normalise(" localhost:1234 ").as_deref(),
            Some("http://localhost:1234")
        );
    }

    #[test]
    fn normalise_drops_trailing_slashes_and_v1() {
        assert_eq!(
            normalise("https://llm.example.com/api/v1/").as_deref(),
            Some("https://llm.example.com/api")
        );
    }

    #[test]
    fn normalise_rejects_other_schemes_and_garbage() {
        assert_eq!(normalise("ftp://host"), None);
        assert_eq!(normalise("http://"), None);
        assert_eq!(normalise("not a url"), None);
    }

    #[test]
    fn for_container_rewrites_loopback_hosts() {
        assert_eq!(
            for_container("http://localhost:11434"),
            "http://host.docker.internal:11434"
        );
        assert_eq!(
            for_container("http://127.0.0.1:1234/base"),
            "http://host.docker.internal:1234/base"
        );
        assert_eq!(
            for_container("http://[::1]:8080"),
            "http://host.docker.internal:8080"
        );
    }

    #[test]
    fn for_container_leaves_other_hosts_alone() {
        assert_eq!(
            for_container("http://192.168.1.20:11434"),
            "http://192.168.1.20:11434"
        );
    }
}
