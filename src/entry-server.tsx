// SPDX-License-Identifier: AGPL-3.0-or-later
import { renderToString } from "react-dom/server";
import { App } from "./App";
import type { PageData } from "./data";

export { ROUTES, SITE, STEPS, FAQ, ROADMAP, SHORTCUTS, COMPARE, PLATFORMS, FORMATS } from "./content";
export { themeCss } from "./themes";

export function render(path: string, data: PageData): string {
  return renderToString(<App path={path} data={data} />);
}
