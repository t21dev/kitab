// SPDX-License-Identifier: AGPL-3.0-or-later
// The download that fits the visitor's computer: worked out in the browser after the page loads,
// so the prerendered page (and anyone without JavaScript) keeps the plain links to /download.
import { useEffect, useState } from "react";
import type { Release, ReleaseAsset } from "../data";
import { Icon } from "./Icon";

export type Os = "windows" | "mac" | "linux";
type Seen = { os: Os; arm: boolean | null };

type UaData = {
  platform?: string;
  getHighEntropyValues?: (hints: string[]) => Promise<{ architecture?: string; platform?: string }>;
};

/** The visitor's system, or null on phones, tablets and anything unknown. `arm` is null when the
 * browser doesn't say (Safari, Firefox). */
async function detect(): Promise<Seen | null> {
  const ua = navigator.userAgent;
  if (/Android|iPhone|iPad|iPod|Mobile/i.test(ua)) return null;
  let os: Os | null = /Windows/i.test(ua) ? "windows" : /Mac OS X|Macintosh/i.test(ua) ? "mac" : /Linux|X11|CrOS/i.test(ua) ? "linux" : null;
  // An iPad asking for the desktop site says "Macintosh" but has a touch screen.
  if (os === "mac" && navigator.maxTouchPoints > 1) return null;
  if (!os) return null;
  let arm: boolean | null = /aarch64|arm64/i.test(ua) ? true : os === "linux" && /x86_64/i.test(ua) ? false : null;
  const uad = (navigator as Navigator & { userAgentData?: UaData }).userAgentData;
  if (uad?.getHighEntropyValues) {
    try {
      const v = await uad.getHighEntropyValues(["architecture"]);
      if (v.architecture) arm = v.architecture.toLowerCase().startsWith("arm");
    } catch {
      // Keep what the user agent said.
    }
  }
  return { os, arm };
}

export function useSystem(): Seen | null {
  const [seen, setSeen] = useState<Seen | null>(null);
  useEffect(() => {
    let live = true;
    detect().then((s) => live && setSeen(s));
    return () => {
      live = false;
    };
  }, []);
  return seen;
}

export type Pick = { name: string; main: { label: string; asset: ReleaseAsset }; more: { label: string; asset: ReleaseAsset }[] };

const NAMES: Record<Os, string> = { windows: "Windows", mac: "macOS", linux: "Linux" };

/** The file to offer first, and the others worth a link, for one system. */
export function pick(release: Release, seen: Seen): Pick | null {
  const find = (re: RegExp) => release.assets.find((a) => re.test(a.name));
  const opts = (list: [string, RegExp][]) => list.flatMap(([label, re]) => (find(re) ? [{ label, asset: find(re)! }] : []));
  let all: { label: string; asset: ReleaseAsset }[] = [];
  if (seen.os === "windows") {
    // Windows on Arm runs the x64 build.
    all = opts([
      ["Installer (.exe)", /_x64_setup\.exe$/],
      ["Portable zip", /_x64_portable\.zip$/],
    ]);
  } else if (seen.os === "mac") {
    const silicon: [string, RegExp] = ["Apple silicon (.dmg)", /_aarch64\.dmg$/];
    const intel: [string, RegExp] = ["Intel Mac (.dmg)", /_x86_64\.dmg$/];
    all = opts(seen.arm === false ? [intel, silicon] : [silicon, intel]);
  } else {
    const arch = seen.arm ? "(aarch64|arm64)" : "(x86_64|amd64)";
    all = opts([
      ["AppImage", new RegExp(`_${arch}\\.AppImage$`)],
      [".deb (Debian, Ubuntu)", new RegExp(`_${arch}\\.deb$`)],
      [".rpm (Fedora, openSUSE)", new RegExp(`_${arch}\\.rpm$`)],
      ["Flatpak", new RegExp(`_${arch}\\.flatpak$`)],
    ]);
  }
  if (!all.length) return null;
  return { name: NAMES[seen.os], main: all[0], more: all.slice(1) };
}

export function size(n: number): string {
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(n / 1024))} KB`;
}

/** Download page: the file for this computer, above the full list. */
export function ForYou({ release }: { release: Release }) {
  const seen = useSystem();
  const p = seen && pick(release, seen);
  if (!p) return null;
  return (
    <div className="mb-8 rounded-2xl bg-panel p-5 ring-1 ring-line sm:p-6">
      <p className="text-[12.5px] font-medium text-faint">Recommended for your computer</p>
      <div className="mt-3 flex flex-wrap items-center gap-x-5 gap-y-3">
        <a
          href={p.main.asset.url}
          className="inline-flex h-11 items-center gap-2 rounded-xl bg-text px-5 text-[15px] font-medium text-bg shadow-elev hover:opacity-90"
        >
          <Icon name="download" size={17} /> Download for {p.name}
        </a>
        <span className="text-[13.5px] text-muted">
          {p.main.label} · {size(p.main.asset.size)}
        </span>
      </div>
      {p.more.length > 0 && (
        <p className="mt-4 text-[13.5px] text-faint">
          Or:{" "}
          {p.more.map((m, i) => (
            <span key={m.asset.name}>
              {i > 0 && " · "}
              <a href={m.asset.url} className="text-muted underline decoration-line underline-offset-2 hover:text-text">
                {m.label}
              </a>
            </span>
          ))}
        </p>
      )}
    </div>
  );
}
