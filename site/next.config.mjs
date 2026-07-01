import { createMDX } from 'fumadocs-mdx/next';

const withMDX = createMDX();

/** @type {import('next').NextConfig} */
const config = {
  output: 'export',
  reactStrictMode: true,
  // this app is the workspace root (silences the multi-lockfile warning in the monorepo)
  turbopack: { root: import.meta.dirname },
};

export default withMDX(config);
