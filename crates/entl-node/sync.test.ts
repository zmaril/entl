// End-to-end proof of the driver sink: entl loads a git repo into its DuckDB, then `syncInto`
// mirrors it into a *live* PGlite (a WASM Postgres). All the DDL / type mapping / upserts are
// generated in Rust core (`DriverSink`); this test only proves the thin JS executor runs them and
// the rows actually land in a real Postgres-compatible database.

import { test, expect } from "bun:test";
import { PGlite } from "@electric-sql/pglite";
import { Entl } from "./index.js";
import { syncInto, EntlTables } from "./sync.ts";
import { fixtureRepo } from "./test-fixtures.ts";

test("syncInto mirrors entl's DuckDB into a live PGlite via the Rust driver plan", async () => {
  const repo = fixtureRepo();
  const entl = new Entl(":memory:");
  await entl.loadGit(repo);

  const pg = new PGlite(); // in-memory WASM Postgres
  const counts = await syncInto(pg, entl);

  // The plan created the schema + typed tables and upserted rows — read them back from PGlite.
  const commits = await pg.query<{ n: number }>('SELECT count(*)::int AS n FROM "entl"."commits"');
  expect(commits.rows[0].n).toBe(2);
  expect(counts.commits).toBe(2);

  // Types survived the trip: booleans are real booleans, oids are hex text (not bytea).
  const one = await pg.query<{ oid: string; is_merge: boolean; author_name: string }>(
    'SELECT oid, is_merge, author_name FROM "entl"."commits" ORDER BY committer_when LIMIT 1',
  );
  expect(one.rows[0].oid).toMatch(/^[0-9a-f]{40}$/);
  expect(one.rows[0].is_merge).toBe(false);
  expect(one.rows[0].author_name).toBe("Tester");

  // A second sync is idempotent (upsert by PK), not a duplicate.
  await syncInto(pg, entl);
  const again = await pg.query<{ n: number }>('SELECT count(*)::int AS n FROM "entl"."commits"');
  expect(again.rows[0].n).toBe(2);

  await pg.close();
});

test("rename + table selection flow through to the plan", async () => {
  const repo = fixtureRepo();
  const entl = new Entl(":memory:");
  await entl.loadGit(repo);

  const pg = new PGlite();
  const counts = await syncInto(pg, entl, {
    tables: [EntlTables.commits],
    rename: { [EntlTables.commits]: "git_commits" },
    schema: "mirror",
  });

  expect(counts.commits).toBe(2);
  const renamed = await pg.query<{ n: number }>('SELECT count(*)::int AS n FROM "mirror"."git_commits"');
  expect(renamed.rows[0].n).toBe(2);
  await pg.close();
});
