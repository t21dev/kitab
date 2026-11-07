# Privacy

Kitab Reader is local-first. This page says exactly what it does with your data.

- **Your files stay on your computer.** Kitab reads documents from your disk and never uploads them.
- **No telemetry.** Kitab does not collect usage data, analytics or crash reports. If it crashes, a report is written to the `crash` folder in its data folder, and goes nowhere unless you send it yourself.
- **Update check, on unless you turn it off.** Ten seconds after Kitab opens, and every six hours while it stays open, it asks GitHub's public API (`api.github.com/repos/t21dev/kitab/releases/latest`) for the latest release number and its notes. The request carries nothing about you or your files; GitHub sees your IP address, as with any web request. Turn it off in Settings › About › Check on start-up; Check now still works when you press it. Kitab never downloads or installs an update by itself: Download opens the release page in your browser.
- **Nothing else goes out unless you turn it on.** Each of these says so where you turn it on:
  - **Remote images** in a Markdown or EPUB file: per file, when you allow them.
  - **Online revocation checks** for digital signatures: contact the certificate authority's OCSP or CRL server named in the certificate.
  - **Links** you click open in your browser after Kitab asks the first time for each site.
- **Signing keys stay local.** Kitab uses your certificate files or your system's certificate store to sign on your computer. It never sends keys or passwords anywhere and never stores a password unless you ask it to remember it in your system's credential store.
- **What Kitab stores, in its data folder:** your library list, reading positions, tabs, bookmarks, highlights and notes, settings, a cache of page thumbnails, and a search index of your books' text, and the certificates you chose to trust for signatures (`trusted-certs/`). Delete the folder and all of it is gone. In portable mode, the folder sits next to the app.

| Platform | Data folder |
|---|---|
| Windows | `%APPDATA%\t21\Kitab` |
| macOS | `~/Library/Application Support/dev.t21.kitab` |
| Linux | `$XDG_DATA_HOME/kitab` (usually `~/.local/share/kitab`) |

Questions: open an issue at https://github.com/t21dev/kitab.
