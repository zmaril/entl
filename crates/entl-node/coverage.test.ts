// The static coverage guard. `syncInto` is generic over `introspect()`, so it
// mirrors exactly the tables the live schema reports. This test asserts the
// committed `ENTL_TABLES` (what we *claim* to cover, and what consumers type
// against) matches the live schema — so adding/removing a table in migrations
// without regenerating `tables.gen.ts` fails CI.

import { expect, test } from "bun:test";
import { rmSync } from "node:fs";
import { Entl } from "./index.js";
import { introspect } from "./introspect.ts";
import { ENTL_TABLES } from "./tables.gen.ts";

test("syncInto covers every entl table (no schema drift)", async () => {
  const tmp = `${process.env.TMPDIR ?? "/tmp"}/entl-cov-${Date.now()}.duckdb`;
  const live = (await introspect(new Entl(tmp))).map((t) => t.name).sort();
  rmSync(tmp, { force: true });
  expect(live).toEqual([...ENTL_TABLES].sort());
});
