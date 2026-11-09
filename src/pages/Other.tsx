// SPDX-License-Identifier: AGPL-3.0-or-later
import { PLATFORMS, SHORTCUTS, SITE } from "../content";
import type { PageData } from "../data";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/Layout";

function bytes(n: number): string {
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(n / 1024))} KB`;
}

export function Download({ data }: { data: PageData }) {
  const r = data.release;
  return (
    <>
      <PageHeader
        kicker="Download"
        title={r ? `Kitab Reader ${r.tag.replace(/^v/, "")}` : "Not released yet"}
        lead={
          r ? (
            <>
              Published {r.publishedAt.slice(0, 10)} on GitHub Releases.{r.prerelease ? " This is a pre-release." : ""}
            </>
          ) : (
            <>
              Kitab is in development and there is nothing to install yet. The first builds will be published on GitHub
              Releases. Watch the repository to be told when they land.
            </>
          )
        }
      />
      <div className="mx-auto max-w-6xl px-5 py-12">
        {r ? (
          <div className="overflow-hidden rounded-2xl ring-1 ring-line">
            <ul className="divide-y divide-line">
              {r.assets.map((a) => (
                <li key={a.name} className="flex items-center gap-4 px-5 py-3">
                  <Icon name="file" size={16} className="text-faint" />
                  <a href={a.url} className="flex-1 truncate font-mono text-[13px] text-text hover:underline">
                    {a.name}
                  </a>
                  <span className="tnum text-[12.5px] text-faint">{bytes(a.size)}</span>
                </li>
              ))}
            </ul>
          </div>
        ) : (
          <div className="flex flex-wrap gap-3">
            <a
              href={SITE.repo}
              className="inline-flex h-10 items-center gap-2 rounded-xl bg-text px-4 text-[14px] font-medium text-bg shadow-elev hover:opacity-90"
            >
              <Icon name="github" size={16} /> Watch on GitHub
            </a>
            <a href={SITE.releases} className="inline-flex h-10 items-center gap-2 rounded-xl bg-raised px-4 text-[14px] font-medium text-text shadow-elev hover:bg-hover">
              GitHub Releases <Icon name="arrow" size={14} />
            </a>
          </div>
        )}

        <h2 className="mt-14 text-[18px] font-semibold text-text">Platforms</h2>
        <div className="mt-4 overflow-x-auto rounded-2xl ring-1 ring-line">
          <table className="w-full min-w-[520px] border-collapse text-left text-[14px]">
            <thead>
              <tr className="bg-panel text-[12.5px] text-faint">
                <th scope="col" className="px-5 py-3 font-medium">System</th>
                <th scope="col" className="px-5 py-3 font-medium">Packages</th>
                <th scope="col" className="px-5 py-3 font-medium">When</th>
              </tr>
            </thead>
            <tbody>
              {PLATFORMS.map((p) => (
                <tr key={p.os} className="border-t border-line">
                  <th scope="row" className="px-5 py-3 font-medium text-text">{p.os}</th>
                  <td className="px-5 py-3 text-muted">{p.formats}</td>
                  <td className="px-5 py-3 text-muted">{p.when}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="mt-4 max-w-2xl text-[14px] text-faint">
          Homebrew and Scoop install it today; WinGet and Flathub listings are planned for version 1.0. The builds are not
          code-signed yet, so Windows SmartScreen and macOS Gatekeeper may warn before the first run (the{" "}
          <a href="/docs#first-run" className="underline decoration-accent underline-offset-2">docs</a> say what to do). Every release lists SHA-256 checksums.
        </p>
      </div>
    </>
  );
}

export function Shortcuts() {
  return (
    <>
      <PageHeader
        kicker="Keyboard"
        title="Keyboard shortcuts"
        lead={
          <>
            Kitab is built to be used from the keyboard. These are the defaults, read from Kitab's own list. Change any of them in{" "}
            <code className="rounded bg-hover px-1 font-mono text-[0.9em] shadow-well">keymap.json</code> (Settings › Shortcuts › Edit keymap.json). The keys are the same on Windows, macOS and Linux. The{" "}
            <a href="/docs#keyboard" className="underline decoration-accent underline-offset-2">docs</a> cover every command.
          </>
        }
      />
      <div className="mx-auto grid max-w-6xl gap-6 px-5 py-12 lg:grid-cols-2">
        {SHORTCUTS.map((g) => (
          <section key={g.area} aria-labelledby={`k-${g.area}`} className="rounded-2xl ring-1 ring-line">
            <h2 id={`k-${g.area}`} className="border-b border-line bg-panel px-5 py-3 text-[13px] font-semibold text-text first:rounded-t-2xl">
              {g.area}
            </h2>
            <dl className="divide-y divide-line">
              {g.keys.map(([k, what]) => (
                <div key={k} className="flex items-center justify-between gap-4 px-5 py-2.5">
                  <dt className="text-[14px] text-muted">{what}</dt>
                  <dd>
                    <kbd className="whitespace-nowrap rounded-md bg-raised px-2 py-0.5 font-mono text-[12px] text-text shadow-elev">{k}</kbd>
                  </dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </>
  );
}

export function MarkdownPage({ kicker, title, lead, data }: { kicker: string; title: string; lead: string; data: PageData }) {
  return (
    <>
      <PageHeader kicker={kicker} title={title} lead={lead} />
      <div className="mx-auto max-w-6xl px-5 py-10">
        <div className="prose-k" dangerouslySetInnerHTML={{ __html: data.html ?? "" }} />
      </div>
    </>
  );
}

export function NotFound() {
  return (
    <div className="mx-auto flex max-w-xl flex-col items-center px-5 py-28 text-center">
      <Icon name="logo" size={56} className="text-ring" />
      <h1 className="mt-6 text-[22px] font-semibold text-text">This page isn't here</h1>
      <p className="mt-2 text-[15px] text-muted">The link may be old, or the page may have moved.</p>
      <a href="/" className="mt-6 inline-flex h-9 items-center gap-2 rounded-xl bg-raised px-4 text-[14px] text-text shadow-elev hover:bg-hover">
        Go to the home page
      </a>
    </div>
  );
}
