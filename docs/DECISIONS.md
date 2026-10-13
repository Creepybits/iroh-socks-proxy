# Decisions

Newest first. Each entry: date, decision, why, alternatives considered.

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
