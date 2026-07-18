// Schema introspection for entl's DuckDB. Kept separate from `sync.ts` (which
// re-exports the generated `tables.gen.ts`) so the codegen can use it without a
// bootstrap cycle.

export interface Column {
  name: string;
  type: string; // DuckDB data_type
  notNull: boolean;
}
export interface EntlTableMeta {
  name: string;
  columns: Column[];
  pk: string[]; // primary-key columns ([] if none)
}
export type EntlHandle = { query(sql: string): Promise<string> };

// Internal bookkeeping we never mirror.
const EXCLUDE = new Set(["sync_state"]);

/** Introspect entl's DuckDB: every mirrorable base table with columns + PK. */
export async function introspect(entl: EntlHandle): Promise<EntlTableMeta[]> {
  const cols = JSON.parse(
    await entl.query(
      `SELECT table_name, column_name, data_type, is_nullable, ordinal_position
       FROM information_schema.columns WHERE table_schema = 'main'
       ORDER BY table_name, ordinal_position`,
    ),
  ) as {
    table_name: string;
    column_name: string;
    data_type: string;
    is_nullable: string;
  }[];
  const bases = new Set(
    (
      JSON.parse(
        await entl.query(
          `SELECT table_name FROM information_schema.tables
         WHERE table_schema='main' AND table_type='BASE TABLE'
           AND table_name NOT LIKE '\\_%' ESCAPE '\\'`,
        ),
      ) as { table_name: string }[]
    ).map((r) => r.table_name),
  );
  const pks = JSON.parse(
    await entl.query(
      `SELECT table_name, constraint_column_names FROM duckdb_constraints()
       WHERE constraint_type = 'PRIMARY KEY'`,
    ),
  ) as { table_name: string; constraint_column_names: string[] }[];

  const byTable = new Map<string, EntlTableMeta>();
  for (const c of cols) {
    if (!bases.has(c.table_name) || EXCLUDE.has(c.table_name)) continue;
    let t = byTable.get(c.table_name);
    if (!t) {
      t = { name: c.table_name, columns: [], pk: [] };
      byTable.set(c.table_name, t);
    }
    t.columns.push({
      name: c.column_name,
      type: c.data_type,
      notNull: c.is_nullable === "NO",
    });
  }
  for (const p of pks) {
    const t = byTable.get(p.table_name);
    if (t) t.pk = p.constraint_column_names;
  }
  return [...byTable.values()].sort((a, b) => a.name.localeCompare(b.name));
}
