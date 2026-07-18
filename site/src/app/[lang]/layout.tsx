import type { ReactNode } from "react";
import { Provider } from "@/components/provider";

export default async function LangLayout({
  params,
  children,
}: {
  params: Promise<{ lang: string }>;
  children: ReactNode;
}) {
  const { lang } = await params;
  return <Provider lang={lang}>{children}</Provider>;
}
