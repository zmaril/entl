import type { Metadata } from "next";
import { Inter, Space_Grotesk } from "next/font/google";
import { appName, siteUrl } from "@/lib/shared";
import "./global.css";

const inter = Inter({
  subsets: ["latin"],
});

// Display face for the wordmark + hero/section headings — geometric but warm,
// pairs with Inter for body. Exposed as a CSS var, used via `.font-display`.
const display = Space_Grotesk({
  subsets: ["latin"],
  weight: ["500", "600", "700"],
  variable: "--font-display",
});

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: { default: appName, template: `%s · ${appName}` },
  description: "The local engine for git + forge data.",
};

// Root layout. `lang` is hardcoded to the default locale here because the root
// segment can't read the `[lang]` route param; with a single language this is
// correct. The per-locale `RootProvider` (theme, search, i18n) lives in
// `app/[lang]/layout.tsx`.
export default function Layout({ children }: LayoutProps<"/">) {
  return (
    <html
      lang="en"
      className={`${inter.className} ${display.variable}`}
      suppressHydrationWarning
    >
      <body className="flex flex-col min-h-screen">{children}</body>
    </html>
  );
}
