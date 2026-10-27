# Codex

A desktop app for macOS and Windows for browsing and organizing a personal PDF library:
real first-page thumbnails, BibTeX/Zotero metadata enrichment, reading
progress tracking, and a live folder watcher that keeps the library in
sync with your filesystem as you add, edit, and remove files. Manual
metadata edits write back into the PDF itself and can rename the file to
match.

Built with Tauri 2 + React 19 + TypeScript/Vite on the frontend, Rust on
the backend, SQLite for the library index.

## Install (just want to use it)

Download from the latest [Release](https://github.com/ldpelletier-afk/Noodle-Codex/releases/latest).

### Mac

Grab the `.dmg`, open it, and drag Codex.app to Applications. Runs on both
Apple Silicon (M-series) and Intel Macs.

**First launch:** Codex isn't signed with an Apple Developer ID, so macOS
blocks it the first time ("can't be opened" or "is damaged"). Either open
**System Settings → Privacy & Security** and click **Open Anyway**, or run
this once in Terminal:

```sh
xattr -cr /Applications/Codex.app
```

### Windows

Grab the `..._x64-setup.exe` and run it. Requires Windows 10 or 11 (64-bit).

**First launch:** Codex isn't code-signed, so Windows SmartScreen may show
"Windows protected your PC". Click **More info**, then **Run anyway**.

"Open PDF" opens files in your default PDF app (Edge, Acrobat, …).

### PDF reader (Mac)

"Open PDF" uses [Skim](https://skim-app.sourceforge.io) if
it's installed (and reopens at the page you left off on), otherwise your
default PDF app — Preview, Acrobat, whatever you've set in Finder. To always
use your default app even with Skim installed, pick **My default PDF app**
under **Settings → Opening PDFs**.

A short walkthrough runs on first launch; replay it any time from
**Settings → Walkthrough**.

## Building from source

Prerequisites (Mac; on Windows, Visual Studio's "Desktop development with
C++" workload replaces the Xcode tools):

- [Node.js](https://nodejs.org) 20+
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- Xcode Command Line Tools (`xcode-select --install`)

```sh
git clone https://github.com/ldpelletier-afk/Noodle-Codex.git
cd Noodle-Codex
npm install
```

Run in dev mode (hot reload):

```sh
npm run tauri dev
```

Build a release `.app`/`.dmg`:

```sh
npm run tauri build
```

Output lands in `src-tauri/target/release/bundle/`. That build only runs on
your own Mac's chip type; for one that runs on both Apple Silicon and Intel:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run release:mac
```

Output lands in `src-tauri/target/universal-apple-darwin/release/bundle/`.

### Releasing

Pushing a version tag builds a universal `.dmg` on GitHub and attaches it to
a draft release (see `.github/workflows/release.yml`):

```sh
git tag v0.2.0
git push origin v0.2.0
```

To skip the first-run walkthrough in your own builds, create `.env.local`
containing `VITE_DISABLE_TOUR=1` (it's git-ignored, so releases keep it).
