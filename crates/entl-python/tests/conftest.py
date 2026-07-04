"""Shared helpers for the python-binding tests. pytest picks this up automatically;
the tests import it explicitly (``from conftest import make_repo``) so direct runs
(``python tests/test_sink.py``) keep working too."""

import os
import subprocess
import tempfile


def make_repo(commits=1):
    """A self-contained throwaway repo with ``commits`` small commits (CI has no
    ~/projects to point at). Author is ``Tester <t@e.com>``."""
    d = tempfile.mkdtemp()
    repo = os.path.join(d, "repo")
    subprocess.run(["git", "init", "-q", repo], check=True)
    subprocess.run(["git", "-C", repo, "config", "user.email", "t@e.com"], check=True)
    subprocess.run(["git", "-C", repo, "config", "user.name", "Tester"], check=True)
    for i in range(commits):
        open(os.path.join(repo, f"f{i}.txt"), "w").write(f"hello {i}\n")
        subprocess.run(["git", "-C", repo, "add", "-A"], check=True)
        subprocess.run(["git", "-C", repo, "commit", "-qm", f"commit {i}"], check=True)
    return repo
