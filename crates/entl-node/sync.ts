// The PGlite sink: mirror entl's tables into a target Postgres/PGlite database, under an `entl`
// schema so they never collide with the app's own tables. Once rows land in PGlite, its `live`
// extension drives realtime.
//
// The DDL, type mapping, blob→hex and upsert logic all live in **Rust core** now (`DriverSink`):
// `entl.driverPlan(opts)` streams `{sql, params}` statements and this file only executes them
// against `pg`. That means every language binding gets the same mirror for free — this is the thin
// JS executor. (Previously all of that logic lived here in TypeScript; see notes/design/multidb.md.)
//
// `pg` is any PGlite/Postgres client with `.query(sql, params)`. `entl` is an `Entl` handle.

import type { Entl } from "./index.js";
import type { EntlTable } from "./tables.gen";

export { EntlTables, ENTL_TABLES, type EntlTable } from "./tables.gen";

type Pg = {
  query(sql: string, params?: unknown[]): Promise<{ rows: unknown[] }>;
};

export interface SyncOptions {
  /** Which tables to mirror. Default: all. Pass e.g. `[EntlTables.pullRequests]`. */
  tables?: EntlTable[];
  /** Override the target table name per entl table (e.g. `{ [EntlTables.pullRequests]: "github_prs" }`). */
  rename?: Partial<Record<EntlTable, string>>;
  /** Target Postgres schema. Default `"entl"`. */
  schema?: string;
}

type Stmt = { sql: string; params: unknown[]; table: string | null };

/**
 * Mirror the chosen entl tables into `pg`. Default: all tables, schema `entl`.
 *
 * Returns per-table row counts. All the shaping is done in Rust (`Entl.driverPlan`); this drains
 * the resulting statement plan and runs each one, so a big mirror never blocks the event loop and
 * stays backpressured (the plan is bounded).
 */
export async function syncInto(
  pg: Pg,
  entl: Entl,
  opts: SyncOptions = {},
): Promise<Record<string, number>> {
  const plan = entl.driverPlan({
    tables: opts.tables ? [...opts.tables] : undefined,
    rename: opts.rename
      ? Object.entries(opts.rename).map(([from, to]) => ({ from, to: to as string }))
      : undefined,
    schema: opts.schema ?? "entl",
  });

  const counts: Record<string, number> = {};
  for (;;) {
    const next = await plan.next();
    if (next === null) break;
    const { sql, params, table } = JSON.parse(next) as Stmt;
    await pg.query(sql, params);
    // A row upsert (has bound params) tallies against its source table.
    if (table && params.length > 0) counts[table] = (counts[table] ?? 0) + 1;
  }
  return counts;
}
