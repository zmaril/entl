import Link from 'next/link';
import { blogSource } from '@/lib/source';
import { i18n } from '@/lib/i18n';

export function generateStaticParams() {
  return i18n.languages.map((lang) => ({ lang }));
}

export default async function BlogIndex(props: { params: Promise<{ lang: string }> }) {
  const { lang } = await props.params;
  const posts = [...blogSource.getPages(lang)].sort((a, b) =>
    a.data.date < b.data.date ? 1 : -1,
  );

  return (
    <main className="mx-auto w-full max-w-3xl px-6 py-16">
      <h1 className="text-3xl font-bold">Blog</h1>
      <p className="mt-2 text-fd-muted-foreground">Release notes and write-ups.</p>
      <ul className="mt-10 space-y-8">
        {posts.map((p) => (
          <li key={p.url}>
            <Link href={p.url} className="text-xl font-semibold text-fd-primary hover:underline">
              {p.data.title}
            </Link>
            <p className="mt-1 text-sm text-fd-muted-foreground">
              {p.data.date} · {p.data.author}
            </p>
            <p className="mt-2 text-fd-muted-foreground">{p.data.description}</p>
          </li>
        ))}
      </ul>
    </main>
  );
}
