#!/usr/bin/env bun
// Generates reference docs from the engine's own sources of truth:
//   - reference/schema.mdx    ← schema/schema_docs.json           (always; committed)
//   - reference/rust-api.mdx  ← crates/entl-core/src/*.rs          (always; committed)
//   - reference/node-api.mdx ← crates/entl-node/index.d.ts        (when present)
//   - reference/cli.mdx       ← `target/release/entl --help`       (when built)
//
// Run with `bun run gen`. It also runs as `prebuild`. Each generator is TOLERANT: if
// its source isn't available (e.g. the napi types aren't built, or the Rust binary
// isn't present on a docs-only CI), it skips that page and leaves the committed copy
// in place — so the deploy never fails on a missing source. Output carries an
// AUTO-GENERATED banner; edit the sources, not the generated files.

import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..", "..");
const SCHEMA_DOCS = join(ROOT, "schema/schema_docs.json");
const CORE_SRC = join(ROOT, "crates/entl-core/src");
const DTS = join(ROOT, "crates/entl-node/index.d.ts");
const BIN = join(ROOT, "target/release/entl");
const OUT = join(import.meta.dir, "..", "content", "docs", "reference");

const banner = (src: string) =>
  `{/* AUTO-GENERATED from ${src} by scripts/gen-reference.ts — run \`bun run gen\`. Do not edit by hand. */}\n\n`;
const fm = (title: string, desc: string) =>
  `---\ntitle: ${title}\ndescription: ${desc}\n---\n\n`;
// Source comments are arbitrary prose — escape `{`/`}` so MDX doesn't read e.g. a
// GitHub API path template (`/repos/{o}/{r}/…`) as a JS expression.
const mdxSafe = (s: string) => s.replace(/[{}]/g, "\\$&");

// ----------------------------------------------------------------- schema ---

type Col = {
  name: string;
  type: string;
  notNull: boolean;
  pk: boolean;
  def?: string;
  desc?: string;
};
type Table = { name: string; cols: Col[]; desc?: string };

// The schema reference as data: fluessig lowers the catalog (schema/entl.tsp) into
// schema/schema_docs.json — per physical table, the columns with their DuckDB
// types, flags, and the docs authored in entl.tsp. Regenerate with `bun run gen`
// in crates/entl-node (which runs scripts/gen.sh).
function parseTables(): Table[] {
  type Raw = {
    name: string;
    desc: string | null;
    cols: {
      name: string;
      type: string;
      notNull: boolean;
      pk: boolean;
      def: string | null;
      desc: string | null;
    }[];
  };
  const raw = JSON.parse(readFileSync(SCHEMA_DOCS, "utf8")) as Raw[];
  return raw
    .map((t) => ({
      name: t.name,
      desc: t.desc ?? undefined,
      cols: t.cols.map((c) => ({
        name: c.name,
        type: c.type,
        notNull: c.notNull,
        pk: c.pk,
        def: c.def ?? undefined,
        desc: c.desc ?? undefined,
      })),
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

function renderTable(t: Table): string {
  const withDesc = t.cols.some((c) => c.desc);
  const head = withDesc
    ? "| Column | Type | Description | |\n|---|---|---|---|"
    : "| Column | Type | |\n|---|---|---|";
  const rows = t.cols
    .map((c) => {
      const notes = [
        c.pk ? "**PK**" : "",
        c.notNull && !c.pk ? "not null" : "",
        c.def ? `default \`${c.def}\`` : "",
      ]
        .filter(Boolean)
        .join(", ");
      const desc = mdxSafe(c.desc ?? "").replace(/\|/g, "\\|"); // escape pipes + braces for the cell
      return withDesc
        ? `| \`${c.name}\` | \`${c.type}\` | ${desc} | ${notes} |`
        : `| \`${c.name}\` | \`${c.type}\` | ${notes} |`;
    })
    .join("\n");
  return `### \`${t.name}\`\n\n${t.desc ? `${mdxSafe(t.desc)}\n\n` : ""}${head}\n${rows}\n`;
}

function genSchema(): string {
  const tables = parseTables();
  const gh = tables.filter((t) => t.name.startsWith("gh_"));
  const git = tables.filter((t) => !t.name.startsWith("gh_"));
  return `${fm("Schema", "Every table Entl writes — generated from the fluessig catalog.")}${banner("schema/schema_docs.json (lowered from schema/entl.tsp)")}# Schema

Entl writes one DuckDB. **git-generic** tables are bare (so a future forge reuses them);
**GitHub** tables are namespaced \`gh_*\`. ${tables.length} tables in total.

<Callout>
Generated from the SQL migrations. Diffs and blob contents are **not** stored — they're
computed on demand (\`diffCommits\`, \`fileAt\`; see the [Node API](/docs/reference/node-api)).
</Callout>

## git

${git.map(renderTable).join("\n")}

## GitHub

${gh.map(renderTable).join("\n")}`;
}

// --------------------------------------------------------------- node api ---

type Entry = {
  name: string;
  kind: "function" | "class" | "interface";
  code: string;
  doc: string;
};

function parseDts(): Entry[] {
  const lines = readFileSync(DTS, "utf8").split("\n");
  const entries: Entry[] = [];
  let doc = "";
  let inDoc = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.trim().startsWith("/**")) {
      inDoc = !line.includes("*/");
      doc = line.replace(/\/\*\*|\*\//g, "").trim();
      doc = doc ? `${doc} ` : "";
      continue;
    }
    if (inDoc) {
      if (line.includes("*/")) inDoc = false;
      const text = line
        .replace(/^\s*\*\/?/, "")
        .replace(/\*\/\s*$/, "")
        .trim();
      if (text) doc += `${text} `;
      continue;
    }
    const fn = line.match(/^export declare function (\w+)/);
    if (fn) {
      entries.push({
        name: fn[1],
        kind: "function",
        code: line.replace(/^export declare /, ""),
        doc: doc.trim(),
      });
      doc = "";
      continue;
    }
    const block = line.match(/^export (?:declare )?(class|interface) (\w+)/);
    if (block) {
      const body: string[] = [line.replace(/^export (?:declare )?/, "")];
      while (i + 1 < lines.length && lines[i].trim() !== "}") {
        i++;
        body.push(lines[i]);
      }
      entries.push({
        name: block[2],
        kind: block[1] as "class" | "interface",
        code: body.join("\n"),
        doc: doc.trim(),
      });
      doc = "";
      continue;
    }
    if (line.trim()) doc = "";
  }
  return entries;
}

function renderEntry(
  e: { name: string; doc: string; code: string },
  lang = "ts",
): string {
  return `### \`${e.name}\`\n\n${e.doc ? `${e.doc}\n\n` : ""}\`\`\`${lang}\n${e.code.trim()}\n\`\`\`\n`;
}

function genNodeApi(): string | null {
  if (!existsSync(DTS)) return null;
  const e = parseDts();
  const entl = e.find((x) => x.name === "Entl");
  const types = e.filter((x) => x.kind !== "function" && x.name !== "Entl");
  const fns = e.filter((x) => x.kind === "function" && x.name !== "version");
  return `${fm("Node API", "The @entl/node bindings — generated from the native addon's types.")}${banner("crates/entl-node/index.d.ts")}# Node API

The \`@entl/node\` native bindings. Heavy methods return Promises and run off the JS thread.

<Callout>
Generated from the napi type definitions (\`index.d.ts\`).
</Callout>

## The \`Entl\` class

${entl ? renderEntry(entl) : ""}

## Functions

${fns.map((x) => renderEntry(x)).join("\n")}

## Types

${types.map((x) => renderEntry(x)).join("\n")}`;
}

// --------------------------------------------------------------- rust api ---

function walkRs(dir: string): string[] {
  const out: string[] = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) out.push(...walkRs(p));
    else if (e.name.endsWith(".rs")) out.push(p);
  }
  return out;
}

function libExports(): string[] {
  const lib = readFileSync(join(CORE_SRC, "lib.rs"), "utf8");
  const names: string[] = [];
  for (const m of lib.matchAll(/pub use [\w:]+::(?:\{([^}]*)\}|(\w+))/g)) {
    for (const n of (m[1] ?? m[2]).split(",")) {
      const name = n.trim().replace(/^.*\bas\s+/, "");
      if (name) names.push(name);
    }
  }
  return [...new Set(names)];
}

function docAbove(lines: string[], idx: number): string {
  const doc: string[] = [];
  for (let i = idx - 1; i >= 0; i--) {
    const t = lines[i].trim();
    if (t.startsWith("///")) doc.unshift(t.replace(/^\/\/\/\s?/, ""));
    else if (t.startsWith("#[")) {
    } // skip attributes
    else break;
  }
  return doc.join("\n").trim();
}

function captureFnSig(lines: string[], idx: number): string {
  let sig = "";
  for (let i = idx; i < lines.length; i++) {
    const brace = lines[i].indexOf("{");
    const semi = lines[i].indexOf(";");
    if (brace >= 0) return `${sig}${lines[i].slice(0, brace)}`.trim();
    if (semi >= 0) return `${sig}${lines[i].slice(0, semi + 1)}`.trim();
    sig += `${lines[i]}\n`;
  }
  return sig.trim();
}

function captureStruct(lines: string[], idx: number): string {
  if (lines[idx].includes(";") || !lines[idx].includes("{"))
    return lines[idx].trim();
  let depth = 0;
  let out = "";
  for (let i = idx; i < lines.length; i++) {
    out += `${lines[i]}\n`;
    for (const ch of lines[i]) {
      if (ch === "{") depth++;
      else if (ch === "}") depth--;
    }
    if (depth === 0) break;
  }
  return out.trim();
}

function genRustApi(): string {
  const files = walkRs(CORE_SRC).map((p) => ({
    lines: readFileSync(p, "utf8").split("\n"),
  }));
  const exports = libExports();
  const fns: { name: string; doc: string; code: string }[] = [];
  const types: { name: string; doc: string; code: string }[] = [];

  for (const name of exports) {
    for (const { lines } of files) {
      const fnIdx = lines.findIndex((l) =>
        new RegExp(`^\\s*pub fn ${name}\\b`).test(l),
      );
      if (fnIdx >= 0) {
        fns.push({
          name,
          doc: docAbove(lines, fnIdx),
          code: captureFnSig(lines, fnIdx).replace(/^pub /, ""),
        });
        break;
      }
      const tyIdx = lines.findIndex((l) =>
        new RegExp(`^\\s*pub (?:struct|enum) ${name}\\b`).test(l),
      );
      if (tyIdx >= 0) {
        let code = captureStruct(lines, tyIdx).replace(/^pub /, "");
        // For Db, append its public methods (constructors etc.).
        if (name === "Db") {
          const methods = implMethods(files, "Db");
          if (methods.length)
            code += `\n\nimpl Db {\n${methods.map((m) => `    ${m.code};`).join("\n")}\n}`;
        }
        types.push({ name, doc: docAbove(lines, tyIdx), code });
        break;
      }
    }
  }

  return `${fm("Rust API", "The entl-core crate — generated from its public source.")}${banner("crates/entl-core/src/*.rs")}# Rust API

The \`entl-core\` crate — the engine itself. The public surface re-exported from \`lib.rs\`.

<Callout>
Generated from the crate source.
</Callout>

## Types

${types.map((x) => renderEntry(x, "rust")).join("\n")}

## Functions

${fns.map((x) => renderEntry(x, "rust")).join("\n")}`;
}

function implMethods(
  files: { lines: string[] }[],
  type: string,
): { name: string; code: string }[] {
  const out: { name: string; code: string }[] = [];
  for (const { lines } of files) {
    for (let i = 0; i < lines.length; i++) {
      if (!new RegExp(`^impl(?:<[^>]*>)?\\s+${type}\\b`).test(lines[i].trim()))
        continue;
      let depth = 0;
      let started = false;
      for (let j = i; j < lines.length; j++) {
        for (const ch of lines[j]) {
          if (ch === "{") {
            depth++;
            started = true;
          } else if (ch === "}") depth--;
        }
        const fn = lines[j].match(/^\s*pub fn (\w+)/);
        if (fn && j > i)
          out.push({
            name: fn[1],
            code: captureFnSig(lines, j).replace(/^\s*pub /, ""),
          });
        if (started && depth === 0) {
          i = j;
          break;
        }
      }
    }
  }
  return out;
}

// ------------------------------------------------------------------- cli ---

function runHelp(args: string[]): string | null {
  if (!existsSync(BIN)) return null;
  const r = Bun.spawnSync([BIN, ...args], { stdout: "pipe", stderr: "pipe" });
  const text = new TextDecoder().decode(r.stdout).trim();
  return text || null;
}

function genCli(): string | null {
  const main = runHelp(["--help"]);
  if (main === null) return null; // binary not built → keep committed copy
  const subs = ["init", "load", "watch", "analysis", "query", "tables"];
  let body = "";
  for (const s of subs) {
    const h = runHelp([s, "--help"]);
    if (h) body += `### \`entl ${s}\`\n\n\`\`\`text\n${h}\n\`\`\`\n\n`;
  }
  return `${fm("CLI", "The Entl command-line interface — generated from its --help output.")}${banner("`entl --help`")}# CLI

\`\`\`text
${main}
\`\`\`

## Commands

${body}`;
}

// --------------------------------------------------------------------- run ---

const pages: Record<string, string | null> = {
  "schema.mdx": genSchema(),
  "rust-api.mdx": genRustApi(),
  "node-api.mdx": genNodeApi(),
  "cli.mdx": genCli(),
};
for (const [file, content] of Object.entries(pages)) {
  if (content === null) {
    console.log(`skip ${file} (source unavailable — keeping committed copy)`);
    continue;
  }
  writeFileSync(join(OUT, file), content);
  console.log(`generated ${file}`);
}
