import { getMDXComponents } from '@/components/mdx';
import { blogSource } from '@/lib/source';
import { DocsBody } from 'fumadocs-ui/layouts/docs/page';
import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';

type Params = Promise<{ lang: string; slug: string }>;

export default async function Page(props: { params: Params }) {
  const { lang, slug } = await props.params;
  const page = blogSource.getPage([slug], lang);
  if (!page) notFound();

  const MDX = page.data.body;

  return (
    <main className="mx-auto w-full max-w-3xl px-6 py-16">
      <Link href={`/${lang}/blog`} className="text-sm text-fd-muted-foreground hover:underline">
        ← Blog
      </Link>
      <h1 className="mt-4 text-3xl font-bold">{page.data.title}</h1>
      <p className="mt-2 text-sm text-fd-muted-foreground">
        {page.data.date} · {page.data.author}
      </p>
      <DocsBody className="mt-8">
        <MDX components={getMDXComponents()} />
      </DocsBody>
    </main>
  );
}

export function generateStaticParams() {
  return blogSource.getPages().map((p) => ({ lang: p.locale, slug: p.slugs[0] }));
}

export async function generateMetadata(props: { params: Params }): Promise<Metadata> {
  const { lang, slug } = await props.params;
  const page = blogSource.getPage([slug], lang);
  if (!page) notFound();
  return { title: page.data.title, description: page.data.description };
}
