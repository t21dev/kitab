// SPDX-License-Identifier: AGPL-3.0-or-later
// Settings shared by every mock on the page, like the app's own settings: pick a
// theme in one window (⋯ menu or Settings) and every window follows. Nothing is
// stored; a reload starts from the site's theme again.
import { useSyncExternalStore } from "react";

export type MockSettings = {
  /** A Kitab theme id, or null to follow the site's Light/Dark. */
  theme: string | null;
  compactDock: boolean;
  fadeDock: boolean;
};

let state: MockSettings = { theme: null, compactDock: false, fadeDock: false };
const subs = new Set<() => void>();

export function setMock(patch: Partial<MockSettings>) {
  state = { ...state, ...patch };
  subs.forEach((f) => f());
}

const SERVER: MockSettings = state;

export function useMock(): MockSettings {
  return useSyncExternalStore(
    (s) => {
      subs.add(s);
      return () => subs.delete(s);
    },
    () => state,
    () => SERVER,
  );
}

/** Whether the site itself is dark right now (system setting or the visitor's pick). */
export function useSiteDark(): boolean {
  return useSyncExternalStore(
    (s) => {
      const mq = window.matchMedia("(prefers-color-scheme: dark)");
      mq.addEventListener("change", s);
      const mo = new MutationObserver(s);
      mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
      return () => {
        mq.removeEventListener("change", s);
        mo.disconnect();
      };
    },
    () => {
      const t = document.documentElement.getAttribute("data-theme");
      return t ? t === "dark" : window.matchMedia("(prefers-color-scheme: dark)").matches;
    },
    () => false,
  );
}
