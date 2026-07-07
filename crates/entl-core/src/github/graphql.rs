//! GraphQL query + response types for the PR graph. One query pulls each PR with
//!
//! straitjacket-allow-file:duplication — the DTOs mirror GitHub's wire schema;
//! PR and issue payloads are similar because the API's shapes are.
//! its reviews, commits, review-comments, requested reviewers, and issue comments
//! inline (notes/design/engine.md — this is the batching win over per-PR REST calls).

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Sub-resource page caps. PRs exceeding these are truncated (totalCount lets us
/// detect + report it); full sub-pagination is a later refinement.
#[allow(dead_code)] // documents the cap baked into PR_QUERY; consumed once sub-pagination lands
pub const PRS_PER_PAGE: usize = 25;

pub const PR_QUERY: &str = r#"
query($owner:String!,$name:String!,$cursor:String){
  repository(owner:$owner,name:$name){
    pullRequests(first:25,after:$cursor,orderBy:{field:UPDATED_AT,direction:DESC}){
      pageInfo{hasNextPage endCursor}
      nodes{
        number title body state isDraft mergeable
        createdAt updatedAt closedAt mergedAt
        additions deletions changedFiles
        headRefName baseRefName headRefOid baseRefOid
        author{...a}
        mergeCommit{oid}
        rollup: commits(last:1){nodes{commit{statusCheckRollup{state}}}}
        labels(first:30){nodes{name color description}}
        commits(first:100){totalCount nodes{commit{oid}}}
        reviews(first:50){totalCount nodes{databaseId state submittedAt body author{...a}}}
        reviewRequests(first:50){nodes{requestedReviewer{__typename ...on User{databaseId login}}}}
        comments(first:50){totalCount nodes{databaseId body createdAt author{...a}}}
        reviewThreads(first:40){nodes{diffSide comments(first:20){nodes{
          databaseId path line originalLine commit{oid} body createdAt
          replyTo{databaseId} author{...a}
        }}}}
      }
    }
  }
}
fragment a on Actor{login __typename ...on User{databaseId} ...on Bot{databaseId} ...on Organization{databaseId}}
"#;

#[derive(Deserialize)]
pub struct PrData {
    pub repository: RepoPrs,
}
#[derive(Deserialize)]
pub struct RepoPrs {
    #[serde(rename = "pullRequests")]
    pub pull_requests: PrConnection,
}
#[derive(Deserialize)]
pub struct PrConnection {
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
    pub nodes: Vec<PrNode>,
}
#[derive(Deserialize)]
pub struct PageInfo {
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

/// A GitHub Actor (User/Bot/Organization). `database_id` is the numeric id used
/// as `gh_users.id`; absent for Mannequin/Enterprise actors (then author_id NULL).
#[derive(Deserialize)]
pub struct Actor {
    // Optional: a `requestedReviewer` can be a Team, which has no `login`.
    pub login: Option<String>,
    #[serde(rename = "__typename")]
    pub typename: Option<String>,
    #[serde(rename = "databaseId")]
    pub database_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct Oid {
    pub oid: String,
}

#[derive(Deserialize)]
pub struct PrNode {
    pub number: i64,
    pub title: Option<String>,
    pub body: Option<String>,
    pub state: String,
    #[serde(rename = "isDraft")]
    pub is_draft: bool,
    /// MergeableState: MERGEABLE | CONFLICTING | UNKNOWN.
    pub mergeable: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(rename = "closedAt")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(rename = "mergedAt")]
    pub merged_at: Option<DateTime<Utc>>,
    pub additions: Option<i64>,
    pub deletions: Option<i64>,
    #[serde(rename = "changedFiles")]
    pub changed_files: Option<i64>,
    #[serde(rename = "headRefName")]
    pub head_ref: Option<String>,
    #[serde(rename = "baseRefName")]
    pub base_ref: Option<String>,
    #[serde(rename = "headRefOid")]
    pub head_oid: Option<String>,
    #[serde(rename = "baseRefOid")]
    pub base_oid: Option<String>,
    pub author: Option<Actor>,
    #[serde(rename = "mergeCommit")]
    pub merge_commit: Option<Oid>,
    /// Aliased `commits(last:1)` → the head commit's CI status rollup.
    pub rollup: RollupConn,
    pub labels: LabelConn,
    pub commits: CommitConn,
    pub reviews: ReviewConn,
    #[serde(rename = "reviewRequests")]
    pub review_requests: ReviewRequestConn,
    pub comments: CommentConn,
    #[serde(rename = "reviewThreads")]
    pub review_threads: ThreadConn,
}

#[derive(Deserialize)]
pub struct CommitConn {
    #[serde(rename = "totalCount")]
    pub total_count: i64,
    pub nodes: Vec<CommitWrap>,
}
#[derive(Deserialize)]
pub struct CommitWrap {
    pub commit: Oid,
}

// `rollup: commits(last:1){nodes{commit{statusCheckRollup{state}}}}` — the head
// commit's CI status rollup (StatusState: SUCCESS | FAILURE | PENDING | ERROR | …).
#[derive(Deserialize)]
pub struct RollupConn {
    pub nodes: Vec<RollupNode>,
}
#[derive(Deserialize)]
pub struct RollupNode {
    pub commit: RollupCommit,
}
#[derive(Deserialize)]
pub struct RollupCommit {
    #[serde(rename = "statusCheckRollup")]
    pub status_check_rollup: Option<Rollup>,
}
#[derive(Deserialize)]
pub struct Rollup {
    pub state: Option<String>,
}

impl PrNode {
    /// The head commit's CI rollup state, if any.
    pub fn checks(&self) -> Option<String> {
        self.rollup
            .nodes
            .first()
            .and_then(|n| n.commit.status_check_rollup.as_ref())
            .and_then(|r| r.state.clone())
    }
}

#[derive(Deserialize)]
pub struct ReviewConn {
    #[serde(rename = "totalCount")]
    pub total_count: i64,
    pub nodes: Vec<ReviewNode>,
}
#[derive(Deserialize)]
pub struct ReviewNode {
    #[serde(rename = "databaseId")]
    pub database_id: Option<i64>,
    pub state: Option<String>,
    #[serde(rename = "submittedAt")]
    pub submitted_at: Option<DateTime<Utc>>,
    pub body: Option<String>,
    pub author: Option<Actor>,
}

#[derive(Deserialize)]
pub struct ReviewRequestConn {
    pub nodes: Vec<ReviewRequestNode>,
}
#[derive(Deserialize)]
pub struct ReviewRequestNode {
    #[serde(rename = "requestedReviewer")]
    pub requested_reviewer: Option<Actor>,
}

#[derive(Deserialize)]
pub struct CommentConn {
    // mirrors the wire schema; used to detect + report truncation once sub-pagination lands
    #[serde(rename = "totalCount")]
    #[allow(dead_code)]
    pub total_count: i64,
    pub nodes: Vec<CommentNode>,
}
#[derive(Deserialize)]
pub struct CommentNode {
    #[serde(rename = "databaseId")]
    pub database_id: Option<i64>,
    pub body: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,
    pub author: Option<Actor>,
}

#[derive(Deserialize)]
pub struct ThreadConn {
    pub nodes: Vec<ReviewThread>,
}
#[derive(Deserialize)]
pub struct ReviewThread {
    #[serde(rename = "diffSide")]
    pub side: Option<String>,
    pub comments: ThreadCommentConn,
}
#[derive(Deserialize)]
pub struct ThreadCommentConn {
    pub nodes: Vec<ReviewCommentNode>,
}
#[derive(Deserialize)]
pub struct ReviewCommentNode {
    #[serde(rename = "databaseId")]
    pub database_id: Option<i64>,
    pub path: Option<String>,
    pub line: Option<i64>,
    #[serde(rename = "originalLine")]
    pub original_line: Option<i64>,
    pub commit: Option<Oid>,
    pub body: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(rename = "replyTo")]
    pub reply_to: Option<ReplyTo>,
    pub author: Option<Actor>,
}
#[derive(Deserialize)]
pub struct ReplyTo {
    #[serde(rename = "databaseId")]
    pub database_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct LabelConn {
    pub nodes: Vec<LabelNode>,
}
#[derive(Deserialize)]
pub struct LabelNode {
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
}

// ---- Issues ----

pub const ISSUE_QUERY: &str = r#"
query($owner:String!,$name:String!,$cursor:String){
  repository(owner:$owner,name:$name){
    issues(first:50,after:$cursor,orderBy:{field:UPDATED_AT,direction:DESC}){
      pageInfo{hasNextPage endCursor}
      nodes{
        number title body state
        createdAt updatedAt closedAt
        author{...a}
        labels(first:30){nodes{name color description}}
        comments(first:50){totalCount nodes{databaseId body createdAt author{...a}}}
      }
    }
  }
}
fragment a on Actor{login __typename ...on User{databaseId} ...on Bot{databaseId} ...on Organization{databaseId}}
"#;

#[derive(Deserialize)]
pub struct IssueData {
    pub repository: RepoIssues,
}
#[derive(Deserialize)]
pub struct RepoIssues {
    pub issues: IssueConnection,
}
#[derive(Deserialize)]
pub struct IssueConnection {
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
    pub nodes: Vec<IssueNode>,
}
#[derive(Deserialize)]
pub struct IssueNode {
    pub number: i64,
    pub title: Option<String>,
    pub body: Option<String>,
    pub state: String,
    #[serde(rename = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(rename = "closedAt")]
    pub closed_at: Option<DateTime<Utc>>,
    pub author: Option<Actor>,
    pub labels: LabelConn,
    pub comments: CommentConn,
}
