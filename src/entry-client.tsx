// SPDX-License-Identifier: AGPL-3.0-or-later
import { hydrateRoot } from "react-dom/client";
import { App } from "./App";
import type { PageData } from "./data";
import "./styles.css";

const root = document.getElementById("root");
const raw = document.getElementById("__data")?.textContent;
if (root && raw) {
  // The prerender step stores the route it rendered, so 404 pages hydrate as 404 at any URL.
  const { path, data } = JSON.parse(raw) as { path: string; data: PageData };
  hydrateRoot(root, <App path={path} data={data} />);
}

// Bands below the first screen settle in once as they arrive. Anything already on screen stays as it is.
const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
const io = new IntersectionObserver(
  (entries) =>
    entries.forEach((e) => {
      if (!e.isIntersecting) return;
      e.target.classList.remove("pre");
      io.unobserve(e.target);
    }),
  { rootMargin: "0px 0px -8% 0px" },
);
document.querySelectorAll<HTMLElement>(".band > .wrap").forEach((el) => {
  if (el.getBoundingClientRect().top > innerHeight) {
    el.classList.add("reveal", "pre");
    io.observe(el);
  }
});
if (reduce) document.querySelectorAll(".pre").forEach((el) => el.classList.remove("pre"));
