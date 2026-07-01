// The PGlite sink: mirror entl's tables (read from its DuckDB) into a target
// Postgres/PGlite database, under an `entl` schema so they never collide with the
// app's own tables. Once rows land in PGlite, its `live` extension drives realtime.
//
// Coverage is **derived from entl's own schema** (introspection), not a hand-kept
// list — so it covers every base table by construction and can't silently drift.
// See `tables.gen.ts` + the coverage test for the compile-time / CI guard.
//
// `pg` is any PGlite/Postgres client with `.exec(sql)` and `.query(sql, params)`.
// `entl` is an `Entl` handle whose `.query(sql)` returns JSON rows.

import { type Column, type EntlHandle, introspect } from "./introspect";
import type { EntlTable } from "./tables.gen";

export { introspect } from "./introspect";
export { EntlTables, ENTL_TABLES, type EntlTable } from "./tables.gen";

type Pg = {
  exec(sql: string): Promise<unknown>;
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

// DuckDB data_type → Postgres type. BLOB columns are projected to hex text.
function pgType(duck: string): string {
  const t = duck.toUpperCase();
  if (t.startsWith("DECIMAL") || t === "HUGEINT") return "numeric";
  if (t.startsWith("TIMESTAMP")) return "timestamptz";
  return (
    {
      BIGINT: "bigint",
      UBIGINT: "numeric",
      INTEGER: "integer",
      UINTEGER: "bigint",
      SMALLINT: "smallint",
      TINYINT: "smallint",
      VARCHAR: "text",
      BOOLEAN: "boolean",
      DOUBLE: "double precision",
      FLOAT: "real",
      DATE: "date",
      BLOB: "text",
    }[t] ?? "text"
  );
}

const q = (id: string) => `"${id.replace(/"/g, '""')}"`;
const isBlob = (c: Column) => c.type.toUpperCase() === "BLOB";

const CHUNK = 500;

/** Mirror the chosen entl tables into `pg`. Default: all tables, schema `entl`. */
export async function syncInto(
  pg: Pg,
  entl: EntlHandle,
  opts: SyncOptions = {},
): Promise<Record<string, number>> {
  const schema = opts.schema ?? "entl";
  const all = await introspect(entl);
  const want = opts.tables ? new Set<string>(opts.tables) : null;
  if (want) {
    const have = new Set(all.map((t) => t.name));
    for (const name of want) {
      if (!have.has(name)) throw new Error(`unknown entl table: ${name}`);
    }
  }
  await pg.exec(`CREATE SCHEMA IF NOT EXISTS ${q(schema)};`);
  const counts: Record<string, number> = {};

  for (const t of all) {
    if (want && !want.has(t.name)) continue;
    const target = opts.rename?.[t.name as EntlTable] ?? t.name; // remap target name
    const dest = `${q(schema)}.${q(target)}`;

    const colDdl = t.columns.map((c) => `${q(c.name)} ${pgType(c.type)}`).join(", ");
    const pkDdl = t.pk.length ? `, PRIMARY KEY (${t.pk.map(q).join(", ")})` : "";
    await pg.exec(`CREATE TABLE IF NOT EXISTS ${dest} (${colDdl}${pkDdl})`);

    // Read from DuckDB (always the real entl table); BLOB oids come back as hex text.
    const sel = t.columns.map((c) => (isBlob(c) ? `lower(hex(${q(c.name)})) AS ${q(c.name)}` : q(c.name))).join(", ");
    const rows = JSON.parse(await entl.query(`SELECT ${sel} FROM ${q(t.name)}`)) as Record<string, unknown>[];

    const names = t.columns.map((c) => c.name);
    if (t.pk.length === 0) {
      await pg.exec(`DELETE FROM ${dest}`); // no PK → full refresh
    }
    const updates = names.filter((n) => !t.pk.includes(n)).map((n) => `${q(n)}=excluded.${q(n)}`).join(", ");
    const conflict = t.pk.length
      ? `ON CONFLICT (${t.pk.map(q).join(", ")}) DO ${updates ? `UPDATE SET ${updates}` : "NOTHING"}`
      : "";

    for (let i = 0; i < rows.length; i += CHUNK) {
      const chunk = rows.slice(i, i + CHUNK);
      const values: unknown[] = [];
      const tuples = chunk
        .map((row, r) => {
          const ph = names.map((_, c) => `$${r * names.length + c + 1}`);
          for (const n of names) values.push(row[n] ?? null);
          return `(${ph.join(", ")})`;
        })
        .join(", ");
      await pg.query(`INSERT INTO ${dest} (${names.map(q).join(", ")}) VALUES ${tuples} ${conflict}`, values);
    }
    counts[t.name] = rows.length;
  }
  return counts;
}
