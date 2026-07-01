import type { Metadata } from 'next';

// Static-export-friendly root redirect: `/` → `/en`. Next.js middleware (the
// usual way to do this) isn't available under `output: 'export'`, so we emit a
// meta-refresh page instead. React hoists the <meta> into <head>.
export const metadata: Metadata = {
  robots: { index: false, follow: false },
};

export default function RootRedirect() {
  return (
    <>
      <meta httpEquiv="refresh" content="0; url=/en" />
      <main className="mx-auto max-w-3xl px-6 py-24">
        Redirecting to <a href="/en">/en</a>…
      </main>
    </>
  );
}
