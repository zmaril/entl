//! The `World` model — an abstract git history (+ a forge state, added later) that the generators
//! produce and the round-trip properties check. Kept deliberately close to git's own object model
//! so materialization is a direct translation.

use std::collections::BTreeMap;

/// A file mode git records for a blob entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Regular file, `100644`.
    Normal,
    /// Executable file, `100755`.
    Exec,
    /// Symlink, `120000` (content is the link target).
    Symlink,
}

impl Mode {
    pub fn octal(self) -> &'static str {
        match self {
            Mode::Normal => "100644",
            Mode::Exec => "100755",
            Mode::Symlink => "120000",
        }
    }
}

/// A blob at a path within a commit's full tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenBlob {
    pub content: Vec<u8>,
    pub mode: Mode,
}

/// A git signature (author or committer). `time_secs` + `tz` reproduce git's raw time format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenSig {
    pub name: String,
    pub email: String,
    pub time_secs: i64,
    /// Offset string like `"+0000"` / `"-0730"`.
    pub tz: String,
}

/// One commit: parents by index (all `< self` — topological), its *full* tree, and metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenCommit {
    pub parents: Vec<usize>,
    pub tree: BTreeMap<String, GenBlob>,
    pub author: GenSig,
    pub committer: GenSig,
    pub message: String,
}

/// A ref pointing at a commit index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenRef {
    /// Full ref name, e.g. `refs/heads/main` or `refs/tags/v1`.
    pub name: String,
    pub target: usize,
}

/// A generated git history: commits in topological order + refs into them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitWorld {
    pub commits: Vec<GenCommit>,
    pub refs: Vec<GenRef>,
}

/// A whole generated world. (Forge added in a later phase.)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct World {
    pub git: GitWorld,
}
