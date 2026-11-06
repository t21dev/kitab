# Kitab Reader website

This is the `landing` branch of [t21dev/kitab](https://github.com/t21dev/kitab). It holds **only** the website: the landing page, download page, privacy policy, changelog and shortcuts. The app lives on `main`.

- Stack: React + TypeScript + Vite + Tailwind CSS v4, deployed to Cloudflare Workers (static assets, `@cloudflare/vite-plugin`, `wrangler.jsonc`).
- Deploys: Cloudflare Workers Builds watches this branch. A push here deploys.
- This branch is an orphan. Never merge it with `main`. Content that comes from the app (CHANGELOG, PRIVACY, performance results, screenshots) is copied in by the release workflow, not edited by hand.
- Design follows `DESIGN.md` on `main`. No trackers, no third-party scripts, no invented numbers: every figure is a measured one with its source.

Status: placeholder. The site is scaffolded in Phase 0 (see `docs/PLAN.md` §10 on `main`).
