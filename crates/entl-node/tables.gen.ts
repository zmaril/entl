// AUTO-GENERATED from the fluessig catalog (crates/fluessig/entl.tsp). Do not edit by hand.
// Regenerate: the fluessig-gen command in crates/fluessig/plan.txt (or `bun run gen` in crates/entl-node).
// straitjacket-allow-file:duplication — generated code repeats by design.

/** The entl tables, as a typed enum. Pass `EntlTables.ghPullRequests` to syncInto. */
export const EntlTables = {
    blobs: "blobs",
    commitParents: "commit_parents",
    commits: "commits",
    conflicts: "conflicts",
    fileChanges: "file_changes",
    ghAssignees: "gh_assignees",
    ghCheckRuns: "gh_check_runs",
    ghComments: "gh_comments",
    ghCommitStatuses: "gh_commit_statuses",
    ghEvents: "gh_events",
    ghIssues: "gh_issues",
    ghJobs: "gh_jobs",
    ghLabeled: "gh_labeled",
    ghLabels: "gh_labels",
    ghPrCommits: "gh_pr_commits",
    ghPrReviews: "gh_pr_reviews",
    ghPullRequests: "gh_pull_requests",
    ghRequestedReviewers: "gh_requested_reviewers",
    ghReviewComments: "gh_review_comments",
    ghSteps: "gh_steps",
    ghUsers: "gh_users",
    ghWorkflowRuns: "gh_workflow_runs",
    ghWorkflows: "gh_workflows",
    gitNotes: "git_notes",
    refs: "refs",
    repos: "repos",
    treeEntries: "tree_entries",
    trees: "trees",
} as const;

export type EntlTable = (typeof EntlTables)[keyof typeof EntlTables];

/** Every entl table name (the values of EntlTables). */
export const ENTL_TABLES = Object.values(EntlTables) as EntlTable[];
