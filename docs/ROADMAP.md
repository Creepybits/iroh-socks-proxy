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

- [x] **2. Extension fixes and README sync**
  - `manifest.json` is missing the `privacy` permission, so the WebRTC leak protection in
    `popup.js` silently does nothing. Add it.
  - Add a small background service worker that re-applies or clears the proxy setting on
    browser startup to match the stored toggle state. This may help with the Brave
    "stuck proxy" bug in the README; the owner must verify by hand, so say so in the PR.
  - Update the README to match reality: Air-Gap Mode (clearnet is blocked, not forwarded),
    the real startup message, the new command-line flag, and how to run the tests.
    Fix the broken code blocks in the Quickstart.

- [x] **3. Release builds**
  - GitHub Actions workflow that, when a tag like `v0.2.0` is pushed, builds the daemon for
    Windows, Linux and macOS and attaches the binaries plus a zip of `extension/` to a
    GitHub Release.
  - Explain in the PR description how the owner makes a release (they're not comfortable
    with git, so describe the GitHub web UI way to create a tag/release).

## Phase 3: Iroh

Design (see `docs/DECISIONS.md`): a `.anon` address is a peer's public key. Visiting it
opens an encrypted Iroh connection to that peer, and the HTTP bytes are tunnelled
through it. Like a Tor onion service, the address is a key; unlike Tor, the connection is
direct, so it is encrypted but not anonymous. Content-addressed storage (`iroh-blobs`)
comes later.

- [x] **4. Iroh identity and tunnel**
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

- [x] **5. Serving a site**
  - `--serve <dir>`: incoming tunnel connections are answered by a minimal static file
    server for that folder (GET only, no directory traversal outside the folder, `index.html`
    as the default page). Alternatively `--forward <127.0.0.1:port>` to forward to an existing
    local web server. Pick one or both, and log the decision.
  - Update the README with a "Publish your own .anon site" walkthrough the owner can follow
    on two computers.
  - Test for the directory-traversal protection (`/../secret` must not escape).

- [x] **6. Security review**
  - Review the whole daemon and extension for: resource exhaustion (slow or huge
    handshakes, unbounded connections; add timeouts and limits), anything that could
    leak clearnet traffic or DNS lookups, path traversal, panics on bad input.
  - Fix what's small, list the rest under "Later" with an explanation.

## Hosting

- [x] **7. Docker image and always-on hosting**
  - A `Dockerfile` that builds the daemon and publishes the repo's `site/` folder,
    running as a non-root user with the secret key on a `/data` volume.
  - GitHub Actions workflow that publishes the image to ghcr.io on pushes to `Master`
    and version tags.
  - A no-command-line guide for hosting on bunny.net Magic Containers.

## Chat

- [x] **8. Terminal chat**
  - `--chat` waits for a caller; `--chat <address>` calls that `.anon` address. Lines of
    text travel over an Iroh stream with its own protocol name (`sovereign/chat/0`).
  - Received text is cleaned of control characters; 4 KiB per message.

- [x] **9. Chat app**
  - `--chat-app` serves the same chat as a page in the browser, reachable only from your
    own computer behind a random secret in its address.

## Later (not now)

- **Anonymity** (today connections are encrypted but direct, so sites see visitors' IP
  addresses; see README "Security & Privacy"). Options, smallest first:
  - **Separate visiting key:** browse with a fresh random key (per session, or per site)
    instead of the daemon's publishing key, so different sites can't link your visits.
    Small change, but it doesn't hide your IP address.
  - **Route through Tor:** carry the Iroh traffic over Tor (e.g. with the `arti` crate,
    Tor's Rust implementation), so neither side learns the other's IP address. Reuses
    Tor's existing network instead of building our own; slower, and needs research into
    how well Iroh works over Tor.
  - **Our own multi-hop routing** through other daemons (onion routing, like Tor). Only
    anonymous once many independent people run relay nodes, and very hard to get right;
    a long-term research project, not a feature.
- **Reaching normal websites through other nodes** ("exit nodes", like Tor). This is the
  long-term idea. It's a big design question with legal implications for whoever runs an
  exit node, so it needs a deliberate decision by the owner before any code.
- Friendly names (`myblog.anon`) on top of key addresses, e.g. a local petname file.
- Content-addressed caching/mirroring of sites with `iroh-blobs`.
- Phase 4: local root CA for HTTPS on `.anon`.
- Extension shows whether the daemon is running.
- Test the extension on Vivaldi, Edge and Firefox.
- Run our own Iroh relay / address lookup server, or make the n0 defaults configurable, so
  n0.computer doesn't see users' IP addresses and keys.
- Reuse one Iroh connection per peer instead of one per browser connection (faster pages).
- From the task 6 security review (not fixed because they need more design or code than
  they're worth right now):
  - **Idle connections:** after the handshake, a tunnel or browser connection that stays
    open but silent is not cut off by the daemon (Iroh's own idle timeout and the 64/256
    connection limits bound the damage). An idle timeout on the copy loops would close it.
  - **Per-peer limits:** one peer can use all 64 visitor slots. Per-peer quotas or rate
    limits would stop one visitor from crowding out others.
  - **Incognito windows:** the extension only sets the proxy for normal windows. Setting it
    for incognito too (when the user allows the extension there) needs testing in each
    browser.
  - **Daemon not running:** the extension could warn when the daemon isn't reachable
    (already listed above as "Extension shows whether the daemon is running").
  - **Secret key at rest:** `secret.key` is protected only by file permissions; encrypting
    it with a password would protect against someone who can read your files.
- **Voice and video calls between `.anon` addresses** (a separate desktop app, not part
  of the browser proxy). Your `.anon` address works like a phone number: the app calls
  another address and the same app on the other side rings. Today's proxy can't do this:
  it only handles CONNECT (web-page style) connections, browser calls (WebRTC) send UDP
  that bypasses the proxy, and Air-Gap Mode would block them anyway.
  - **Shape:** one desktop app with the Iroh code built in (no separate daemon or
    extension), reusing this project's Iroh module. Likely a separate repository.
  - **Pieces:** ringing (call request / accept / decline as small messages on an Iroh
    connection); audio captured from the microphone, compressed with Opus and sent as
    Iroh datagrams (late packets are dropped, not waited for); a contact list mapping
    names to addresses; video later.
  - **Hard parts:** call quality (echo cancellation, smoothing uneven network timing);
    receiving calls only works while the app is running (e.g. in the system tray), since
    push notifications would mean a middleman; calls through a relay add delay (voice
    usually fine, video may stutter); encrypted but not anonymous, like the sites.
  - **Already done:** text chat (tasks 8 and 9 above). The chat app's page could become
    the interface of the desktop window.
  - **Steps:** (1) a command-line voice call between two computers using `.anon`
    addresses; (2) a desktop window (e.g. Tauri: Rust core with a web-style interface);
    (3) contact list; (4) video.
