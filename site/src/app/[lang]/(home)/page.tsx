import Link from 'next/link';
import { Boxes, Database, GitBranch, HardDrive } from 'lucide-react';
import { EmbroideryBand } from '@/components/embroidery-band';
import { i18n } from '@/lib/i18n';

export function generateStaticParams() {
  return i18n.languages.map((lang) => ({ lang }));
}

const features = [
  {
    icon: Database,
    title: 'One engine, every shape',
    body: "Streaming, OLTP, and OLAP over the same local store. Repos aren't that big — your tools shouldn't pretend they are.",
  },
  {
    icon: GitBranch,
    title: 'git + forge, unified',
    body: 'Commits, refs, diffs, PRs, reviews, CI, events — one schema, queryable with plain SQL.',
  },
  {
    icon: Boxes,
    title: 'Embeddable',
    body: 'Run in-process in Node/Bun via native bindings, or reach for the CLI and the Rust crate.',
  },
  {
    icon: HardDrive,
    title: 'Local-first',
    body: 'No service to run, nothing leaves your machine. Fetch once, query forever.',
  },
];

const schemaTokens = [
  'commits',
  'refs',
  'file_changes',
  'gh_pull_requests',
  'gh_reviews',
  'gh_checks',
  'gh_events',
];

export default async function HomePage(props: { params: Promise<{ lang: string }> }) {
  const { lang } = await props.params;
  return (
    <main className="flex flex-1 flex-col">
      <section
        className="relative overflow-hidden text-white"
        style={{ background: 'linear-gradient(160deg, var(--entl-green-deep), var(--entl-green))' }}
      >
        {/* warm duckling-yellow glow */}
        <div
          aria-hidden
          className="pointer-events-none absolute -right-24 -top-32 h-[34rem] w-[34rem] rounded-full opacity-25 blur-3xl"
          style={{ background: 'radial-gradient(circle, var(--entl-yellow), transparent 62%)' }}
        />

        <div className="relative mx-auto flex max-w-5xl flex-col items-center gap-10 px-6 pb-28 pt-20 md:flex-row md:items-center md:gap-14">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src="/duckling.png"
            alt="Entl — a duckling in a Tyrolean hat"
            width={256}
            height={256}
            className="h-44 w-44 shrink-0 object-contain drop-shadow-2xl md:h-60 md:w-60"
          />
          <div className="text-center md:text-left">
            <h1 className="font-display text-6xl font-bold tracking-tight">
              <span className="text-wordmark">Entl</span>
            </h1>
            <p className="mt-4 text-xl font-medium text-white/95">
              The local engine for git + forge data.
            </p>
            <p className="mx-auto mt-3 max-w-xl text-white/80 md:mx-0">
              Streaming, OLTP, and OLAP over one DuckDB file — query a repo's history and forge
              activity locally, in any major language.
            </p>
            <div className="mt-8 flex flex-wrap justify-center gap-4 md:justify-start">
              <Link href={`/${lang}/docs/getting-started`} className="btn-duck rounded-lg px-5 py-2.5">
                Get started →
              </Link>
              <Link
                href="https://github.com/zmaril/entl"
                className="rounded-lg border border-white/40 px-5 py-2.5 font-medium text-white transition hover:bg-white/10"
              >
                View on GitHub
              </Link>
            </div>
            <div className="mt-8 flex flex-wrap justify-center gap-x-3 gap-y-1 font-mono text-xs text-white/55 md:justify-start">
              {schemaTokens.map((t, i) => (
                <span key={t}>
                  {t}
                  {i < schemaTokens.length - 1 ? <span className="ml-3 text-white/30">·</span> : null}
                </span>
              ))}
            </div>
          </div>
        </div>

        {/* folk embroidery band */}
        <EmbroideryBand className="absolute inset-x-0 bottom-0 opacity-70" />
      </section>

      <section className="mx-auto grid max-w-5xl grid-cols-1 gap-4 px-6 py-16 sm:grid-cols-2">
        {features.map((f) => {
          const Icon = f.icon;
          return (
            <div
              key={f.title}
              className="feature-card rounded-xl border border-fd-border bg-fd-card p-6"
            >
              <div
                className="mb-4 inline-flex h-10 w-10 items-center justify-center rounded-lg"
                style={{
                  backgroundColor: 'color-mix(in srgb, var(--entl-green) 14%, transparent)',
                  color: 'var(--color-fd-primary)',
                }}
              >
                <Icon className="h-5 w-5" />
              </div>
              <h3 className="font-display text-lg font-semibold">{f.title}</h3>
              <p className="mt-2 text-fd-muted-foreground">{f.body}</p>
            </div>
          );
        })}
      </section>
    </main>
  );
}
