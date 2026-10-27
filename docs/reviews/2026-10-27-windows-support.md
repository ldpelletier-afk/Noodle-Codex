# Review: Windows support (v0.3.0)

Reviewed: the Windows-support changes (`1481aac`, `1c8984c`, `69f0f75`) plus the
existing code they depend on. Fixes landed in v0.3.1.

## How it was checked

- Read the full diff, then searched the backend and frontend for anything that
  assumes `/` path separators, macOS-only tools (`qlmanage`, `osascript`,
  `open`), or macOS-only behaviour.
- Ran the Rust suite on macOS (62 tests) and on a GitHub `windows-latest`
  runner (61; the symlink test is Unix-only). The Windows run includes real
  first-page renders through `Windows.Data.Pdf` and, for the new tests, a PDF
  held open with no sharing — the same lock Acrobat/Edge take.
- Built the v0.3.0 release in CI and checked its assets.

## Findings

### Fixed

1. **Temp files leaked into the user's folder (Windows).** `write_info_fields`
   rewrites a PDF to `.codex-tmp-*.pdf` and renames it over the original. If
   that final rename failed, the temp copy was left behind. On macOS the dot
   prefix hides it; Windows doesn't hide dot-files, and the rename fails there
   whenever another app has the PDF open. Now removed on every failure path.
   Test: `failed_pdf_rewrite_leaves_no_temp_file_behind`.

2. **A failed rename threw away the whole edit.** Renaming a PDF that's open in
   another app fails on Windows; the edit then errored with "metadata saved, but
   couldn't rename file" although nothing had been saved. The edit is now saved
   to Codex under the current file name and a warning explains why the file
   wasn't renamed. Test:
   `perform_document_edit_keeps_the_edit_when_the_file_cant_be_renamed`.

3. **Unhelpful error text.** Windows' "being used by another process (os error
   32)" is now "open in another app — close it there and try again".

4. **Release race.** The macOS and Windows build jobs each created the draft
   release if missing; finishing together, they could create two drafts with
   half the installers each. A `create-release` job now creates (or finds) the
   draft first and both builds upload into it by id. (v0.3.0 happened not to
   hit this.)

5. **Live deletions missed on Windows** (fixed during the v0.3.0 work, listed
   for completeness). `canonicalize` returns `\\?\C:\Users\long-name\…` on
   Windows while the watcher reports paths as watched; a deleted file can't be
   canonicalized, so it never matched its folder. Its parent is canonicalized
   instead. Test: `resolve_tracked_folder_matches_a_deleted_file`.

### Checked, no change needed

- **Path handling.** Folder membership and overlap checks use `Path::starts_with`
  (component-wise, separator-agnostic); folder browsing uses `relativeDir`,
  which the backend always builds with `/`. Only two UI spots split on `/`
  (file names in toasts/settings) — fixed in v0.3.0 via `baseName`.
- **Skim / AppleScript.** `skim_installed()` is compiled out off macOS, so
  `open_document` never runs `osascript` and never starts the page tracker.
- **Thumbnails.** Same retry/timeout/cache logic on both platforms; Windows
  renders to a per-call scratch dir and is moved into place like Quick Look's
  output.
- **Asset protocol.** Thumbnails live under the app's data dir, inside the
  configured scope on Windows too.
- **Installer naming.** CI produced `Codex_0.3.0_x64-setup.exe`, matching the
  README.

### Known limitations (not fixed)

- If Windows' PDF engine ever hangs on a malformed file, the abandoned render
  thread may keep that PDF open until Codex quits. The 8 s timeout keeps the
  thumbnail job moving; no hang has been observed.
- Windows path comparisons are case-sensitive. Paths come from the system
  folder picker with consistent casing, so this shouldn't arise in practice.
- The app itself hasn't been run interactively on Windows — only the test
  suite and the release build. A first real-world try is the remaining check.
