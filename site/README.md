# Entl docs site (Fumadocs)

[Fumadocs](https://fumadocs.dev) (Next.js + MDX + Tailwind) — multilingual-ready,
**static-exported** docs with synced multi-language code tabs, a generated reference, and a
blog. Themed a duckling-in-a-Tyrolean-hat green + yellow.

## Develop

```sh
cd site
bun install
bun run dev      # http://localhost:3000
bun run build    # static export → site/out
bun run start    # serve the build (serve out)
bun run gen      # regenerate the reference pages from source (also runs on prebuild)
```

## How the pieces map

- **Synced code tabs** — `<Tabs groupId="lang" persist>` + `<Tab>` (registered globally in
  `src/components/mdx.tsx`). Pick a language once, every snippet follows.
- **Diátaxis** — folders under `content/docs/` (`guides/`, `reference/`, `explanation/`)
  with `meta.json` for titles + order.
- **Reference (generated)** — `scripts/gen-reference.ts` reads the engine's sources
  (migrations / Rust / `index.d.ts` / `entl --help`) and writes `content/docs/reference/*.mdx`
  (with `<Callout>` admonitions). Tolerant: a missing source keeps the committed copy, so the
  cloud build never fails. Runs on `prebuild`.
- **Blog** — `content/blog/*.mdx` (a `blog` collection in `source.config.ts`) + the routes in
  `src/app/blog/`.
- **Theme** — `src/app/global.css` overrides Fumadocs's `--color-fd-*` (green primary) + the
  duckling-yellow CTA.
- **Search** — Orama, built in (`src/app/api/search`), works on the static build.

## Versioning — branch per version

No in-repo version machinery: this branch is **latest**. To publish a frozen version, cut a
git branch (`v1`) and deploy it to its own subdomain (`v1.entl.dev`), then link to it from
the nav. Old branches are unaffected by future changes.

## Deploy — Cloudflare Pages (static)

`output: 'export'` produces a fully static site in `out/` (Orama search works statically).

| Setting | Value |
|---|---|
| Build command | `bun run build` |
| Build output directory | `out` |
| Root directory (Advanced) | `site` (the repo is a monorepo; `../crates/...` is reachable for the generator) |

Set the production domain in the metadata before launch.

## i18n

Wired with one language (`en`). Routes live under `src/app/[lang]/`; `src/lib/i18n.ts`
declares the locales. Because static export forbids the locale-hiding middleware, URLs are
prefixed (`/en/docs`) and `/` redirects to `/en`. Add a language by appending it to
`languages` in `i18n.ts` and adding localized content (e.g. `content/docs/index.es.mdx`).
