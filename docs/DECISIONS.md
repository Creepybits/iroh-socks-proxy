# Decisions

Newest first. Each entry: date, decision, why, alternatives considered.

## 2026-10-13: Say plainly that `.anon` is encrypted, not anonymous

- **Decision:** README and roadmap now state that connections are end-to-end encrypted but
  direct, so sites see visitors' IP addresses, a site's host can be found, n0's servers see
  who connects to whom, and one key per daemon lets sites link visits. "Anonymity" options
  are listed under "Later" (separate visiting key, routing over Tor, own multi-hop routing).
- **Why:** The owner asked whether this is as anonymous as Tor and had assumed a bigger
  network would make it anonymous by default. It doesn't: anonymity comes from routing
  through other peers, which isn't built. The name `.anon` and the earlier "much like a Tor
  onion service" wording could mislead users who need real anonymity.
- **Alternatives:** Renaming the `.anon` TLD (bigger change, owner's call; not done).

## 2026-10-13: Docker image and bunny.net hosting (task 7)

- **Decision:** A two-stage `Dockerfile` at the repository root: build with the official
  `rust:1.97-slim-trixie` image, run on `debian:trixie-slim`. The CA certificate bundle is
  copied from the build image (no `apt` in the runtime stage). The image publishes the
  repo's `site/` folder by default (`--serve /site`) and keeps the key in `/data`. An
  entrypoint script takes ownership of `/data` when started as root (hosting platforms
  often mount root-owned volumes) and then drops to the unprivileged `sovereign` user with
  `setpriv`. Published to `ghcr.io/creepybits/iroh-socks-proxy` (linux/amd64, which
  bunny.net requires) with tags `latest`, the version, and the commit.
- **Why:** The owner wanted an always-on site without running their own computer, and
  already has a bunny.net account; Magic Containers runs Docker images and has persistent
  volumes. ghcr.io needs no extra account or secrets (the workflow uses GitHub's built-in
  token). Docker also makes running the daemon easier for others.
- **Hosting rule:** exactly one region and one instance. Each instance gets its own blank
  volume, so a second instance would create a second key and a different address.
- **Not verified:** whether bunny's dashboard requires an endpoint for an app that only
  connects outward; the guide says what to pick if it does (Anycast, never a public
  endpoint for the SOCKS port).
- **Alternatives:** Docker Hub (extra account and secret); a distroless/scratch image
  (smaller, but no shell for the ownership fix and harder to debug); Fly.io or a VPS
  (also fine, the image works anywhere).

## 2026-10-13: Security review fixes and limits (task 6)

- **Reviewed:** SOCKS parsing, routing, the tunnel and serving code, the extension, and
  anything that could leak clearnet traffic or DNS lookups.
- **Fixed:**
  - SOCKS handshake (and the landing page's first read) time out after 10 s; previously a
    silent client held its task forever.
  - At most 256 browser connections at once (more wait), and at most 64 visiting peers at
    once (more are refused immediately, so strangers can't make us queue).
  - `--forward` gives up connecting to the local server after 5 s.
  - Hidden files and folders (`.git`, `.env`, …) are never served by `--serve`.
  - One theoretical panic spot (slicing the key string) replaced with a non-panicking form.
  - Tests use tokio's `test-util` explicitly instead of relying on a dependency enabling it.
- **Checked, no change needed:** no `unwrap`/`expect` on network or browser data; the daemon
  never resolves DNS for the browser (Chrome sends the host name to a SOCKS5 proxy) and only
  ever connects out to Iroh and the loopback `--forward` target; the extension's bypass
  list is empty and WebRTC is restricted; error pages don't echo requests; logs contain
  host names only, never paths or contents.
- **Limits chosen:** 256/64/10 s/8 KiB are generous for one person browsing or a small
  personal site, and small enough to keep a laptop responsive under abuse.
- **Left for later** (listed in the roadmap): idle timeouts after the handshake, per-peer
  quotas, incognito support, a daemon-status indicator, and encrypting the key at rest.

## 2026-10-13: Serving a site: both `--serve` and `--forward` (task 5)

- **Decision:** Both options, mutually exclusive.
  - `--serve <folder>`: a minimal built-in HTTP/1.1 file server (`daemon/src/site.rs`). One
    GET per tunnel, `Connection: close`, `index.html` as the default page, a small
    extension → content-type table, 8 KiB / 10 s limit on request headers. Paths are
    percent-decoded, then refused if they contain `..`, backslashes, NUL or anything the OS
    treats as more than a plain name (e.g. `C:`); after resolving symlinks the file must
    still be inside the folder. Errors are plain 4xx pages.
  - `--forward <addr>`: copies the tunnel byte-for-byte to a local web server. Only
    loopback addresses are accepted, so publishing can't make the daemon a gateway to other
    machines or the internet (that would weaken Air-Gap Mode from the other side).
  - Incoming tunnels finish their handshake in their own task (a slow peer can't block
    others). After answering, the publisher waits up to 10 s for the visitor to hang up so
    the last bytes aren't cut off.
- **Why:** `--serve` is what a non-developer needs (no other software). `--forward` costs a
  few lines and lets anyone publish a real web app; the owner knows Python, so
  `python -m http.server` + `--forward` is a natural path too.
- **Alternatives:** A web-server crate such as `hyper`/`axum` (more dependencies than this
  needs); only one of the two options.

## 2026-10-13: Iroh identity and tunnel details (task 4)

- **Decision:** Iroh 1.3.0, all Iroh code in `daemon/src/p2p.rs`.
  - Key encoding is **z-base-32** (`to_z32`/`from_z32` from Iroh): lowercase, 52 characters,
    no padding. It's the base32 variant Iroh and pkarr already use for DNS names, so no
    extra crate is needed.
  - The secret key is a hex text file `secret.key` in `--data-dir` (default: the OS app
    data folder via the `dirs` crate + `sovereign-web`), created with owner-only permissions
    on Unix.
  - The daemon joins the public Iroh network with the `N0` preset: n0's relay servers, and
    it publishes its address under its key so others can find it by key alone.
  - The tunnel carries raw bytes with no header; the port the browser asked for is ignored,
    because a peer serves one site. One Iroh connection per browser connection; peers are
    given up on after 30 seconds ("host unreachable").
  - If Iroh fails to start, the daemon keeps running with only `testsite.anon`.
  - Without `--serve`/`--forward` the daemon accepts no incoming tunnels (no ALPN
    registered).
- **Why:** Smallest thing that works end to end and matches the roadmap's design.
- **Trade-offs to know:** (1) Iroh's UDP socket listens on all network interfaces. That's
  how peer-to-peer connections reach you; it only speaks authenticated, encrypted QUIC and
  refuses protocols we don't register. The SOCKS proxy itself stays on 127.0.0.1. (2) The
  `N0` preset contacts n0.computer's relay and DNS servers, so they see your IP address and
  key (not your traffic, which is end-to-end encrypted). Running your own relay is under
  "Later".
- **Alternatives:** RFC 4648 base32 (needs another crate, not what Iroh uses); a small
  tunnel header with the port (not needed yet); reusing one connection per peer (faster
  page loads, more code; can come later).

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
