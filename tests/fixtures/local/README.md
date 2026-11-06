# Local reference PDFs

Real-world PDFs from the maintainer's drive, used to check the engine and the benchmarks against documents nobody generated for us. They are copyrighted (books, cheat sheets, papers), so they are **never committed**. Only this README is.

- `local-corpus.json` (git-ignored) lists each file's absolute path, SHA-256, page count, size and the case it covers. It is written by hand or by a one-off script; regenerate it on another machine with your own files.
- The files are **read-only to Kitab's tooling.** Tests and benches open them in place. Nothing may copy them into the repo, move, modify or delete them.
- Tests that use them are opt-in: they skip when `local-corpus.json` is missing, and fail if a file's hash changed (so a result is never silently compared against a different file).
- They supplement `tests/fixtures/` and `expected.json`; they never replace a synthetic fixture. A behaviour found here gets a synthetic fixture via `tools/generate_fixtures.py` when one can be made.

Cases covered by the current set (L01–L10): a 462-page book with two page sizes and a 384-entry outline; a scanned-style book with 11 page sizes and non-embedded fonts; a file with a damaged xref that MuPDF repairs; a 244-page text book for find and outline; a landscape page with about 5,100 vector paths; text drawn as outlines with no text layer; a single full-page image at an odd size; a dense PDF 1.2 reference card; an academic paper with non-embedded fonts; and an 88-page technical manual heavy with diagrams and photos.
