// SPDX-License-Identifier: AGPL-3.0-or-later
// Layout: mockup A (Feature Stack: a sticky stage beside the story). Copy: mockup B.
// The stage shows Kitab itself, rebuilt in HTML from the app's own tokens, icons and sizes
// (src/mock), so each feature can be tried right here.
import { useEffect, useState } from "react";
import { FAQ, ROADMAP, SITE, SPECS, STEPS, type Step } from "../content";
import { KitabApp } from "../mock/KitabApp";
import { HERO, SCENES } from "../mock/scenes";
import type { PageData } from "../data";
import { FEATURE_ICONS, Icon } from "../components/Icon";

function Hero({ data }: { data: PageData }) {
  return (
    <section className="hero wrap" aria-labelledby="hero-title">
      <div className="hero__text">
        <p className="status rise" style={{ "--i": 0 } as React.CSSProperties}>
          <i />
          {`${data.release ? data.release.tag.replace(/^v/, "") : SITE.latest} is out · ${SITE.latestNote}`}
        </p>
        <h1 id="hero-title" className="rise" style={{ "--i": 1 } as React.CSSProperties}>
          {SITE.tagline}
        </h1>
        <p className="lede rise" style={{ "--i": 2 } as React.CSSProperties}>
          <span className="lede__lead">{SITE.subtitle}</span> A reader for PDF, EPUB, Markdown and comics, written in Rust and
          drawn on the GPU with GPUI, with MuPDF rendering the pages and no browser engine inside. Your files stay on your
          computer.
        </p>
        <div className="ctas rise" style={{ "--i": 3 } as React.CSSProperties}>
          <a className="btn btn--primary" href={SITE.repo}>
            <Icon name="github" size={16} /> Follow on GitHub
          </a>
          <a className="btn btn--raised" href="/download">
            {data.release ? "Download" : "Release status"}
          </a>
        </div>
        <ul className="facts rise" style={{ "--i": 4 } as React.CSSProperties}>
          <li>
            <b>Free</b> · AGPL-3.0
          </li>
          <li>
            <b>Windows, macOS</b> and <b>Linux</b>
          </li>
          <li>
            <b>No</b> account, no telemetry
          </li>
        </ul>
      </div>
      <HeroShot />
    </section>
  );
}

/** The product shot beside the hero text: a book open in Kitab, in the same HTML window as the
 * feature stage, so it can be used right here. */
function HeroShot() {
  return (
    <div className="hero__shot rise" style={{ "--i": 3 } as React.CSSProperties}>
      <div className="hero__tilt">
        <KitabApp start={HERO.start} label={HERO.label} />
      </div>
    </div>
  );
}

function Scene({ step }: { step: Step }) {
  const scene = SCENES[step.id];
  if (!scene) return null;
  return (
    <figure className="shot">
      <div className="shot__frame">
        <KitabApp start={scene.start} label={scene.label} />
      </div>
      <figcaption>{scene.hint}</figcaption>
    </figure>
  );
}

function Features() {
  const [active, setActive] = useState(STEPS[0].id);
  useEffect(() => {
    const io = new IntersectionObserver(
      (entries) => entries.forEach((e) => e.isIntersecting && setActive((e.target as HTMLElement).dataset.step ?? STEPS[0].id)),
      { rootMargin: "-45% 0px -45% 0px" },
    );
    document.querySelectorAll<HTMLElement>("[data-step]").forEach((el) => io.observe(el));
    return () => io.disconnect();
  }, []);
  return (
    <section id="features" className="stack wrap" aria-label="Features">
      <ol className="story">
        {STEPS.map((s) => (
          <li key={s.id} id={s.id} data-step={s.id} className="step" data-active={active === s.id || undefined}>
            <span className="step__icon">
              <Icon name={FEATURE_ICONS[s.id] ?? "file"} size={17} />
            </span>
            <h2>{s.title}</h2>
            {s.body.map((p) => (
              <p key={p.slice(0, 24)}>{p}</p>
            ))}
            {s.points ? (
              <ul>
                {s.points.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            ) : null}
            <div className="step__shot">
              <Scene step={s} />
            </div>
          </li>
        ))}
      </ol>
      <div className="stage">
        {STEPS.map((s) => (
          <div key={s.id} className="stage__scene" data-on={active === s.id || undefined} inert={active !== s.id}>
            <Scene step={s} />
          </div>
        ))}
      </div>
    </section>
  );
}

function Compare() {
  return (
    <section id="compare" className="band" aria-labelledby="compare-title">
      <div className="wrap">
        <h2 id="compare-title" className="band__title">
          What's inside
        </h2>
        <p className="band__lede">One app for reading and working with documents, built natively for each system.</p>
        <div className="table-wrap">
          <table className="specs">
            <caption className="sr-only">What Kitab Reader includes</caption>
            <tbody>
              {SPECS.map((r) => (
                <tr key={r.what}>
                  <th scope="row">{r.what}</th>
                  <td>{r.value}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

function Roadmap() {
  return (
    <section id="roadmap" className="band" aria-labelledby="roadmap-title">
      <div className="wrap band__grid">
        <div>
          <h2 id="roadmap-title" className="band__title">
            Where it stands
          </h2>
          <p className="band__lede">
            Two releases are out. 0.1.0 brought the reader, books, Markdown, markup, printing, signatures and the PDF tools;
            0.2.0 added each system's own print dialog, more downloads and a Home built for big libraries. What is left before
            1.0.0 is the last phase: accessibility, the published benchmark, signed installers, winget and Flathub. The{" "}
            <a href={SITE.plan}>plan is public</a>, and so is every line of code.
          </p>
        </div>
        <ol className="phases">
          {ROADMAP.map((p) => (
            <li key={p.n} data-state={p.state}>
              <span className="phases__dot" aria-hidden="true" />
              <div>
                <h3>
                  Phase {p.n}: {p.title} {p.release ? <span className="mono">{p.release}</span> : null}
                  {p.state === "now" ? <span className="now">Now</span> : null}
                  {p.state === "done" ? <span className="now now--done">Shipped</span> : null}
                </h3>
                <p>{p.summary}</p>
              </div>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

function Faq() {
  return (
    <section id="faq" className="band" aria-labelledby="faq-title">
      <div className="wrap">
        <h2 id="faq-title" className="band__title">
          Questions
        </h2>
        <div className="faq">
          {FAQ.map((f) => (
            <details key={f.q}>
              <summary>
                <h3>{f.q}</h3>
                <Icon name="close" size={14} className="faq__x" />
              </summary>
              <p>{f.a}</p>
            </details>
          ))}
        </div>
      </div>
    </section>
  );
}

function Closing() {
  return (
    <section className="band closing" aria-label="Closing note">
      <div className="wrap">
        <p className="closing__line">
          Thanks for reading this far.
          <br />
          <b>Team 21 and TriptoAfsin</b>
        </p>
        <p className="closing__ps">
          P.S. If a PDF opens badly in other readers, keep it. When Kitab has a release, it is exactly the file we want in an issue.
        </p>
        <div className="ctas">
          <a className="btn btn--primary" href={SITE.repo}>
            <Icon name="github" size={16} /> Follow on GitHub
          </a>
          <a className="btn btn--raised" href={SITE.plan}>
            Read the plan
          </a>
        </div>
      </div>
    </section>
  );
}

export function Home({ data }: { data: PageData }) {
  return (
    <>
      <Hero data={data} />
      <Features />
      <Compare />
      <Roadmap />
      <Faq />
      <Closing />
    </>
  );
}
