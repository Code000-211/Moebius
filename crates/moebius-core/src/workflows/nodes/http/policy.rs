//! The URL policy of the `http` node: only `https` or plain `http`, no user name or password, and only a host the
//! server configuration names. A refused call fails the node before any request is made.

use reqwest::Url;

use super::{NodeError, Request};

impl Request {
    /// The URL, if the policy allows the call: `https` or `http`, no user name or password, and a host the server
    /// configuration names.
    pub(super) fn checked_url(&self) -> Result<Url, NodeError> {
        let refuse = |reason: String| {
            NodeError::Failed(format!("the URL policy refuses the call: {reason}"))
        };
        let url = Url::parse(&self.url).map_err(|e| refuse(format!("not a URL ({e})")))?;
        if !matches!(url.scheme(), "https" | "http") {
            return Err(refuse(format!(
                "the scheme \"{}\" is not https or http",
                url.scheme()
            )));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(refuse(
                "the URL carries a user name or password; name a secret instead".to_string(),
            ));
        }
        let host = url
            .host_str()
            .ok_or_else(|| refuse("the URL has no host".to_string()))?
            .to_ascii_lowercase();
        let allowed = self
            .settings
            .as_ref()
            .is_some_and(|settings| settings.hosts.contains(&host));
        if !allowed {
            return Err(refuse(format!(
                "the host \"{host}\" is not in workflows.http_hosts"
            )));
        }
        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{fails, node, settings};
    use serde_json::json;

    #[test]
    fn the_url_policy_refuses_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let allow = settings(dir.path(), &["api.example.com"]);
        for (url, reason) in [
            (
                "https://other.example.com/x",
                "the host \"other.example.com\" is not in workflows.http_hosts",
            ),
            (
                "ftp://api.example.com/x",
                "the scheme \"ftp\" is not https or http",
            ),
            (
                "file:///etc/passwd",
                "the scheme \"file\" is not https or http",
            ),
            (
                "https://user:pw@api.example.com/x",
                "the URL carries a user name or password",
            ),
            ("not a url", "not a URL"),
        ] {
            let message = fails(&node(json!({"url": url}), allow.clone()));
            assert!(
                message.starts_with("the URL policy refuses the call: ")
                    && message.contains(reason),
                "{url}: {message}"
            );
        }
        // Without any host in the configuration nothing is allowed.
        let message = fails(&node(
            json!({"url": "https://api.example.com/"}),
            settings(dir.path(), &[]),
        ));
        assert!(message.contains("not in workflows.http_hosts"), "{message}");
        let message = fails(&node(json!({"url": "https://api.example.com/"}), None));
        assert!(message.contains("not in workflows.http_hosts"), "{message}");
    }
}
