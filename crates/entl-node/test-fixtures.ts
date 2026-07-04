// Shared test fixture: a self-contained two-commit repo (CI has no ~/projects
// to point at). Author is `Tester <t@e.com>`.

import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

export function fixtureRepo(): string {
  const repo = mkdtempSync(join(tmpdir(), "entl-test-repo-"));
  const git = (...args: string[]) => execFileSync("git", ["-C", repo, ...args], { stdio: "ignore" });
  execFileSync("git", ["init", "-q", repo], { stdio: "ignore" });
  git("config", "user.email", "t@e.com");
  git("config", "user.name", "Tester");
  writeFileSync(join(repo, "a.txt"), "hello\n");
  git("add", "-A");
  git("commit", "-qm", "first");
  writeFileSync(join(repo, "b.txt"), "world\n");
  git("add", "-A");
  git("commit", "-qm", "second");
  return repo;
}
