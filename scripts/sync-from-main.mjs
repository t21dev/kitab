// SPDX-License-Identifier: AGPL-3.0-or-later
// Copies the files the site shows from the app branch into content/.
// Never edit content/*.md by hand on this branch; run `npm run sync` (or let the
// release workflow on main do it) so the site and the app never disagree.
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";

const ref = process.argv[2] ?? "origin/main";
const files = ["CHANGELOG.md", "PRIVACY.md"];

for (const f of files) {
  const body = execFileSync("git", ["show", `${ref}:${f}`], { encoding: "utf8" });
  writeFileSync(new URL(`../content/${f}`, import.meta.url), body);
  console.log(`synced ${f} from ${ref}`);
}
