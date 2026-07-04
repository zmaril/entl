// AUTO-GENERATED from the fluessig catalog (crates/fluessig/entl.tsp). Do not edit by hand.
// Regenerate: the fluessig-gen command in crates/fluessig/plan.txt (or `bun run gen` in crates/entl-node).

import {bigint, boolean, integer, pgSchema, primaryKey, text, timestamp} from "drizzle-orm/pg-core";

/** The Postgres schema entl's tables are mirrored into by `syncInto`. */
export const entl = pgSchema("entl");

export const blobs = entl.table("blobs", {
    oid: text("oid").notNull(),
    repoId: text("repo_id").notNull(),
    size: bigint("size", { mode: "number" }).notNull(),
    isBinary: boolean("is_binary").notNull(),
    contentText: text("content_text"),
    contentSha: text("content_sha"),
    content: text("content"),
},
    (t) => [primaryKey({ columns: [t.oid] })]);

export const commitParents = entl.table("commit_parents", {
    commitOid: text("commit_oid").notNull(),
    idx: integer("idx").notNull(),
    parentOid: text("parent_oid").notNull(),
},
    (t) => [primaryKey({ columns: [t.commitOid, t.idx] })]);

export const commits = entl.table("commits", {
    oid: text("oid").notNull(),
    repoId: text("repo_id").notNull(),
    treeOid: text("tree_oid").notNull(),
    message: text("message").notNull(),
    summary: text("summary").notNull(),
    authorName: text("author_name"),
    authorEmail: text("author_email"),
    authorWhen: timestamp("author_when", { withTimezone: true }),
    authorTz: text("author_tz"),
    committerName: text("committer_name"),
    committerEmail: text("committer_email"),
    committerWhen: timestamp("committer_when", { withTimezone: true }),
    committerTz: text("committer_tz"),
    parentCount: integer("parent_count").notNull(),
    isMerge: boolean("is_merge").notNull(),
    gpgSigned: boolean("gpg_signed").notNull(),
},
    (t) => [primaryKey({ columns: [t.oid] })]);

export const conflicts = entl.table("conflicts", {
    repoId: text("repo_id").notNull(),
    mergeOid: text("merge_oid").notNull(),
    path: text("path").notNull(),
    unresolved: boolean("unresolved").notNull(),
},
    (t) => [primaryKey({ columns: [t.repoId, t.mergeOid, t.path] })]);

export const fileChanges = entl.table("file_changes", {
    commitOid: text("commit_oid").notNull(),
    path: text("path").notNull(),
    oldPath: text("old_path"),
    status: text("status").notNull(),
    additions: integer("additions"),
    deletions: integer("deletions"),
    blobOid: text("blob_oid"),
    oldBlobOid: text("old_blob_oid"),
},
    (t) => [primaryKey({ columns: [t.commitOid, t.path] })]);

export const ghAssignees = entl.table("gh_assignees", {
    repoId: text("repo_id").notNull(),
    subjectType: text("subject_type").notNull(),
    subjectNumber: integer("subject_number").notNull(),
    userId: bigint("user_id", { mode: "number" }).notNull(),
},
    (t) => [primaryKey({ columns: [t.repoId, t.subjectType, t.subjectNumber, t.userId] })]);

export const ghCheckRuns = entl.table("gh_check_runs", {
    id: bigint("id", { mode: "number" }).notNull(),
    repoId: text("repo_id").notNull(),
    commitOid: text("commit_oid"),
    name: text("name"),
    status: text("status"),
    conclusion: text("conclusion"),
    startedAt: timestamp("started_at", { withTimezone: true }),
    completedAt: timestamp("completed_at", { withTimezone: true }),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const ghComments = entl.table("gh_comments", {
    id: bigint("id", { mode: "number" }).notNull(),
    subjectType: text("subject_type").notNull(),
    repoId: text("repo_id").notNull(),
    subjectNumber: integer("subject_number").notNull(),
    authorId: bigint("author_id", { mode: "number" }),
    body: text("body"),
    createdAt: timestamp("created_at", { withTimezone: true }),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const ghCommitStatuses = entl.table("gh_commit_statuses", {
    id: bigint("id", { mode: "number" }).notNull(),
    repoId: text("repo_id").notNull(),
    commitOid: text("commit_oid").notNull(),
    context: text("context"),
    state: text("state"),
    description: text("description"),
    targetUrl: text("target_url"),
    createdAt: timestamp("created_at", { withTimezone: true }),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const ghEvents = entl.table("gh_events", {
    repoId: text("repo_id").notNull(),
    id: text("id").notNull(),
    type: text("type"),
    actorId: bigint("actor_id", { mode: "number" }),
    actorLogin: text("actor_login"),
    createdAt: timestamp("created_at", { withTimezone: true }),
    payload: text("payload"),
},
    (t) => [primaryKey({ columns: [t.repoId, t.id] })]);

export const ghIssues = entl.table("gh_issues", {
    repoId: text("repo_id").notNull(),
    number: integer("number").notNull(),
    title: text("title"),
    body: text("body"),
    state: text("state").notNull(),
    authorId: bigint("author_id", { mode: "number" }),
    createdAt: timestamp("created_at", { withTimezone: true }),
    updatedAt: timestamp("updated_at", { withTimezone: true }),
    closedAt: timestamp("closed_at", { withTimezone: true }),
},
    (t) => [primaryKey({ columns: [t.repoId, t.number] })]);

export const ghJobs = entl.table("gh_jobs", {
    id: bigint("id", { mode: "number" }).notNull(),
    runId: bigint("run_id", { mode: "number" }).notNull(),
    name: text("name"),
    status: text("status"),
    conclusion: text("conclusion"),
    startedAt: timestamp("started_at", { withTimezone: true }),
    completedAt: timestamp("completed_at", { withTimezone: true }),
    runnerName: text("runner_name"),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const ghLabeled = entl.table("gh_labeled", {
    repoId: text("repo_id").notNull(),
    subjectType: text("subject_type").notNull(),
    subjectNumber: integer("subject_number").notNull(),
    labelName: text("label_name").notNull(),
},
    (t) => [primaryKey({ columns: [t.repoId, t.subjectType, t.subjectNumber, t.labelName] })]);

export const ghLabels = entl.table("gh_labels", {
    repoId: text("repo_id").notNull(),
    name: text("name").notNull(),
    color: text("color"),
    description: text("description"),
},
    (t) => [primaryKey({ columns: [t.repoId, t.name] })]);

export const ghPrCommits = entl.table("gh_pr_commits", {
    repoId: text("repo_id").notNull(),
    prNumber: integer("pr_number").notNull(),
    commitOid: text("commit_oid").notNull(),
},
    (t) => [primaryKey({ columns: [t.repoId, t.prNumber, t.commitOid] })]);

export const ghPrReviews = entl.table("gh_pr_reviews", {
    id: bigint("id", { mode: "number" }).notNull(),
    repoId: text("repo_id").notNull(),
    prNumber: integer("pr_number").notNull(),
    reviewerId: bigint("reviewer_id", { mode: "number" }),
    state: text("state"),
    submittedAt: timestamp("submitted_at", { withTimezone: true }),
    body: text("body"),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const ghPullRequests = entl.table("gh_pull_requests", {
    repoId: text("repo_id").notNull(),
    number: integer("number").notNull(),
    title: text("title"),
    body: text("body"),
    state: text("state").notNull(),
    authorId: bigint("author_id", { mode: "number" }),
    createdAt: timestamp("created_at", { withTimezone: true }),
    updatedAt: timestamp("updated_at", { withTimezone: true }),
    closedAt: timestamp("closed_at", { withTimezone: true }),
    mergedAt: timestamp("merged_at", { withTimezone: true }),
    mergeCommitOid: text("merge_commit_oid"),
    headRef: text("head_ref"),
    baseRef: text("base_ref"),
    additions: integer("additions"),
    deletions: integer("deletions"),
    changedFiles: integer("changed_files"),
    isDraft: boolean("is_draft").notNull(),
    mergeable: text("mergeable"),
    checks: text("checks"),
    headOid: text("head_oid"),
    baseOid: text("base_oid"),
},
    (t) => [primaryKey({ columns: [t.repoId, t.number] })]);

export const ghRequestedReviewers = entl.table("gh_requested_reviewers", {
    repoId: text("repo_id").notNull(),
    prNumber: integer("pr_number").notNull(),
    userId: bigint("user_id", { mode: "number" }).notNull(),
},
    (t) => [primaryKey({ columns: [t.repoId, t.prNumber, t.userId] })]);

export const ghReviewComments = entl.table("gh_review_comments", {
    id: bigint("id", { mode: "number" }).notNull(),
    repoId: text("repo_id").notNull(),
    prNumber: integer("pr_number").notNull(),
    path: text("path"),
    line: integer("line"),
    side: text("side"),
    commitOid: text("commit_oid"),
    authorId: bigint("author_id", { mode: "number" }),
    body: text("body"),
    createdAt: timestamp("created_at", { withTimezone: true }),
    inReplyTo: bigint("in_reply_to", { mode: "number" }),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const ghSteps = entl.table("gh_steps", {
    jobId: bigint("job_id", { mode: "number" }).notNull(),
    number: integer("number").notNull(),
    name: text("name"),
    status: text("status"),
    conclusion: text("conclusion"),
    startedAt: timestamp("started_at", { withTimezone: true }),
    completedAt: timestamp("completed_at", { withTimezone: true }),
},
    (t) => [primaryKey({ columns: [t.jobId, t.number] })]);

export const ghUsers = entl.table("gh_users", {
    id: bigint("id", { mode: "number" }).notNull(),
    login: text("login").notNull(),
    type: text("type"),
    name: text("name"),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const ghWorkflowRuns = entl.table("gh_workflow_runs", {
    id: bigint("id", { mode: "number" }).notNull(),
    repoId: text("repo_id").notNull(),
    workflowId: bigint("workflow_id", { mode: "number" }),
    headOid: text("head_oid"),
    headBranch: text("head_branch"),
    event: text("event"),
    status: text("status"),
    conclusion: text("conclusion"),
    runNumber: integer("run_number"),
    runAttempt: integer("run_attempt"),
    createdAt: timestamp("created_at", { withTimezone: true }),
    updatedAt: timestamp("updated_at", { withTimezone: true }),
    runStartedAt: timestamp("run_started_at", { withTimezone: true }),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const ghWorkflows = entl.table("gh_workflows", {
    id: bigint("id", { mode: "number" }).notNull(),
    repoId: text("repo_id").notNull(),
    name: text("name"),
    path: text("path"),
    state: text("state"),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const refs = entl.table("refs", {
    repoId: text("repo_id").notNull(),
    name: text("name").notNull(),
    kind: text("kind").notNull(),
    targetOid: text("target_oid").notNull(),
    isSymbolic: boolean("is_symbolic").notNull(),
    upstream: text("upstream"),
},
    (t) => [primaryKey({ columns: [t.repoId, t.name] })]);

export const repos = entl.table("repos", {
    id: text("id").notNull(),
    path: text("path").notNull(),
    remoteUrl: text("remote_url"),
    host: text("host"),
    owner: text("owner"),
    name: text("name"),
    defaultBranch: text("default_branch"),
    firstSyncedAt: timestamp("first_synced_at", { withTimezone: true }),
    lastSyncedAt: timestamp("last_synced_at", { withTimezone: true }),
},
    (t) => [primaryKey({ columns: [t.id] })]);

export const treeEntries = entl.table("tree_entries", {
    treeOid: text("tree_oid").notNull(),
    name: text("name").notNull(),
    path: text("path").notNull(),
    mode: text("mode").notNull(),
    entryType: text("entry_type").notNull(),
    childOid: text("child_oid").notNull(),
},
    (t) => [primaryKey({ columns: [t.treeOid, t.name] })]);

export const trees = entl.table("trees", {
    oid: text("oid").notNull(),
    repoId: text("repo_id").notNull(),
},
    (t) => [primaryKey({ columns: [t.oid] })]);
