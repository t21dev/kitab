// SPDX-License-Identifier: AGPL-3.0-or-later
// Data the prerender step fetches or reads at build time and hands to the pages.

export type ReleaseAsset = { name: string; url: string; size: number };

export type Release = {
  tag: string;
  name: string;
  url: string;
  publishedAt: string;
  prerelease: boolean;
  assets: ReleaseAsset[];
};

export type PageData = {
  /** Latest published GitHub release, or null when there is none yet (or GitHub was unreachable at build time). */
  release: Release | null;
  /** HTML rendered from content/CHANGELOG.md or content/PRIVACY.md, for those pages only. */
  html?: string;
  /** Build date (YYYY-MM-DD), shown where a page says when it was generated. */
  builtAt: string;
};
