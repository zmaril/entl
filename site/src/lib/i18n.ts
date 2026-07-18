import { defineI18n } from "fumadocs-core/i18n";

// One language for now (English). To add another, append it to `languages`
// and create the localized content (e.g. `content/docs/index.es.mdx`); the
// `[lang]` routing + language switcher are already wired.
//
// `hideLocale: 'never'` keeps the locale prefix on every URL (`/en/docs`).
// We need this because clean prefix-hiding relies on Next.js middleware, which
// `output: 'export'` (our static build) forbids — so we prefix instead.
export const i18n = defineI18n({
  defaultLanguage: "en",
  languages: ["en"],
  hideLocale: "never",
});
