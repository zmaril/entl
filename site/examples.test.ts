// Executes every runnable code block in the cookbook (and any other doc) against a throwaway repo,
// so the docs can't drift from the real API. `sh`/`bash` blocks run the built `entl` binary; `js`/
// `ts` blocks run under bun with `@entl/node` rewritten to the local build; `python` blocks run
// under the entl-python venv. Skips cleanly if a prerequisite isn't built.
//
//   cd site && bun test examples.test.ts

import { test, expect } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";

const SITE = import.meta.dir;
const ROOT = join(SITE, "..");
const NODE_INDEX = join(ROOT, "crates/entl-node/index.js");
const PY = join(ROOT, "crates/entl-python/.venv/bin/python");
const ENTL_BIN = ["target/release/entl", "target/debug/entl"]
  .map((p) => join(ROOT, p))
  .find(existsSync);

const DOCS = [join(SITE, "content/docs/guides/cookbook.mdx")];
const RUNNABLE = new Set(["sh", "bash", "js", "ts", "python", "py"]);

/** Extract fenced code blocks, dedenting by the opening fence's indentation (blocks live in JSX). */
function blocks(md: string): { lang: string; code: string }[] {
  const lines = md.split("\n");
  const out: { lang: string; code: string }[] = [];
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(/^(\s*)```(\w+)\s*$/);
    if (!m) continue;
    const [, indent, lang] = m;
    const code: string[] = [];
    let j = i + 1;
    for (; j < lines.length && !/^\s*```\s*$/.test(lines[j]); j++) {
      code.push(lines[j].startsWith(indent) ? lines[j].slice(indent.length) : lines[j]);
    }
    out.push({ lang, code: code.join("\n") });
    i = j;
  }
  return out;
}

function run(cmd: string, args: string[], cwd: string, extraPath?: string) {
  const env = { ...process.env };
  if (extraPath) env.PATH = `${extraPath}:${env.PATH}`;
  return spawnSync(cmd, args, { cwd, env, encoding: "utf8" });
}

const ready = ENTL_BIN && existsSync(NODE_INDEX) && existsSync(PY);
const t = ready ? test : test.skip;

t("cookbook examples all run", () => {
  // A throwaway git repo the recipes' `./repo` resolves to.
  const T = mkdtempSync(join(tmpdir(), "entl-ex-"));
  const setup = run(
    "bash",
    ["-c", "git init -q repo && cd repo && git config user.email t@e.com && git config user.name Tester && printf 'hello\\n' > a.txt && git add -A && git commit -qm first"],
    T,
  );
  expect(setup.status, setup.stderr).toBe(0);

  const failures: string[] = [];
  for (const doc of DOCS) {
    for (const b of blocks(readFileSync(doc, "utf8"))) {
      if (!RUNNABLE.has(b.lang)) continue;
      let r;
      if (b.lang === "sh" || b.lang === "bash") {
        r = run("bash", ["-eu", "-c", b.code], T, dirname(ENTL_BIN!));
      } else if (b.lang === "js" || b.lang === "ts") {
        const file = join(T, "block.mjs");
        // Blocks run from a tmpdir, so bare imports get rewritten to local
        // resolutions: the built addon, and site's own node_modules for
        // apache-arrow (the Arrow-consuming recipes).
        const code = b.code
          .replaceAll('"@entl/node"', JSON.stringify(NODE_INDEX))
          .replaceAll('"apache-arrow"', JSON.stringify(join(SITE, "node_modules/apache-arrow/Arrow.mjs")));
        writeFileSync(file, code);
        r = run("bun", [file], T);
      } else {
        const file = join(T, "block.py");
        writeFileSync(file, b.code);
        r = run(PY, [file], T);
      }
      if (r.status !== 0) {
        failures.push(`[${b.lang}] exit ${r.status}\n${b.code}\n--- stderr ---\n${r.stderr}`);
      }
    }
  }
  expect(failures.join("\n\n"), `\n${failures.join("\n\n")}`).toHaveLength(0);
});
