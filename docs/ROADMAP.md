# Roadmap

Each task fits one cloud session and ends in one pull request. Do them in order.
Rules for working on these are in `CLAUDE.md`.

## Foundation

- [x] **1. Clean up, test, CI**
  - Split `daemon/src/main.rs` into modules (e.g. `socks.rs` for the protocol parsing,
    `router.rs` for deciding what happens to a target host, `main.rs` for startup).
  - Add a command-line flag for the listen address (default `127.0.0.1:9999`), using `clap`.
  - Fix small bugs while you're there: IPv6 targets are logged as a placeholder instead of
    the real address; the `.anon` check is case-sensitive (`TEST.ANON` should count);
    a reply is never sent when the domain isn't valid UTF-8.
  - Unit tests for SOCKS5 parsing (IPv4, IPv6, domain, bad version, unsupported command,
    unsupported address type, no acceptable auth method).
  - Integration tests that start the daemon on a free port and talk real SOCKS5 to it:
    `.anon` gets the landing page, a clearnet host gets "host unreachable".
  - GitHub Actions workflow: fmt, clippy and test on Linux, Windows and macOS.
  - Commit `Cargo.lock`.

- [ ] **2. Extension fixes and README sync**
  - `manifest.json` is missing the `privacy` permission, so the WebRTC leak protection in
    `popup.js` silently does nothing. Add it.
  - Add a small background service worker that re-applies or clears the proxy setting on
    browser startup to match the stored toggle state. This may help with the Brave
    "stuck proxy" bug in the README; the owner must verify by hand, so say so in the PR.
  - Update the README to match reality: Air-Gap Mode (clearnet is blocked, not forwarded),
    the real startup message, the new command-line flag, and how to run the tests.
    Fix the broken code blocks in the Quickstart.

- [ ] **3. Release builds**
  - GitHub Actions workflow that, when a tag like `v0.2.0` is pushed, builds the daemon for
    Windows, Linux and macOS and attaches the binaries plus a zip of `extension/` to a
    GitHub Release.
  - Explain in the PR description how the owner makes a release (they're not comfortable
    with git, so describe the GitHub web UI way to create a tag/release).

## Phase 3: Iroh

Design (see `docs/DECISIONS.md`): a `.anon` address is a peer's public key. Visiting it
opens an encrypted Iroh connection to that peer, and the HTTP bytes are tunnelled
through it, much like a Tor onion service. Content-addressed storage (`iroh-blobs`)
comes later.

- [ ] **4. Iroh identity and tunnel**
  - The daemon creates an Iroh endpoint at startup, with a secret key stored in a data
    directory (`--data-dir`, default: the OS-appropriate app data folder) so the identity
    survives restarts. Print the daemon's own `.anon` address at startup.
  - Address format: the 32-byte public key encoded as lowercase base32 without padding
    (52 characters, fits the 63-character limit for one part of a domain name), then `.anon`.
    E.g. `<52 chars>.anon`. Invalid keys get "host unreachable".
  - On a SOCKS CONNECT to a valid key address: connect to that peer over Iroh with a
    custom protocol name (ALPN) such as `sovereign/tunnel/0`, open a stream, and copy
    bytes both ways between the browser and the stream.
  - Keep `testsite.anon` serving the built-in landing page, so the demo still works offline.
  - Integration test: two endpoints in one test, connected by direct local address,
    a tunnelled HTTP request goes through and the response comes back.

- [ ] **5. Serving a site**
  - `--serve <dir>`: incoming tunnel connections are answered by a minimal static file
    server for that folder (GET only, no directory traversal outside the folder, `index.html`
    as the default page). Alternatively `--forward <127.0.0.1:port>` to forward to an existing
    local web server. Pick one or both, and log the decision.
  - Update the README with a "Publish your own .anon site" walkthrough the owner can follow
    on two computers.
  - Test for the directory-traversal protection (`/../secret` must not escape).

- [ ] **6. Security review**
  - Review the whole daemon and extension for: resource exhaustion (slow or huge
    handshakes, unbounded connections; add timeouts and limits), anything that could
    leak clearnet traffic or DNS lookups, path traversal, panics on bad input.
  - Fix what's small, list the rest under "Later" with an explanation.

## Later (not now)

- **Reaching normal websites through other nodes** ("exit nodes", like Tor). This is the
  long-term idea. It's a big design question with legal implications for whoever runs an
  exit node, so it needs a deliberate decision by the owner before any code.
- Friendly names (`myblog.anon`) on top of key addresses, e.g. a local petname file.
- Content-addressed caching/mirroring of sites with `iroh-blobs`.
- Phase 4: local root CA for HTTPS on `.anon`.
- Extension shows whether the daemon is running.
- Test the extension on Vivaldi, Edge and Firefox.
