# Decisions

Newest first. Each entry: date, decision, why, alternatives considered.

## 2026-10-13: Release builds (task 3)

- **Decision:** A tag `v*` triggers `.github/workflows/release.yml`. It builds Linux
  x86_64, Windows x86_64 and macOS (Apple chip and Intel), packs each with the README
  (`.zip` on Windows, `.tar.gz` elsewhere so the file stays executable), zips `extension/`,
  and publishes with GitHub's own `gh` tool. If the release already exists (made in the web
  UI), files are attached to it. The daemon version goes to 0.2.0 to match the first tag.
- **Why:** `gh` is preinstalled on GitHub's runners, so no third-party release action is
  needed. Pull requests that touch the workflow build without publishing, so it gets tested.
- **Alternatives:** A third-party action such as `softprops/action-gh-release`; Linux ARM
  builds (need cross-compilation, can be added later); signing builds (needs paid
  certificates from Apple/Microsoft, owner decision).

## 2026-10-13: Extension background worker (task 2)

- **Decision:** A background service worker syncs the proxy with the stored toggle state
  when the browser starts and when the extension is installed or updated. The proxy
  on/off code moved to a shared `extension/proxy.js` used by both the popup and the worker.
- **Why:** One copy of the proxy settings means the popup and the worker can't drift
  apart. Running on install/update as well means reloading the extension also clears a
  leftover proxy setting when the toggle is OFF.
- **Alternatives:** Popup sends a message to the worker, which does all the work (more
  moving parts); duplicating the proxy settings in both files.

## 2026-10-13: Task 1 details (daemon cleanup)

- **Decision:** A domain name that is empty or not valid UTF-8 now gets the SOCKS reply
  "host unreachable" (same as any other refused host). The `.anon` check ignores case and
  one trailing dot (`test.anon.`). The README is not changed in this PR; task 2 already
  covers syncing it (startup message, `--listen` flag, how to run tests).
- **Why:** "Host unreachable" is what Air-Gap Mode already sends for refused hosts, so the
  browser behaves the same either way. Leaving the README to task 2 keeps the PRs
  separate as the roadmap plans.
- **Alternatives:** SOCKS "general failure" (0x01) for bad names; updating the README now.

## 2026-10-13: Clearnet stays blocked for now (Air-Gap Mode)

- **Decision:** Non-`.anon` traffic is refused. The README's older claim that normal sites
  are forwarded is out of date and gets fixed.
- **Why:** The long-term idea is that normal websites become reachable through other nodes
  in the network. How that should work (and who carries the legal risk of being an exit
  node) is undecided, so nothing is forwarded until then.
- **Alternatives:** Forward clearnet directly from the local daemon (this gives no privacy
  and leaks the user's real IP to the sites they visit).

## 2026-10-13: `.anon` address = peer public key; Phase 3 tunnels HTTP over Iroh

- **Decision:** A `.anon` name is the peer's 32-byte public key in lowercase base32
  (52 characters). Visiting it tunnels the browser's bytes to that peer over an Iroh
  connection. Friendly names and `iroh-blobs`/`iroh-docs` storage come later.
- **Why:** Needs no registry and no extra infrastructure, and a tunnel is the smallest
  thing that serves a real site end to end. Base32 rather than hex because 64 hex
  characters are over the 63-character DNS label limit.
- **Alternatives:** Petname file (each user keeps their own list); content-addressed
  sites via `iroh-blobs` (more moving parts, deferred). This changes the README's Phase 3
  wording, which mentioned `iroh-blobs` and `iroh-docs` first.
