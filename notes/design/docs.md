<!-- straitjacket-allow-file:duplication — design notes quote repo trees and config blocks verbatim. -->

# Entl docs site — software design

The design of the documentation site under `site/`. This is about *how the site is built*,
not what it says. For the engine itself, see [engine.md](./engine.md); for the *why*,
[purpose.md](../purpose.md).

## Purpose

Two jobs at once:

1. **Public documentation** for Entl — usable by people who reach it cold.
2. **A review surface** — a presentable form of the work, the way it'll eventually be seen.

Two non-negotiables shaped the stack: it must support **multiple spoken languages** and
**versioned docs**, and the reference must not drift from the code.

## Framework: Fumadocs (Next.js + MDX, static export)

We use **[Fumadocs](https://fumadocs.dev)** — Next.js + MDX + Tailwind — built as a fully
**static export** (`output: 'export'` → `out/`).

History worth knowing: this started on **Docusaurus** (chosen for its built-in versioning,
after a false start on Starlight), then moved to Fumadocs for the modern Next.js/Tailwind
foundation, the nicer default UI, and built-in Orama search. The one thing Docusaurus gave
that Fumadocs doesn't is a `docs:version` snapshot command — here, **versioning is
branch-per-version** instead (below), which suits a pre-release library and avoids snapshots
bloating the repo.

## Information architecture: Diátaxis

Four modes, expressed as folders under `content/docs/` with `meta.json` for titles + order:

- **Tutorials** — `getting-started.mdx`
- **How-to guides** — `guides/`
- **Reference** — `reference/` (generated)
- **Explanation** — `explanation/`

## The reference generator — the heart of it

`site/scripts/gen-reference.ts` (Bun) generates the Reference section from the engine's own
**sources of truth**, so the reference can't drift:

| page | source |
|---|---|
| `reference/schema.mdx` | `crates/entl-core/migrations/*.sql` |
| `reference/rust-api.mdx` | `crates/entl-core/src/**/*.rs` |
| `reference/node-api.mdx` | `crates/entl-node/index.d.ts` (gitignored) |
| `reference/cli.mdx` | `target/release/entl --help` (needs the binary) |

- **Tolerant by design.** If a source is missing (the napi types aren't built, or there's no
  Rust toolchain on the cloud build host), the generator **skips that page and keeps the
  committed copy** rather than failing. The generated `.mdx` files are committed; `prebuild`
  runs `gen` on every build, so schema + rust-api stay fresh from the cloud build while
  node-api + cli fall back to their committed copies.
- **Explanations live in the source.** A `--` comment block above `CREATE TABLE` documents
  the table; a trailing `-- …` documents a column. Rust `///`, napi JSDoc, and clap `--help`
  do the same. All of it ports straight into the generated pages.
- **Fumadocs MDX.** The generator emits `<Callout>` admonitions (not Docusaurus `:::note`)
  and escapes MDX-special chars (`{`/`}`/`|`) in ported prose so a `/repos/{o}/{r}` template
  doesn't get read as a JS expression.

## Versioning — branch per version

No in-repo machinery: this branch is **latest**. To freeze a version, cut a git branch
(`v1`) and deploy it to its own subdomain (`v1.entl.dev`), linked from the nav. Old branches
are untouched by future changes.

## i18n

**Wired, one language (`en`) for now.** Routes live under `app/[lang]/`; `src/lib/i18n.ts`
declares the locales and the language switcher is on. Adding a language is: append it to
`languages` in `i18n.ts` and add the localized content (e.g. `content/docs/index.es.mdx`).

The static-export catch worth knowing: Fumadocs hides the locale prefix (`/docs` instead of
`/en/docs`) with Next.js **middleware**, which `output: 'export'` forbids. So we run
`hideLocale: 'never'` — every URL carries its locale (`/en/docs`) — and a meta-refresh
`app/page.tsx` redirects `/` → `/en`. Internal links authored as locale-agnostic absolute
paths (`/docs/…`) are prefixed at render in the docs page (`localize()`); relative links
(`./`, `../`) resolve via `createRelativeLink`. To get clean prefix-less URLs for a default
locale later, the site would need to move off pure static export to an SSR/Workers deploy.

## Synced code tabs

`<Tabs groupId="lang" persist>` + `<Tab value="…">`, registered globally in
`src/components/mdx.tsx` so content doesn't import them per file. The reader's choice persists
across the whole site — the Stripe-style multi-language switcher.

## Blog

A `blog` content collection in `source.config.ts` (posts in `content/blog/*.mdx`, schema =
page + `date` + `author`) loaded in `src/lib/source.ts`, with routes in `src/app/blog/` (list
+ `[slug]`). Fumadocs has no built-in blog; this is ~3 small files.

## Theme

`src/app/global.css` overrides Fumadocs's `--color-fd-primary` (hunter green, light + dark),
adds a duckling-yellow `.btn-duck` CTA, and a display font for the wordmark. The landing
(`src/app/[lang]/(home)/page.tsx`) is a custom green hero built around the **duckling mascot**
(`public/duckling.png` — an AI-generated duckling in a Tyrolean hat), which also sits in the
nav and serves as the favicon (`src/app/icon.png`), with a procedural folk-embroidery band
(`src/components/embroidery-band.tsx`) along the bottom.

## Build & deploy — Cloudflare Pages (static)

`output: 'export'` produces a fully static site in `site/out` (Orama search works statically).

| Setting | Value |
|---|---|
| Build command | `bun run build` (runs `gen` via `prebuild`) |
| Build output directory | `out` |
| Root directory (Advanced) | `site` (monorepo; `../crates/...` is reachable for the generator) |

`turbopack.root` is set in `next.config.mjs` to silence the monorepo multi-lockfile warning.

## Key files

```
site/
  source.config.ts           content collections (docs + blog)
  next.config.mjs            static export + turbopack root
  src/lib/i18n.ts            locale config (en; hideLocale 'never')
  src/lib/source.ts          loaders (docs + blog), i18n-aware
  src/lib/shared.ts          appName, siteUrl, gitConfig, routes
  src/lib/layout.shared.tsx  baseOptions(locale) + translations; nav (duckling + links)
  src/components/mdx.tsx      global MDX components (Tabs/Tab/Callout)
  src/app/layout.tsx         root <html>, fonts, metadataBase, favicon
  src/app/page.tsx           / → /en redirect (static-safe)
  src/app/[lang]/...         home (duckling hero) · docs · blog, all under [lang]
  src/app/global.css         theme (green + duckling yellow)
  content/{docs,blog}/       docs (Diátaxis + meta.json) + blog posts
  scripts/gen-reference.ts   the reference generator
```

## Conventions & gotchas

- **Don't edit generated reference files** — edit the migrations / Rust / napi sources, run
  `bun run gen`.
- **Adding a content collection** (`source.config.ts`) needs a `fumadocs-mdx` regen; the
  build/`postinstall` handle it.
- **The generator uses Bun APIs** (`import.meta.dir`), so `scripts/` is excluded from the
  Next `tsconfig` typecheck.

## Open items

- **A second language** — append to `languages` in `src/lib/i18n.ts` + add localized content
  (the `[lang]` routing + switcher are already wired).
- Wire the **version switcher** in the nav once a real `v1` branch exists.

