import { createFromSource } from "fumadocs-core/search/server";
import { source } from "@/lib/source";

export const revalidate = false;

export const { staticGET: GET } = createFromSource(source, {
  // i18n source → map each locale to an Orama language.
  // https://docs.orama.com/docs/orama-js/supported-languages
  localeMap: { en: "english" },
});
