export const appName = 'Entl';

// Production origin, used as `metadataBase` so OG/Twitter image URLs resolve to
// absolute links. Override per-deploy with NEXT_PUBLIC_SITE_URL; the default is
// a placeholder until the real domain is set.
export const siteUrl = process.env.NEXT_PUBLIC_SITE_URL ?? 'https://entl.dev';

export const docsRoute = '/docs';
export const docsImageRoute = '/og/docs';
export const docsContentRoute = '/llms.mdx/docs';

export const gitConfig = {
  user: 'zmaril',
  repo: 'entl',
  branch: 'main',
};
