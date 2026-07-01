// Codegen: introspect entl's schema (a fresh, migrated DB) and emit, from that one
// source of truth:
//   • tables.gen.ts  — the typed `EntlTables` enum (+ ENTL_TABLES)
//   • schema.gen.ts  — the full Drizzle (pg-core) schema for every table
// Run `bun run gen` after changing migrations; the coverage test fails on drift.

import { rmSync, writeFileSync } from "node:fs";
import { Entl } from "./index.js";
import { type EntlTableMeta, introspect } from "./introspect.ts";

const tmp = `${process.env.TMPDIR ?? "/tmp"}/entl-gen-${Date.now()}.duckdb`;
const tables = await introspect(new Entl(tmp));
rmSync(tmp, { force: true });

const camel = (s: string) => s.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
const banner = "// AUTO-GENERATED from entl's schema by `bun run gen`. Do not edit by hand.\n";

// ---- tables.gen.ts (the enum) ----
const names = tables.map((t) => t.name);
const entries = names.map((n) => `  ${camel(n)}: ${JSON.stringify(n)},`).join("\n");
writeFileSync(
  new URL("./tables.gen.ts", import.meta.url),
  `${banner}// Regenerate after changing migrations; the coverage test fails if this drifts.

/** The entl tables, as a typed enum. Pass \`EntlTables.ghPullRequests\` to syncInto. */
export const EntlTables = {
${entries}
} as const;

export type EntlTable = (typeof EntlTables)[keyof typeof EntlTables];

/** Every entl table name (the values of EntlTables). */
export const ENTL_TABLES = Object.values(EntlTables) as EntlTable[];
`,
);

// ---- schema.gen.ts (the Drizzle pg-core contract) ----
const used = new Set<string>(["pgSchema"]);

function column(c: EntlTableMeta["columns"][number]): string {
  const t = c.type.toUpperCase();
  let fn: string;
  let args = JSON.stringify(c.name);
  if (t.startsWith("TIMESTAMP")) {
    fn = "timestamp";
    args += ", { withTimezone: true }";
  } else if (t === "BIGINT" || t === "UBIGINT") {
    fn = "bigint";
    args += ', { mode: "number" }';
  } else if (t === "INTEGER" || t === "UINTEGER") {
    fn = "integer";
  } else if (t === "SMALLINT" || t === "TINYINT") {
    fn = "smallint";
  } else if (t === "BOOLEAN") {
    fn = "boolean";
  } else if (t === "DOUBLE" || t === "FLOAT") {
    fn = "doublePrecision";
  } else if (t.startsWith("DECIMAL") || t === "HUGEINT") {
    fn = "numeric";
  } else {
    fn = "text"; // VARCHAR, and BLOB (mirrored as hex text)
  }
  used.add(fn);
  return `  ${camel(c.name)}: ${fn}(${args})${c.notNull ? ".notNull()" : ""},`;
}

function tableDef(t: EntlTableMeta): string {
  const cols = t.columns.map(column).join("\n");
  let extra = "";
  if (t.pk.length) {
    used.add("primaryKey");
    extra = `,\n  (t) => [primaryKey({ columns: [${t.pk.map((p) => `t.${camel(p)}`).join(", ")}] })]`;
  }
  return `export const ${camel(t.name)} = entl.table(${JSON.stringify(t.name)}, {\n${cols}\n}${extra});`;
}

const defs = tables.map(tableDef).join("\n\n");
const imports = [...used].sort().join(", ");
writeFileSync(
  new URL("./schema.gen.ts", import.meta.url),
  `${banner}import { ${imports} } from "drizzle-orm/pg-core";

/** The Postgres schema entl's tables are mirrored into by \`syncInto\`. */
export const entl = pgSchema("entl");

${defs}
`,
);

console.log(`generated ${names.length} tables → tables.gen.ts (enum) + schema.gen.ts (drizzle)`);
