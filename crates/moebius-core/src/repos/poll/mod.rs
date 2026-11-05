//! The poll of the forges (ADR 0003, with webhooks to wake it: ADR 0012). Every repository on a forge is brought up to date in turn: its
//! clone fetches `origin`, its issues become and update cards, and the pull or merge requests of its open changes are
//! read. A repository that fails is reported on the bus and tried again at the next poll.
//!
//! What every forge shares is here: the loop over the repositories, the issues handed to the board as
//! [`Issue`](crate::repos::forge::Issue)s (trusted authors and "blocked by #n" are the board's), and what became of each
//! change as a [`Request`]. What a forge's API answers is turned into those by its client's [`Polled`], in `github`,
//! `gitlab` and `forgejo`.

mod forgejo;
mod github;
mod gitlab;

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use crate::app::App;
use crate::bus::Event;
use crate::repos::forge::{Comment, ForgeError, Issue, Remembered};
use crate::store::{Change, Repository};

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
    /// The issues of the repository `name` on the forge; none when nothing changed since the last call. Once a
    /// previous answer is known, a forge may list only the issues updated since its newest one, not all of them.
    fn issues(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Option<Vec<Issue>>, ForgeError>> + Send;
    /// The comments of the issue `number` of `name`, oldest first.
    fn comments(
        &self,
        name: &str,
        number: i64,
    ) -> impl Future<Output = Result<Vec<Comment>, ForgeError>> + Send;
    /// What this client remembers of the blockers it read (see [`Polled::blockers`]), which the poll prunes and, when
    /// the forge says nothing changed, reads again once old.
    fn remembered(&self) -> &Remembered;
    /// The numbers of the issues that block the issue `number` of `name`, which `updated` is the time of the last change
    /// of (see [`Issue::updated`]); none where the forge cannot say. Called only for an open issue that has, or is
    /// about to get, a card and whose [`Issue::blockers_unread`] is set, and an answer is kept for as long as the
    /// issue does not change.
    fn blockers(
        &self,
        name: &str,
        number: i64,
        updated: &str,
    ) -> impl Future<Output = Option<Vec<i64>>> + Send;
    /// Forgets what the last call to `issues` saw, so that the next lists the issues again.
    fn forget_issues(&self, name: &str);
    /// What became of the pull or merge request `number` of `name`.
    fn request(
        &self,
        name: &str,
        number: i64,
    ) -> impl Future<Output = Result<Request, ForgeError>> + Send;
}

/// How many answers about blockers a poll reads again though the forge listed no issue: they are old, and a forge need
/// not say that a link changed. The oldest go first, so that every one is read in turn and not all at once.
const BLOCKER_REREADS: usize = 5;

/// Polls every repository on the forge of `client` once.
pub async fn poll<F: Polled>(app: &Arc<App>, client: &F) {
    poll_only(app, client, None).await
}

/// Polls the repositories on the forge of `client` that are called one of `only` on it, or all of them without `only`.
pub async fn poll_only<F: Polled>(app: &Arc<App>, client: &F, only: Option<&[String]>) {
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
        if only.is_some_and(|only| !only.contains(&name)) {
            continue;
        }
        if let Err(message) = poll_repository(app, client, repository, &name).await {
            app.bus.publish(Event::ForgeError {
                repository: name.to_string(),
                message,
            });
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

/// Lists the issues of `repository` on the forge as `name` and syncs them into the board, unless nothing changed; the
/// blockers that were read for the issues are read again from time to time even then.
async fn poll_issues<F: Polled>(
    app: &Arc<App>,
    client: &F,
    repository: Repository,
    name: &str,
) -> Result<(), String> {
    let read_at = crate::app::now_millis();
    if let Some(issues) = client.issues(name).await.map_err(|e| e.to_string())? {
        sync_listed(app, client, &repository, name, issues, read_at).await?;
    }
    reread_blockers(app, client, &repository, name).await
}

/// Reads what is read besides the issues, and syncs them. That is read only for an issue that is synced: one a person
/// did not delete, and one that has a card or is open and gets one. Its comments are read when the forge says it has
/// other than Moebius has stored (and other than it said when they were last read, which is how a comment deleted on the
/// forge is not read for ever), or when the issue changed since they were read and it has any (a comment deleted and
/// another written between two polls leave the count as it was, and the forge says no more of them in its list); its
/// blockers, when it is open and the forge keeps them apart from the issue.
async fn sync_listed<F: Polled>(
    app: &Arc<App>,
    client: &F,
    repository: &Repository,
    name: &str,
    mut issues: Vec<Issue>,
    read_at: i64,
) -> Result<(), String> {
    let (store, id) = (app.store.clone(), repository.id);
    let counted: Vec<(i64, i64, bool, String)> = issues
        .iter()
        .map(|i| (i.number, i.comment_count, i.open, i.updated.clone()))
        .collect();
    let synced = blocking(
        move || -> Result<BTreeMap<i64, bool>, crate::store::StoreError> {
            let mut synced = BTreeMap::new();
            for (number, count, open, updated) in counted {
                if store.card_was_deleted(id, number)? {
                    continue;
                }
                let card = store.card_by_number(id, number)?;
                if card.is_none() && !open {
                    continue;
                }
                let stored = match card {
                    Some(card) => store.forge_comment_count(card.id)?,
                    None => 0,
                };
                let seen = store.forge_seen(id, number, "comments")?;
                let seen_at = store.forge_seen(id, number, "comments_at")?;
                let changed = (stored != count && seen.as_deref() != Some(&*count.to_string()))
                    || (count > 0 && seen_at.is_some_and(|at| at != updated));
                synced.insert(number, changed);
            }
            Ok(synced)
        },
    )
    .await?;
    for issue in issues.iter_mut() {
        let Some(&changed) = synced.get(&issue.number) else {
            continue;
        };
        if changed {
            match client.comments(name, issue.number).await {
                Ok(comments) => issue.comments = Some(comments),
                // The issue is still synced; its comments are read again at the next poll.
                Err(e) => {
                    app.bus.publish(Event::ForgeError {
                        repository: name.to_string(),
                        message: format!("#{}: {e}", issue.number),
                    });
                }
            }
        }
        if issue.open && issue.blockers_unread {
            issue.blocked_by = client.blockers(name, issue.number, &issue.updated).await;
        }
    }
    let (app, repository) = (app.clone(), repository.clone());
    // On a failure the forge would answer "not modified" next time; forgetting lists the issues again.
    blocking(move || app.sync_issues_read_at(&repository, &issues, read_at))
        .await
        .inspect_err(|_| client.forget_issues(name))
}

/// Reads the blockers of the open issues again that were read long ago, a few each poll, whether or not the forge listed
/// any issue: it need not change an issue's time when a link changes, and it says "not modified" while nothing else
/// does. What is remembered of an issue that is closed or has no card any more is forgotten.
async fn reread_blockers<F: Polled>(
    app: &Arc<App>,
    client: &F,
    repository: &Repository,
    name: &str,
) -> Result<(), String> {
    let id = repository.id;
    let open = {
        let app = app.clone();
        blocking(move || app.open_issue_numbers(id)).await?
    };
    let remembered = client.remembered();
    remembered.retain(name, |number| open.contains(&number));
    for (number, stamp) in remembered.due(name, BLOCKER_REREADS) {
        let read_at = crate::app::now_millis();
        // The answer is old, so this reads it again.
        let Some(kept) = client.blockers(name, number, &stamp).await else {
            continue;
        };
        let app = app.clone();
        blocking(move || app.reread_blockers(id, number, read_at, &kept)).await?;
    }
    Ok(())
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
    let mut failures = Vec::new();
    for change in changes {
        let Some(number) = change.pull_request else {
            continue;
        };
        // One change that cannot be read (a deleted merge request answers 404) does not stop the others.
        if let Err(message) = poll_request(app, client, name, change, number).await {
            failures.push(format!("#{number}: {message}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// Reads the pull or merge request `number` of `change` and records what became of it.
async fn poll_request<F: Polled>(
    app: &Arc<App>,
    client: &F,
    name: &str,
    change: Change,
    number: i64,
) -> Result<(), String> {
    let request = client
        .request(name, number)
        .await
        .map_err(|e| e.to_string())?;
    let app = app.clone();
    match request {
        Request::Merged { commit } => {
            blocking(move || app.change_merged(&change, commit.as_deref())).await
        }
        Request::Closed => blocking(move || app.change_closed(&change)).await,
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
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::app::columns;
    use crate::store::Store;

    /// A forge client that lists what it is told to and counts what it is asked to read.
    struct Fake {
        remembered: Remembered,
        issues: Mutex<Option<Vec<Issue>>>,
        comments: Mutex<Vec<Comment>>,
        blockers: Mutex<BTreeMap<i64, Vec<i64>>>,
        comment_reads: Mutex<Vec<i64>>,
        blocker_reads: Mutex<Vec<i64>>,
    }

    impl Fake {
        fn new(lifetime: Duration) -> Self {
            Self {
                remembered: Remembered::lasting(lifetime),
                issues: Mutex::default(),
                comments: Mutex::default(),
                blockers: Mutex::default(),
                comment_reads: Mutex::default(),
                blocker_reads: Mutex::default(),
            }
        }
    }

    impl Polled for Fake {
        const FORGE: &'static str = "github";

        async fn issues(&self, _: &str) -> Result<Option<Vec<Issue>>, ForgeError> {
            Ok(self.issues.lock().unwrap().take())
        }

        async fn comments(&self, _: &str, number: i64) -> Result<Vec<Comment>, ForgeError> {
            self.comment_reads.lock().unwrap().push(number);
            Ok(self.comments.lock().unwrap().clone())
        }

        fn remembered(&self) -> &Remembered {
            &self.remembered
        }

        async fn blockers(&self, name: &str, number: i64, updated: &str) -> Option<Vec<i64>> {
            if let Some(known) = self.remembered.get(name, number, updated) {
                return Some(known);
            }
            self.blocker_reads.lock().unwrap().push(number);
            let blockers = self
                .blockers
                .lock()
                .unwrap()
                .get(&number)
                .cloned()
                .unwrap_or_default();
            self.remembered.put(name, number, updated, blockers.clone());
            Some(blockers)
        }

        fn forget_issues(&self, _: &str) {}

        async fn request(&self, _: &str, _: i64) -> Result<Request, ForgeError> {
            Err(ForgeError::Invalid("no requests here".into()))
        }
    }

    fn setup() -> (tempfile::TempDir, Arc<App>, Repository) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = Arc::new(App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(1))),
            &dir.path().join("data"),
        ));
        let repository = app
            .store
            .add_github_repository("w", "/w", "main", "acme/w", &["person".into()])
            .unwrap();
        (dir, app, repository)
    }

    fn open_card(app: &App, repository: &Repository, number: i64) -> crate::store::Card {
        app.store
            .add_numbered_card(
                repository.id,
                number,
                "Card",
                "",
                "person",
                columns::BACKLOG,
            )
            .unwrap()
    }

    #[tokio::test]
    async fn blockers_are_read_again_when_old_though_the_forge_lists_nothing_a_few_at_a_time() {
        let (_dir, app, repository) = setup();
        for number in 1..=7 {
            open_card(&app, &repository, number);
        }
        // An issue that is closed, and one with no card: nothing is kept of them.
        let closed = open_card(&app, &repository, 8);
        app.store.set_issue_closed(closed.id, true).unwrap();
        app.move_card(closed.id, columns::DONE).unwrap();
        let fake = Fake::new(Duration::ZERO);
        for number in 1..=9 {
            fake.remembered.put("acme/w", number, "t1", Vec::new());
        }
        fake.blockers.lock().unwrap().insert(1, vec![2]);

        poll_issues(&app, &fake, repository.clone(), "acme/w")
            .await
            .unwrap();
        // The forge said "not modified", and the five oldest answers were read again.
        assert_eq!(*fake.blocker_reads.lock().unwrap(), [1, 2, 3, 4, 5]);
        let one = app.store.card_by_number(repository.id, 1).unwrap().unwrap();
        assert_eq!(app.store.dependencies(one.id).unwrap(), [2]);
        assert!(fake.remembered.get("acme/w", 8, "t1").is_none());
        assert_eq!(
            fake.remembered
                .due("acme/w", 100)
                .into_iter()
                .map(|(number, _)| number)
                .collect::<Vec<_>>(),
            [6, 7, 1, 2, 3, 4, 5],
            "what is gone is forgotten, and the rest is read in turn"
        );

        poll_issues(&app, &fake, repository, "acme/w")
            .await
            .unwrap();
        assert_eq!(
            *fake.blocker_reads.lock().unwrap(),
            [1, 2, 3, 4, 5, 6, 7, 1, 2, 3]
        );
    }

    #[tokio::test]
    async fn a_comment_deleted_and_another_written_between_two_polls_is_read() {
        let (_dir, app, repository) = setup();
        let fake = Fake::new(Duration::from_secs(600));
        let comment = |id: i64| Comment {
            id,
            author: "leon".into(),
            body: format!("Comment {id}"),
            at: 1_700_000_000 + id,
        };
        let issue = |updated: &str| Issue {
            number: 1,
            title: "T".into(),
            open: true,
            author: "person".into(),
            comment_count: 1,
            updated: updated.into(),
            blocked_by: Some(Vec::new()),
            ..Issue::default()
        };
        let poll = |updated: &'static str| {
            *fake.issues.lock().unwrap() = Some(vec![issue(updated)]);
            poll_issues(&app, &fake, repository.clone(), "acme/w")
        };

        *fake.comments.lock().unwrap() = vec![comment(10)];
        poll("t1").await.unwrap();
        assert_eq!(*fake.comment_reads.lock().unwrap(), [1]);

        // Nothing changed: the comments are not read again.
        poll("t1").await.unwrap();
        assert_eq!(*fake.comment_reads.lock().unwrap(), [1]);

        // The comment was deleted and another written: the count is the same, the issue's time is not.
        *fake.comments.lock().unwrap() = vec![comment(11)];
        poll("t2").await.unwrap();
        assert_eq!(*fake.comment_reads.lock().unwrap(), [1, 1]);
        let card = app.store.card_by_number(repository.id, 1).unwrap().unwrap();
        let bodies: Vec<String> = app
            .comments(card.id)
            .unwrap()
            .into_iter()
            .map(|c| c.body)
            .collect();
        assert_eq!(bodies, ["Comment 10", "Comment 11"]);
        poll("t2").await.unwrap();
        assert_eq!(*fake.comment_reads.lock().unwrap(), [1, 1]);
    }
}
