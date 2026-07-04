// straitjacket-allow-file:duplication — Next.js layout boilerplate; the home/docs
// layouts share params plumbing but wrap different fumadocs layouts.
import { HomeLayout } from 'fumadocs-ui/layouts/home';
import type { ReactNode } from 'react';
import { baseOptions } from '@/lib/layout.shared';

export default async function Layout({
  params,
  children,
}: {
  params: Promise<{ lang: string }>;
  children: ReactNode;
}) {
  const { lang } = await params;
  return <HomeLayout {...baseOptions(lang)}>{children}</HomeLayout>;
}
