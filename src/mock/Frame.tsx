// SPDX-License-Identifier: AGPL-3.0-or-later
// The mock window's size. The window is laid out at a logical size like a real
// one (860 × 600 when there is room) and scaled to fit its box. In a narrow box
// it doesn't shrink past 70 %: it becomes a smaller window instead, and drops
// what the app drops in a small window (dock groups, a shorter page slider).
import { createContext, useContext, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useMock } from "./store";

const BASE_W = 860;
const BASE_H = 600;
const MIN_SCALE = 0.7;

type Win = { w: number; h: number; s: number };
const WinCtx = createContext<Win>({ w: BASE_W, h: BASE_H, s: 1 });
export const useWin = () => useContext(WinCtx);

const useIso = typeof window === "undefined" ? () => {} : useLayoutEffect;

export function Frame({ label, children }: { label: string; children: ReactNode }) {
  const box = useRef<HTMLDivElement>(null);
  const [win, setWin] = useState<Win | null>(null);
  const { theme } = useMock();
  useIso(() => {
    const el = box.current;
    if (!el) return;
    const fit = () => {
      const cw = el.clientWidth;
      const ch = el.clientHeight;
      if (!cw || !ch) return;
      const s = Math.max(MIN_SCALE, Math.min(1, cw / BASE_W, ch / BASE_H));
      setWin((old) => {
        const next = { w: Math.round(cw / s), h: Math.round(ch / s), s };
        return old && old.w === next.w && old.h === next.h && old.s === next.s ? old : next;
      });
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const style: CSSProperties | undefined = win ? { width: win.w, height: win.h, transform: `scale(${win.s})` } : undefined;
  return (
    <div ref={box} className="kbox" role="group" aria-label={label}>
      <WinCtx.Provider value={win ?? { w: BASE_W, h: BASE_H, s: 0.7 }}>
        <div className="kt kwin" data-kt={theme ?? undefined} data-measured={win ? "" : undefined} style={style}>
          {children}
        </div>
      </WinCtx.Provider>
    </div>
  );
}
