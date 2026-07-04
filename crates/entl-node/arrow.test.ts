// The Arrow handoff, proven end-to-end in JS: `changes()` batches carry real Arrow
// (decoded with apache-arrow's tableFromIPC), row counts agree with the JSON
// `query()` plane, and `queryArrow()` decodes to a typed Table. Native-type
// semantics: oids arrive as Binary (bytes), not the hex text of query()/extract().

import { test, expect } from "bun:test";
import { tableFromIPC } from "apache-arrow";
import { Entl } from "./index.js";
import { fixtureRepo } from "./test-fixtures.ts";

test("changes() batches decode with tableFromIPC and agree with the JSON plane", async () => {
  const repo = fixtureRepo();
  const entl = new Entl(":memory:");

  const rowsByTable = new Map<string, number>();
  const stream = entl.changes(repo, { github: false });
  let sawCommitColumns: string[] = [];
  for (let batch = await stream.next(); batch !== null; batch = await stream.next()) {
    const table = tableFromIPC(batch.ipc);
    rowsByTable.set(batch.table, (rowsByTable.get(batch.table) ?? 0) + table.numRows);
    expect(["insert", "update", "upsert", "delete", "replace"]).toContain(batch.op);
    if (batch.table === "commits") {
      sawCommitColumns = table.schema.fields.map((f) => f.name);
    }
  }

  expect(rowsByTable.get("commits")).toBe(2);
  expect(sawCommitColumns).toContain("oid");
  expect(sawCommitColumns).toContain("author_name");

  // The stream landed in the DB too — the JSON plane must agree on the counts.
  const viaJson = JSON.parse(await entl.query("SELECT count(*)::int AS n FROM commits"));
  expect(viaJson[0].n).toBe(2);
});

test("queryArrow returns one IPC stream the JS Arrow lib decodes, native types included", async () => {
  const repo = fixtureRepo();
  const entl = new Entl(":memory:");
  await entl.loadGit(repo);

  const t = tableFromIPC(await entl.queryArrow("SELECT oid, author_name, is_merge FROM commits"));
  expect(t.numRows).toBe(2);
  expect(t.schema.fields.map((f) => f.name)).toEqual(["oid", "author_name", "is_merge"]);
  // Native Arrow semantics: the oid column is Binary (bytes), not hex text.
  const oid = t.getChild("oid")!.get(0) as Uint8Array;
  expect(oid.byteLength).toBe(20);
  expect(t.getChild("author_name")!.get(0)).toBe("Tester");

  // Zero rows still decodes (schema-only stream).
  const empty = tableFromIPC(await entl.queryArrow("SELECT 1 AS x WHERE false"));
  expect(empty.numRows).toBe(0);
  expect(empty.schema.fields[0].name).toBe("x");
});
