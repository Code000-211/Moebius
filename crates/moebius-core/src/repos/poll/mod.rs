//! The poll of the forges (ADR 0003: no webhooks). Every repository on a forge is brought up to date in turn: its
//! clone fetches `origin`, its issues become and update cards, and the pull or merge requests of its open changes are
//! read. A repository that fails is reported on the bus and tried again at the next poll.
//!
//! What every forge shares is here: the loop over the repositories, the issues handed to the board as
//! [`Issue`](crate::repos::forge::Issue)s (trusted authors and "blocked by #n" are the board's), and what became of each
//! change as a [`Request`]. What a forge's API answers is turned into those by its client's [`Polled`], in `github`
//! and `gitlab`.

mod github;
mod gitlab;

use std::future::Future;
use std::sync::Arc;

use serde_json::json;

use crate::app::App;
use crate::bus::AppEvent;
use crate::repos::forge::{ForgeError, Issue};
use crate::store::Repository;

/// What became of the pull or merge request of a change, as the poll reads it from any forge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Merged, leaving `commit` on the default branch if the forge says which.
    Merged { commit: Option<String> },
    /// Closed without a merge.
    Closed,
    /// Still open, with its checks for its head commit and its latest review.
    Open(Status),
}

/// How an open pull or merge request stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The commit at its head, which the checks are for.
    pub head: String,
    /// `success`, `failure` or `pending`; none when the head has no checks at all.
    pub checks: Option<&'static str>,
    /// The failed checks, one per line: name, how it ended, what it says and where to read more.
    pub failed: Option<String>,
    /// The latest review in lower case, such as `approved`.
    pub review: Option<String>,
}

/// A forge's client as the poll uses it.
pub trait Polled: Send + Sync {
    /// The forge repositories name, such as `github`.
    const FORGE: &'static str;
    /// The issues of the repository `name` on the forge; none when nothing changed since the last call.
    fn issues(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Option<Vec<Issue>>, ForgeError>> + Send;
    /// Forgets what the last call to `issues` saw, so that the next lists the issues again.
    fn forget_issues(&self, name: &str);
    /// What became of the pull or merge request `number` of `name`.
    fn request(
        &self,
        name: &str,
        number: i64,
    ) -> impl Future<Output = Result<Request, ForgeError>> + Send;
}

/// Polls every repository on the forge of `client` once.
pub async fn poll<F: Polled>(app: &Arc<App>, client: &F) {
    let Ok(repositories) = app.store.repositories() else {
        return;
    };
    for repository in repositories {
        if repository.forge != F::FORGE {
            continue;
        }
        let Some(name) = repository.forge_name().map(str::to_string) else {
            continue;
        };
        if let Err(message) = poll_repository(app, client, repository, &name).await {
            app.bus.publish(AppEvent::new(
                "forge_error",
                json!({ "repository": name, "message": message }),
            ));
        }
    }
}

async fn poll_repository<F: Polled>(
    app: &Arc<App>,
    client: &F,
    repository: Repository,
    name: &str,
) -> Result<(), String> {
    let (refresh_app, refresh_repository) = (app.clone(), repository.clone());
    blocking(move || refresh_app.refresh(&refresh_repository)).await?;
    poll_issues(app, client, repository.clone(), name).await?;
    poll_requests(app, client, &repository, name).await
}

/// Runs a blocking call on the core off the async threads.
async fn blocking<T, E>(f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Result<T, String>
where
    T: Send + 'static,
    E: ToString + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Lists the issues of `repository` on the forge as `name` and syncs them into the board, unless nothing changed.
async fn poll_issues<F: Polled>(
    app: &Arc<App>,
    client: &F,
    repository: Repository,
    name: &str,
) -> Result<(), String> {
    let Some(issues) = client.issues(name).await.map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let app = app.clone();
    // On a failure the forge would answer "not modified" next time; forgetting lists the issues again.
    blocking(move || app.sync_issues(&repository, &issues))
        .await
        .inspect_err(|_| client.forget_issues(name))
}

/// Reads the pull or merge request of each open change of `repository` on the forge as `name`: a merged one finishes
/// its change, one closed without a merge closes it, and for any other the checks and the latest review are recorded.
async fn poll_requests<F: Polled>(
    app: &Arc<App>,
    client: &F,
    repository: &Repository,
    name: &str,
) -> Result<(), String> {
    let (store, id) = (app.store.clone(), repository.id);
    let changes = blocking(move || store.open_pull_requests(id)).await?;
    for change in changes {
        let Some(number) = change.pull_request else {
            continue;
        };
        let request = client
            .request(name, number)
            .await
            .map_err(|e| e.to_string())?;
        let app = app.clone();
        match request {
            Request::Merged { commit } => {
                blocking(move || app.change_merged(&change, commit.as_deref())).await?
            }
            Request::Closed => blocking(move || app.change_closed(&change)).await?,
            Request::Open(status) => {
                blocking(move || {
                    app.record_change_status(
                        &change,
                        status.checks,
                        &status.head,
                        status.failed.as_deref(),
                        status.review.as_deref(),
                    )
                })
                .await?
            }
        }
    }
    Ok(())
}
