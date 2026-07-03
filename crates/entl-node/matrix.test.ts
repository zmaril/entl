// Cross-language round-trip matrix (Phase 4, notes/design/testing.md): for each corpus world,
// sink the repo through the Node binding into each store, extract it back, and assert it equals
// the reference snapshot the Rust corpus generator produced (`expected.json`). Set ENTL_CORPUS to
// a `gen_corpus` output directory to run.

import { test, expect } from "bun:test";
import { Entl, SinkTarget } from "./index.js";
import { readdirSync, readFileSync, mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

const corpus = process.env.ENTL_CORPUS;
const t = corpus ? test : test.skip;

t("cross-language P1 matrix (git tables) via Node", async () => {
  for (const name of readdirSync(corpus!).sort()) {
    const dir = join(corpus!, name);
    const repo = join(dir, "repo");
    const expected = readFileSync(join(dir, "expected.json"), "utf8");

    // SQLite
    const spath = join(mkdtempSync(join(tmpdir(), "entl-s-")), "s.db");
    const e1 = new Entl(":memory:");
    await e1.sink(repo, { target: SinkTarget.Sqlite, path: spath, github: false });
    expect(await e1.extract({ source: "sqlite", path: spath })).toBe(expected);

    // JSONL
    const jdir = mkdtempSync(join(tmpdir(), "entl-j-"));
    const e2 = new Entl(":memory:");
    await e2.sink(repo, { target: SinkTarget.Jsonl, path: jdir, github: false });
    expect(await e2.extract({ source: "jsonl", path: jdir })).toBe(expected);

    // Postgres (gated) — a fresh schema per world
    const pg = process.env.ENTL_TEST_PG;
    if (pg) {
      const schema = "m_" + name.replace(/[^a-z0-9_]/gi, "");
      const e3 = new Entl(":memory:");
      await e3.sink(repo, { target: SinkTarget.Postgres, path: pg, github: false, schema });
      expect(await e3.extract({ source: "postgres", path: pg, schema })).toBe(expected);
    }
  }
});
