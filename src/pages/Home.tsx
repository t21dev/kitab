// SPDX-License-Identifier: AGPL-3.0-or-later
// Layout: mockup A (Feature Stack: a sticky stage beside the story). Copy: mockup B.
// The stage shows only real captures of the app; a feature with none yet says which phase brings it.
import { useEffect, useState } from "react";
import { FAQ, ROADMAP, SITE, SPECS, STEPS, type Step } from "../content";
import type { PageData } from "../data";
import { FEATURE_ICONS, Icon } from "../components/Icon";

function Hero({ data }: { data: PageData }) {
  return (
    <section className="hero wrap" aria-labelledby="hero-title">
      <p className="status rise" style={{ "--i": 0 } as React.CSSProperties}>
        <i />
        {data.release ? `Version ${data.release.tag.replace(/^v/, "")} is out` : `In development · ${SITE.phase}`}
      </p>
      <h1 id="hero-title" className="rise" style={{ "--i": 1 } as React.CSSProperties}>
        Read beautifully.
        <br />
        <span>Work efficiently.</span>
      </h1>
      <p className="lede rise" style={{ "--i": 2 } as React.CSSProperties}>
        Kitab is a reader for PDF, EPUB, Markdown and comics. It is native and drawn on the GPU, it opens in a blink, and it
        keeps your files on your computer.
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
    </section>
  );
}

function Shot({ step, eager = false }: { step: Step; eager?: boolean }) {
  const phase = ROADMAP.find((p) => p.n === step.phase);
  if (!step.shot) {
    return (
      <div className="pending" role="note">
        <Icon name={FEATURE_ICONS[step.id] ?? "file"} size={22} />
        <p className="pending__title">Screenshot arrives with Phase {step.phase}</p>
        <p className="pending__body">
          {phase ? `${phase.title} (${phase.version}).` : ""} This panel only ever shows real captures of the app, taken from
          the build itself, so it stays empty until the feature exists.
        </p>
      </div>
    );
  }
  const s = step.shot;
  return (
    <figure className="shot">
      <span className="shot__frame">
        <img className="only-light" src={`/shots/light/${s.file}`} alt={s.alt} width={s.width} height={s.height} loading={eager ? "eager" : "lazy"} decoding="async" />
        <img className="only-dark" src={`/shots/dark/${s.file}`} alt={s.alt} width={s.width} height={s.height} loading={eager ? "eager" : "lazy"} decoding="async" />
      </span>
      <figcaption>{s.caption}</figcaption>
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
              <Shot step={s} />
            </div>
          </li>
        ))}
      </ol>
      <div className="stage" aria-hidden="true">
        {STEPS.map((s, i) => (
          <div key={s.id} className="stage__scene" data-on={active === s.id || undefined}>
            <Shot step={s} eager={i < 2} />
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
            Kitab is in its first phase: the foundations, and a test of how fast the interface can start and draw the first
            page. Every phase after that ends with its tests passing and a release. The <a href={SITE.plan}>plan is public</a>,
            and so is every line of code.
          </p>
        </div>
        <ol className="phases">
          {ROADMAP.map((p) => (
            <li key={p.n} data-state={p.state}>
              <span className="phases__dot" aria-hidden="true" />
              <div>
                <h3>
                  Phase {p.n}: {p.title} <span className="mono">{p.version}</span>
                  {p.state === "now" ? <span className="now">Now</span> : null}
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
