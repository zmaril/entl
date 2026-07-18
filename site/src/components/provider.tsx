"use client";
import { i18nProvider } from "fumadocs-ui/i18n";
import { RootProvider } from "fumadocs-ui/provider/next";
import type { ReactNode } from "react";
import SearchDialog from "@/components/search";
import { translations } from "@/lib/layout.shared";

export function Provider({
  lang,
  children,
}: {
  lang: string;
  children: ReactNode;
}) {
  return (
    <RootProvider
      search={{ SearchDialog }}
      i18n={i18nProvider(translations, lang)}
    >
      {children}
    </RootProvider>
  );
}
