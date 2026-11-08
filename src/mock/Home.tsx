// SPDX-License-Identifier: AGPL-3.0-or-later
// Home (shell.rs, home_ui.rs): the pinned header with search and buttons, the
// shelf chips, Continue reading, and the Recent cards whose covers rise 6 px
// under the pointer.
import { useState } from "react";
import { Cover, KIcon, Progress } from "./atoms";
import { DOCS, RECENT } from "./docs";
import { useWin } from "./Frame";

type Shelf = "Recent" | "All" | "Favourites" | "In progress" | "Finished";
const SHELVES: Shelf[] = ["Recent", "All", "Favourites", "In progress", "Finished"];

export function HomeView({ onOpen, onSettings }: { onOpen: (id: string) => void; onSettings: () => void }) {
  const { w } = useWin();
  const [shelf, setShelf] = useState<Shelf>("Recent");
  const all = RECENT.map((id) => DOCS[id]);
  const items =
    shelf === "Favourites"
      ? all.filter((d) => d.favourite)
      : shelf === "In progress"
        ? all.filter((d) => d.progress > 0 && d.progress < 0.995)
        : shelf === "Finished"
          ? all.filter((d) => d.progress >= 0.995)
          : shelf === "All"
            ? [...all].sort((a, b) => a.title.localeCompare(b.title))
            : all;
  const continuing = all.filter((d) => d.progress > 0 && d.progress < 0.995).slice(0, Math.max(1, Math.min(3, Math.floor((w - 80 + 16) / 336))));
  const narrow = w < 640;
  return (
    <div className="k-home">
      <div className="k-home__head">
        <div className="k-home__top">
          <h2 className="k-home__title">Home</h2>
          <div className="k-home__actions">
            {!narrow ? (
              <label className="k-search">
                <KIcon name="search" size={14} />
                <input type="text" placeholder="Search your library" aria-label="Search your library" />
              </label>
            ) : null}
            <button type="button" className="k-btn" onClick={onSettings}>
              <KIcon name="settings" size={15} />
              {narrow ? null : "Settings"}
            </button>
            <button type="button" className="k-btn" onClick={() => onOpen("sizes")}>
              <KIcon name="folder-open" size={15} />
              Open file…
            </button>
          </div>
        </div>
        <div className="k-chips" role="tablist" aria-label="Shelves">
          {SHELVES.map((s) => (
            <button key={s} type="button" role="tab" aria-selected={shelf === s} className="k-chip" data-on={shelf === s || undefined} onClick={() => setShelf(s)}>
              {s}
            </button>
          ))}
          <span className="k-chip k-chip--new">+ Collection</span>
        </div>
      </div>
      <div className="k-home__body">
        {shelf === "Recent" ? (
          <>
            <h3 className="k-home__section">Continue reading</h3>
            <div className="k-continue">
              {continuing.map((d) => (
                <button key={d.id} type="button" className="k-ccard" onClick={() => onOpen(d.id)}>
                  <Cover title={d.title} width={56} />
                  <span className="k-ccard__text">
                    <b>{d.title}</b>
                    <span>{d.author}</span>
                    <span className="k-grow" />
                    <Progress value={d.progress} />
                    <small>{Math.round(d.progress * 100)} % read</small>
                  </span>
                </button>
              ))}
            </div>
          </>
        ) : null}
        <h3 className="k-home__section">{shelf}</h3>
        {items.length ? (
          <div className="k-cards">
            {items.map((d) => (
              <button key={d.id} type="button" className="k-card" onClick={() => onOpen(d.id)}>
                <span className="k-card__cover">
                  <Cover title={d.title} width={132} />
                  {d.favourite ? <span className="k-ribbon" aria-label="Favourite" /> : null}
                  <span className="k-badge">{d.format}</span>
                </span>
                <span className="k-card__title" title={d.title}>
                  {d.title}
                </span>
                <span className="k-card__author">{d.author ?? ""}</span>
                <Progress value={d.progress} />
              </button>
            ))}
          </div>
        ) : (
          <p className="k-home__empty">Nothing here yet.</p>
        )}
      </div>
    </div>
  );
}
