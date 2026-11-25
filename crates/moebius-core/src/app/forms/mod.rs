//! Templates for what is written on a forge (ADR 0034): the pull request template whose fixed headings are read into
//! fields (`pull`), and the comment templates every kind of comment Moebius reads or writes follows (`comments`). The
//! defaults ship with Moebius; a project replaces them in the harness configuration repository, under
//! `projects/<project>/templates/`, and Moebius keeps the repository's own copies current in `.github/` (`sync`).

pub mod comments;
mod keep;
mod own;
pub mod pull;
mod read;

pub use keep::{
    COMMENT_DIR, ISSUE_FORM_PATH, PULL_REQUEST_FILE, PULL_REQUEST_PATH, SYNC_BRANCH,
    TemplateOverview,
};

pub use comments::{Kind, Reply};
pub use own::{OwnReview, Posted};
pub use pull::{PullTemplate, slug};
