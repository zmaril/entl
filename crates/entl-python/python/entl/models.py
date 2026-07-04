# AUTO-GENERATED from the fluessig catalog (crates/fluessig/entl.tsp). Do not edit by hand.
# Regenerate: the fluessig-gen command in crates/fluessig/plan.txt (or `bun run gen` in crates/entl-node).
# straitjacket-allow-file:duplication — generated code repeats by design.

from sqlalchemy import Boolean, Column, Integer, String, event
from sqlalchemy.orm import declarative_base

Base = declarative_base()

def _read_only(action):
    "entl owns the schema via its sink; the ORM is a pure read projection."
    def _guard(*_args, **_kw):
        raise RuntimeError(
            "entl.models are read-only: the entl sink owns the schema, so " + action + " is "
            "disallowed here. Create tables by sinking data with entl, not through SQLAlchemy."
        )
    return _guard

# Abort any attempt to emit DDL from these models (create_all / drop_all) before it
# runs, so the models can never author a schema that drifts from what the sink writes.
event.listen(Base.metadata, "before_create", _read_only("create_all()"))
event.listen(Base.metadata, "before_drop", _read_only("drop_all()"))

#: Every table entl writes, by table name.
ENTL_TABLES = [
    "blobs",
    "commit_parents",
    "commits",
    "conflicts",
    "file_changes",
    "gh_assignees",
    "gh_check_runs",
    "gh_comments",
    "gh_commit_statuses",
    "gh_events",
    "gh_issues",
    "gh_jobs",
    "gh_labeled",
    "gh_labels",
    "gh_pr_commits",
    "gh_pr_reviews",
    "gh_pull_requests",
    "gh_requested_reviewers",
    "gh_review_comments",
    "gh_steps",
    "gh_users",
    "gh_workflow_runs",
    "gh_workflows",
    "refs",
    "repos",
    "sync_state",
    "tree_entries",
    "trees",
]

class Blobs(Base):
    __tablename__ = "blobs"
    oid = Column(String, primary_key=True)
    repo_id = Column(String, nullable=False)
    size = Column(Integer, nullable=False)
    is_binary = Column(Boolean, nullable=False)
    content_text = Column(String)
    content_sha = Column(String)
    content = Column(String)

class CommitParents(Base):
    __tablename__ = "commit_parents"
    commit_oid = Column(String, primary_key=True)
    idx = Column(Integer, primary_key=True)
    parent_oid = Column(String, nullable=False)

class Commits(Base):
    __tablename__ = "commits"
    oid = Column(String, primary_key=True)
    repo_id = Column(String, nullable=False)
    tree_oid = Column(String, nullable=False)
    message = Column(String, nullable=False)
    summary = Column(String, nullable=False)
    author_name = Column(String)
    author_email = Column(String)
    author_when = Column(String)
    author_tz = Column(String)
    committer_name = Column(String)
    committer_email = Column(String)
    committer_when = Column(String)
    committer_tz = Column(String)
    parent_count = Column(Integer, nullable=False)
    is_merge = Column(Boolean, nullable=False)
    gpg_signed = Column(Boolean, nullable=False)

class Conflicts(Base):
    __tablename__ = "conflicts"
    repo_id = Column(String, primary_key=True)
    merge_oid = Column(String, primary_key=True)
    path = Column(String, primary_key=True)
    unresolved = Column(Boolean, nullable=False)

class FileChanges(Base):
    __tablename__ = "file_changes"
    commit_oid = Column(String, primary_key=True)
    path = Column(String, primary_key=True)
    old_path = Column(String)
    status = Column(String, nullable=False)
    additions = Column(Integer)
    deletions = Column(Integer)
    blob_oid = Column(String)
    old_blob_oid = Column(String)

class GhAssignees(Base):
    __tablename__ = "gh_assignees"
    repo_id = Column(String, primary_key=True)
    subject_type = Column(String, primary_key=True)
    subject_number = Column(Integer, primary_key=True)
    user_id = Column(Integer, primary_key=True)

class GhCheckRuns(Base):
    __tablename__ = "gh_check_runs"
    id = Column(Integer, primary_key=True)
    repo_id = Column(String, nullable=False)
    commit_oid = Column(String)
    name = Column(String)
    status = Column(String)
    conclusion = Column(String)
    started_at = Column(String)
    completed_at = Column(String)

class GhComments(Base):
    __tablename__ = "gh_comments"
    id = Column(Integer, primary_key=True)
    subject_type = Column(String, nullable=False)
    repo_id = Column(String, nullable=False)
    subject_number = Column(Integer, nullable=False)
    author_id = Column(Integer)
    body = Column(String)
    created_at = Column(String)

class GhCommitStatuses(Base):
    __tablename__ = "gh_commit_statuses"
    id = Column(Integer, primary_key=True)
    repo_id = Column(String, nullable=False)
    commit_oid = Column(String, nullable=False)
    context = Column(String)
    state = Column(String)
    description = Column(String)
    target_url = Column(String)
    created_at = Column(String)

class GhEvents(Base):
    __tablename__ = "gh_events"
    repo_id = Column(String, primary_key=True)
    id = Column(String, primary_key=True)
    type = Column(String)
    actor_id = Column(Integer)
    actor_login = Column(String)
    created_at = Column(String)
    payload = Column(String)

class GhIssues(Base):
    __tablename__ = "gh_issues"
    repo_id = Column(String, primary_key=True)
    number = Column(Integer, primary_key=True)
    title = Column(String)
    body = Column(String)
    state = Column(String, nullable=False)
    author_id = Column(Integer)
    created_at = Column(String)
    updated_at = Column(String)
    closed_at = Column(String)

class GhJobs(Base):
    __tablename__ = "gh_jobs"
    id = Column(Integer, primary_key=True)
    run_id = Column(Integer, nullable=False)
    name = Column(String)
    status = Column(String)
    conclusion = Column(String)
    started_at = Column(String)
    completed_at = Column(String)
    runner_name = Column(String)

class GhLabeled(Base):
    __tablename__ = "gh_labeled"
    repo_id = Column(String, primary_key=True)
    subject_type = Column(String, primary_key=True)
    subject_number = Column(Integer, primary_key=True)
    label_name = Column(String, primary_key=True)

class GhLabels(Base):
    __tablename__ = "gh_labels"
    repo_id = Column(String, primary_key=True)
    name = Column(String, primary_key=True)
    color = Column(String)
    description = Column(String)

class GhPrCommits(Base):
    __tablename__ = "gh_pr_commits"
    repo_id = Column(String, primary_key=True)
    pr_number = Column(Integer, primary_key=True)
    commit_oid = Column(String, primary_key=True)

class GhPrReviews(Base):
    __tablename__ = "gh_pr_reviews"
    id = Column(Integer, primary_key=True)
    repo_id = Column(String, nullable=False)
    pr_number = Column(Integer, nullable=False)
    reviewer_id = Column(Integer)
    state = Column(String)
    submitted_at = Column(String)
    body = Column(String)

class GhPullRequests(Base):
    __tablename__ = "gh_pull_requests"
    repo_id = Column(String, primary_key=True)
    number = Column(Integer, primary_key=True)
    title = Column(String)
    body = Column(String)
    state = Column(String, nullable=False)
    author_id = Column(Integer)
    created_at = Column(String)
    updated_at = Column(String)
    closed_at = Column(String)
    merged_at = Column(String)
    merge_commit_oid = Column(String)
    head_ref = Column(String)
    base_ref = Column(String)
    additions = Column(Integer)
    deletions = Column(Integer)
    changed_files = Column(Integer)
    is_draft = Column(Boolean, nullable=False)
    mergeable = Column(String)
    checks = Column(String)
    head_oid = Column(String)
    base_oid = Column(String)

class GhRequestedReviewers(Base):
    __tablename__ = "gh_requested_reviewers"
    repo_id = Column(String, primary_key=True)
    pr_number = Column(Integer, primary_key=True)
    user_id = Column(Integer, primary_key=True)

class GhReviewComments(Base):
    __tablename__ = "gh_review_comments"
    id = Column(Integer, primary_key=True)
    repo_id = Column(String, nullable=False)
    pr_number = Column(Integer, nullable=False)
    path = Column(String)
    line = Column(Integer)
    side = Column(String)
    commit_oid = Column(String)
    author_id = Column(Integer)
    body = Column(String)
    created_at = Column(String)
    in_reply_to = Column(Integer)

class GhSteps(Base):
    __tablename__ = "gh_steps"
    job_id = Column(Integer, primary_key=True)
    number = Column(Integer, primary_key=True)
    name = Column(String)
    status = Column(String)
    conclusion = Column(String)
    started_at = Column(String)
    completed_at = Column(String)

class GhUsers(Base):
    __tablename__ = "gh_users"
    id = Column(Integer, primary_key=True)
    login = Column(String, nullable=False)
    type = Column(String)
    name = Column(String)

class GhWorkflowRuns(Base):
    __tablename__ = "gh_workflow_runs"
    id = Column(Integer, primary_key=True)
    repo_id = Column(String, nullable=False)
    workflow_id = Column(Integer)
    head_oid = Column(String)
    head_branch = Column(String)
    event = Column(String)
    status = Column(String)
    conclusion = Column(String)
    run_number = Column(Integer)
    run_attempt = Column(Integer)
    created_at = Column(String)
    updated_at = Column(String)
    run_started_at = Column(String)

class GhWorkflows(Base):
    __tablename__ = "gh_workflows"
    id = Column(Integer, primary_key=True)
    repo_id = Column(String, nullable=False)
    name = Column(String)
    path = Column(String)
    state = Column(String)

class Refs(Base):
    __tablename__ = "refs"
    repo_id = Column(String, primary_key=True)
    name = Column(String, primary_key=True)
    kind = Column(String, nullable=False)
    target_oid = Column(String, nullable=False)
    is_symbolic = Column(Boolean, nullable=False)
    upstream = Column(String)

class Repos(Base):
    __tablename__ = "repos"
    id = Column(String, primary_key=True)
    path = Column(String, nullable=False)
    remote_url = Column(String)
    host = Column(String)
    owner = Column(String)
    name = Column(String)
    default_branch = Column(String)
    first_synced_at = Column(String)
    last_synced_at = Column(String)

class SyncState(Base):
    __tablename__ = "sync_state"
    resource = Column(String, primary_key=True)
    cursor = Column(String)
    etag = Column(String)
    watermark = Column(String)
    last_synced_at = Column(String)
    last_error = Column(String)

class TreeEntries(Base):
    __tablename__ = "tree_entries"
    tree_oid = Column(String, primary_key=True)
    name = Column(String, primary_key=True)
    path = Column(String, nullable=False)
    mode = Column(String, nullable=False)
    entry_type = Column(String, nullable=False)
    child_oid = Column(String, nullable=False)

class Trees(Base):
    __tablename__ = "trees"
    oid = Column(String, primary_key=True)
    repo_id = Column(String, nullable=False)
