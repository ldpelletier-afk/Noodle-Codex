# Codex

A macOS desktop app for browsing and organizing a personal PDF library:
real first-page thumbnails, BibTeX/Zotero metadata enrichment, reading
progress tracking, and a live folder watcher that keeps the library in
sync with your filesystem as you add, edit, and remove files. Manual
metadata edits write back into the PDF itself and can rename the file to
match.

Built with Tauri 2 + React 19 + TypeScript/Vite on the frontend, Rust on
the backend, SQLite for the library index.

## Install (just want to use it)

Grab the latest `.dmg` from [Releases](https://github.com/ldpelletier-afk/Noodle-Codex/releases),
open it, and drag Codex.app to Applications.

**Requires Apple Silicon (M-series) macOS.** Not built for Intel Macs.

## Building from source

Prerequisites:

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

Output lands in `src-tauri/target/release/bundle/`.
