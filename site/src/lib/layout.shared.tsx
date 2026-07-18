import { uiTranslations } from "fumadocs-ui/i18n";
import type { BaseLayoutProps } from "fumadocs-ui/layouts/shared";
import { i18n } from "./i18n";
import { appName, gitConfig } from "./shared";

// UI string packs per locale. `uiTranslations()` supplies Fumadocs's built-in
// strings; `.add()` layers our own (currently just the switcher display name).
export const translations = i18n
  .translations()
  .extend(uiTranslations())
  .add({
    en: {
      displayName: "English",
    },
  });

export function baseOptions(locale: string): BaseLayoutProps {
  return {
    // enable the language switcher; locale data comes from RootProvider's i18n
    // context (passing the config object here would break client serialization)
    i18n: true,
    nav: {
      title: (
        <span className="inline-flex items-center gap-2">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src="/duckling-mark.png"
            alt=""
            width={28}
            height={28}
            className="h-7 w-7 object-contain"
          />
          <span className="font-semibold">{appName}</span>
        </span>
      ),
      url: `/${locale}`,
    },
    links: [
      { text: "Docs", url: `/${locale}/docs`, active: "nested-url" },
      { text: "Blog", url: `/${locale}/blog`, active: "nested-url" },
    ],
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}
