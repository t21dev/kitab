// SPDX-License-Identifier: AGPL-3.0-or-later
import type { PageData } from "./data";
import { Layout } from "./components/Layout";
import { Home } from "./pages/Home";
import { Docs } from "./pages/Docs";
import { Download, MarkdownPage, NotFound, Shortcuts } from "./pages/Other";

export function App({ path, data }: { path: string; data: PageData }) {
  return <Layout>{page(path, data)}</Layout>;
}

function page(path: string, data: PageData) {
  switch (path) {
    case "/":
      return <Home data={data} />;
    case "/download":
      return <Download data={data} />;
    case "/docs":
      return <Docs />;
    case "/shortcuts":
      return <Shortcuts />;
    case "/changelog":
      return <MarkdownPage kicker="Changes" title="Changelog" lead="Every user-visible change, newest first. Kitab follows Semantic Versioning; before 1.0, each phase is a minor release." data={data} />;
    case "/privacy":
      return <MarkdownPage kicker="Privacy" title="Privacy" lead="What Kitab Reader does with your data, and what this website does." data={data} />;
    default:
      return <NotFound />;
  }
}
