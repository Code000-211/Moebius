//! The Jira client (issue #390): reads one Jira project through the REST API, for the import into a Moebius project
//! (`app::jira`). It reads only: the field catalogue, the issues of the project with every field, and the comments the
//! issues' own page left out. A Jira Cloud site is reached with an account's email and an API token (Basic), a Jira
//! Server or Data Center with a personal access token (Bearer, no email). The token is never part of an error.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::blobs::redact;

/// The issues asked for at once.
const PAGE: usize = 100;
/// The most pages read, so that a server that never ends its list is given up on.
const PAGE_LIMIT: usize = 2000;
/// The most characters of an answer's body an error shows.
const BODY_LIMIT: usize = 300;

/// Why reading Jira failed.
#[derive(Debug, thiserror::Error)]
pub enum JiraError {
    #[error("{0} is not the address of a Jira site: write it as https://your-site.atlassian.net")]
    Address(String),
    #[error("cannot reach Jira: {0}")]
    Http(String),
    #[error("Jira answered {status} to {path}: {body}")]
    Status {
        path: String,
        status: u16,
        body: String,
    },
    #[error("Jira answered something that is not what the API describes: {0}")]
    Shape(String),
}

/// How to reach a Jira site.
#[derive(Clone)]
pub struct Connection {
    /// The site's address, such as `https://your-site.atlassian.net`.
    pub url: String,
    /// The email of the account the token belongs to; none for a personal access token of a Server or Data Center.
    pub email: Option<String>,
    /// The API token. Not shown by `Debug`.
    pub token: String,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("url", &self.url)
            .field("email", &self.email)
            .field("token", &"<hidden>")
            .finish()
    }
}

/// A field of Jira's catalogue: how its issues name it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FieldInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub custom: bool,
    #[serde(default)]
    pub schema: Option<FieldSchema>,
}

/// The type of a field's value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FieldSchema {
    #[serde(rename = "type", default)]
    pub value_type: String,
    #[serde(default)]
    pub items: Option<String>,
    #[serde(default)]
    pub custom: Option<String>,
}

/// An issue as Jira gives it: its key, its fields as they are, and every comment.
#[derive(Debug, Clone, PartialEq)]
pub struct RawIssue {
    pub key: String,
    pub fields: Value,
    pub comments: Vec<Value>,
}

/// What reading a project brought.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub fields: Vec<FieldInfo>,
    pub issues: Vec<RawIssue>,
}

/// Whether `key` can be a Jira project's key: letters, digits and `_`, starting with a letter.
pub fn is_project_key(key: &str) -> bool {
    key.starts_with(|c: char| c.is_ascii_alphabetic())
        && key.len() <= 50
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The address of the site as it is used: `http` or `https`, no credentials in it, no path, no trailing slash.
pub fn site_address(text: &str) -> Result<String, JiraError> {
    let wrong = || JiraError::Address(redact(text));
    let url = reqwest::Url::parse(text.trim()).map_err(|_| wrong())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(wrong());
    }
    // A site that lives under a path (`/jira`) keeps it.
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// Reads `project` (a key) of the site: the field catalogue and every issue with its comments.
pub async fn fetch(connection: &Connection, project: &str) -> Result<Fetched, JiraError> {
    if !is_project_key(project) {
        return Err(JiraError::Shape(format!(
            "{project:?} is not a Jira project key"
        )));
    }
    let client = Client::new(connection)?;
    let fields: Vec<FieldInfo> = from_value(client.get("/rest/api/2/field", &[]).await?)?;
    let jql = format!("project = \"{project}\" ORDER BY created ASC, key ASC");
    let mut issues = client.search(&jql).await?;
    for issue in &mut issues {
        client.complete_comments(issue).await?;
    }
    Ok(Fetched { fields, issues })
}

fn from_value<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, JiraError> {
    serde_json::from_value(value).map_err(|e| JiraError::Shape(e.to_string()))
}

struct Client {
    base: String,
    email: Option<String>,
    token: String,
    http: reqwest::Client,
}

impl Client {
    fn new(connection: &Connection) -> Result<Self, JiraError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            // A redirect would send the token to wherever it points.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| JiraError::Http(e.to_string()))?;
        Ok(Self {
            base: site_address(&connection.url)?,
            email: connection.email.clone().filter(|e| !e.trim().is_empty()),
            token: connection.token.clone(),
            http,
        })
    }

    /// A GET of `path`, answered as JSON; the status of any other answer is the error.
    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, JiraError> {
        let mut url = reqwest::Url::parse(&format!("{}{path}", self.base))
            .map_err(|_| JiraError::Address(self.base.clone()))?;
        for (name, value) in query {
            url.query_pairs_mut().append_pair(name, value);
        }
        let request = self.http.get(url).header("Accept", "application/json");
        let request = match &self.email {
            Some(email) => request.basic_auth(email, Some(&self.token)),
            None => request.bearer_auth(&self.token),
        };
        let response = request
            .send()
            .await
            .map_err(|e| JiraError::Http(redact(&e.without_url().to_string())))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| JiraError::Http(redact(&e.without_url().to_string())))?;
        if !status.is_success() {
            let body: String = redact(&text).chars().take(BODY_LIMIT).collect();
            return Err(JiraError::Status {
                path: path.to_string(),
                status: status.as_u16(),
                body,
            });
        }
        serde_json::from_str(&text).map_err(|e| JiraError::Shape(e.to_string()))
    }

    /// The issues `jql` finds, through the search that pages by a token (Jira Cloud) and, where the site has none,
    /// the one that pages by position.
    async fn search(&self, jql: &str) -> Result<Vec<RawIssue>, JiraError> {
        let mut issues = Vec::new();
        let mut token: Option<String> = None;
        for page in 0..PAGE_LIMIT {
            let mut query = vec![
                ("jql", jql.to_string()),
                ("maxResults", PAGE.to_string()),
                ("fields", "*all".to_string()),
            ];
            if let Some(token) = &token {
                query.push(("nextPageToken", token.clone()));
            }
            match self.get("/rest/api/2/search/jql", &query).await {
                Ok(answer) => {
                    issues.extend(raw_issues(&answer)?);
                    token = answer["nextPageToken"].as_str().map(str::to_string);
                    if token.is_none() || answer["isLast"].as_bool() == Some(true) {
                        return Ok(issues);
                    }
                }
                Err(JiraError::Status { status, .. })
                    if page == 0 && matches!(status, 404 | 405 | 410) =>
                {
                    return self.search_by_position(jql).await;
                }
                Err(e) => return Err(e),
            }
        }
        Err(JiraError::Shape("the search never ends".into()))
    }

    async fn search_by_position(&self, jql: &str) -> Result<Vec<RawIssue>, JiraError> {
        let mut issues: Vec<RawIssue> = Vec::new();
        for _ in 0..PAGE_LIMIT {
            let query = [
                ("jql", jql.to_string()),
                ("startAt", issues.len().to_string()),
                ("maxResults", PAGE.to_string()),
                ("fields", "*all".to_string()),
            ];
            let answer = self.get("/rest/api/2/search", &query).await?;
            let page = raw_issues(&answer)?;
            let total = answer["total"].as_u64().unwrap_or(0) as usize;
            let empty = page.is_empty();
            issues.extend(page);
            if empty || issues.len() >= total {
                return Ok(issues);
            }
        }
        Err(JiraError::Shape("the search never ends".into()))
    }

    /// Reads the comments the issue's own page left out, when it has more than it showed.
    async fn complete_comments(&self, issue: &mut RawIssue) -> Result<(), JiraError> {
        let total = issue.fields["comment"]["total"].as_u64().unwrap_or(0) as usize;
        for _ in 0..PAGE_LIMIT {
            if issue.comments.len() >= total {
                break;
            }
            let query = [
                ("startAt", issue.comments.len().to_string()),
                ("maxResults", PAGE.to_string()),
            ];
            let path = format!("/rest/api/2/issue/{}/comment", issue.key);
            let answer = self.get(&path, &query).await?;
            let more = answer["comments"].as_array().cloned().unwrap_or_default();
            if more.is_empty() {
                break;
            }
            issue.comments.extend(more);
        }
        Ok(())
    }
}

fn raw_issues(answer: &Value) -> Result<Vec<RawIssue>, JiraError> {
    let issues = answer["issues"]
        .as_array()
        .ok_or_else(|| JiraError::Shape("a search answer without issues".into()))?;
    issues
        .iter()
        .map(|issue| {
            let key = issue["key"]
                .as_str()
                .ok_or_else(|| JiraError::Shape("an issue without a key".into()))?
                .to_string();
            let fields = issue["fields"].clone();
            let comments = fields["comment"]["comments"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            Ok(RawIssue {
                key,
                fields,
                comments,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_address_keeps_its_path_and_refuses_credentials() {
        assert_eq!(
            site_address(" https://acme.atlassian.net/ ").unwrap(),
            "https://acme.atlassian.net"
        );
        assert_eq!(
            site_address("http://127.0.0.1:8080/jira/").unwrap(),
            "http://127.0.0.1:8080/jira"
        );
        for wrong in [
            "acme.atlassian.net",
            "ftp://acme.atlassian.net",
            "https://user:pw@acme.atlassian.net",
            "https://acme.atlassian.net/?x=1",
        ] {
            assert!(site_address(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn a_project_key_cannot_carry_a_query() {
        assert!(is_project_key("WEB"));
        assert!(is_project_key("A_1"));
        assert!(!is_project_key("1A"));
        assert!(!is_project_key("W\" OR 1=1"));
        assert!(!is_project_key(""));
    }

    #[test]
    fn the_token_is_not_in_the_debug_form_of_a_connection() {
        let connection = Connection {
            url: "https://acme.atlassian.net".into(),
            email: None,
            token: "abc-secret-token".into(),
        };
        assert!(!format!("{connection:?}").contains("abc-secret-token"));
    }

    use crate::repos::jira_fake::{self, basic};

    fn connection(address: &str, token: &str) -> Connection {
        Connection {
            url: address.into(),
            email: Some("ada@example.com".into()),
            token: token.into(),
        }
    }

    #[tokio::test]
    async fn a_project_is_read_over_pages_with_the_comments_its_issues_left_out() {
        let (address, site) =
            jira_fake::serve(jira_fake::project(basic("ada@example.com", "tok"))).await;
        let fetched = fetch(&connection(&address, "tok"), "WEB").await.unwrap();
        assert_eq!(fetched.issues.len(), 6);
        let story = fetched.issues.iter().find(|i| i.key == "WEB-2").unwrap();
        assert_eq!(story.comments.len(), 3);
        let asked = site.lock().unwrap().asked.clone();
        assert!(
            asked.iter().any(|a| a.contains("project = \"WEB\"")),
            "{asked:?}"
        );
    }

    #[tokio::test]
    async fn a_site_without_the_search_by_token_is_read_by_position() {
        let mut site = jira_fake::project(basic("ada@example.com", "tok"));
        site.legacy_search = true;
        let (address, _) = jira_fake::serve(site).await;
        let fetched = fetch(&connection(&address, "tok"), "WEB").await.unwrap();
        assert_eq!(fetched.issues.len(), 6);
    }

    #[tokio::test]
    async fn a_personal_access_token_is_sent_as_a_bearer_and_a_wrong_token_is_refused_without_showing_it()
     {
        let mut site = jira_fake::project("Bearer pat-1".into());
        site.expects = "Bearer pat-1".into();
        let (address, _) = jira_fake::serve(site).await;
        let bearer = Connection {
            url: address.clone(),
            email: None,
            token: "pat-1".into(),
        };
        assert_eq!(fetch(&bearer, "WEB").await.unwrap().issues.len(), 6);
        let wrong = Connection {
            token: "pat-secret-2".into(),
            ..bearer
        };
        let error = fetch(&wrong, "WEB").await.unwrap_err();
        assert!(
            matches!(error, JiraError::Status { status: 401, .. }),
            "{error:?}"
        );
        assert!(!error.to_string().contains("pat-secret-2"));
    }
}
