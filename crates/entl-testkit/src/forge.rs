//! The `ForgeWorld` model — an abstract GitHub state (PRs/issues/comments/reviews/labels/users/
//! events) that references the git world's commits by index. The mock server serves it as GitHub
//! API responses, and the generators build it. Commit references are indices into the git world's
//! commit list, resolved to real OIDs after materialize (see [`crate::mock`]).

/// A GitHub user/actor. `typ` is `User` | `Bot` | `Organization`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhUser {
    pub id: i64,
    pub login: String,
    pub typ: String,
}

/// A label definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhLabel {
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// A PR review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhReview {
    pub id: i64,
    pub state: Option<String>,
    pub submitted_at: Option<String>,
    pub body: Option<String>,
    pub author: Option<usize>,
}

/// An issue/PR comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhComment {
    pub id: i64,
    pub body: Option<String>,
    pub created_at: Option<String>,
    pub author: Option<usize>,
}

/// A PR review-thread comment (has a diff `side` + optional `commit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhReviewComment {
    pub id: i64,
    pub path: Option<String>,
    pub line: Option<i64>,
    pub side: Option<String>,
    pub commit: Option<usize>,
    pub body: Option<String>,
    pub created_at: Option<String>,
    pub reply_to: Option<i64>,
    pub author: Option<usize>,
}

/// A pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhPull {
    pub number: i64,
    pub title: Option<String>,
    pub body: Option<String>,
    /// `OPEN` | `CLOSED` | `MERGED`.
    pub state: String,
    pub is_draft: bool,
    pub mergeable: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub merged_at: Option<String>,
    pub additions: Option<i64>,
    pub deletions: Option<i64>,
    pub changed_files: Option<i64>,
    pub head_ref: Option<String>,
    pub base_ref: Option<String>,
    pub head_commit: Option<usize>,
    pub base_commit: Option<usize>,
    pub merge_commit: Option<usize>,
    pub author: Option<usize>,
    /// The head commit's CI rollup state (SUCCESS/FAILURE/…), if any.
    pub rollup: Option<String>,
    pub labels: Vec<usize>,
    pub commits: Vec<usize>,
    pub reviews: Vec<GhReview>,
    pub requested_reviewers: Vec<usize>,
    pub comments: Vec<GhComment>,
    pub review_comments: Vec<GhReviewComment>,
}

/// An issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhIssue {
    pub number: i64,
    pub title: Option<String>,
    pub body: Option<String>,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub author: Option<usize>,
    pub labels: Vec<usize>,
    pub comments: Vec<GhComment>,
}

/// An activity-feed event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhEvent {
    pub id: String,
    pub typ: Option<String>,
    pub actor: Option<usize>,
    pub created_at: Option<String>,
    pub payload: serde_json::Value,
}

/// A generated GitHub state. `users`/`labels` are pools indexed by the resources above; commit
/// references are indices into the git world's commits (resolved to OIDs by the mock).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForgeWorld {
    pub owner: String,
    pub name: String,
    pub users: Vec<GhUser>,
    pub labels: Vec<GhLabel>,
    pub pulls: Vec<GhPull>,
    pub issues: Vec<GhIssue>,
    pub events: Vec<GhEvent>,
}
