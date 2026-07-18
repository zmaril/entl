import { metaSchema, pageSchema } from "fumadocs-core/source/schema";
import { defineConfig, defineDocs } from "fumadocs-mdx/config";
import { z } from "zod";

// You can customize Zod schemas for frontmatter and `meta.json` here
// see https://fumadocs.dev/docs/mdx/collections
export const docs = defineDocs({
  dir: "content/docs",
  docs: {
    schema: pageSchema,
    postprocess: {
      includeProcessedMarkdown: true,
    },
  },
  meta: {
    schema: metaSchema,
  },
});

// the blog — flat MDX posts with a date + author
export const blog = defineDocs({
  dir: "content/blog",
  docs: {
    schema: pageSchema.extend({
      date: z.string(),
      author: z.string(),
    }),
  },
});

export default defineConfig({
  mdxOptions: {
    // MDX options
  },
});
