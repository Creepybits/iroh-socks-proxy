# Instructions for Claude (cloud sessions)

This repo is a proof-of-concept: a local Rust SOCKS5 daemon plus a Manifest V3 browser
toggle, aiming to serve `.anon` sites over the Iroh P2P network. See `README.md` for the
pitch and `docs/ROADMAP.md` for the work plan.

## About the owner

- Experienced Python developer. **Not** a networking or Rust expert, so explain network
  concepts briefly in plain language, with Python analogies where they help.
- Not comfortable with git or the command line. Never ask them to run git commands.
- Works in short sessions and does not want to answer questions mid-task.

## How to work

1. **Pick the task.** If the prompt names a task, do that one. Otherwise do the first
   unchecked task in `docs/ROADMAP.md`. Do only that one task.
2. **Don't stop to ask questions.** When something is unclear, choose the simplest option
   that fits the roadmap, write it down in `docs/DECISIONS.md` (date, decision, why,
   alternatives considered), and continue. Ask only if the task truly can't be done
   without the owner, e.g. it needs a secret or account access. In that case, stop and
   explain why in the PR description.
3. **Treat `docs/ROADMAP.md` as the source of truth for scope.** Ideas outside the current
   task go under "Later" in the roadmap. Don't build them.
4. **Finish with a pull request** against `Master`:
   - One branch and one PR per task.
   - In the same PR, tick the task's checkbox in `docs/ROADMAP.md`.
   - Write the PR description for a non-expert: what changed, why, how to try it
     (exact commands), and anything the owner needs to decide or test by hand
     (e.g. browser behaviour, which you can't test).
   - CI must pass. If it can't, say exactly why in the PR description.

## Code rules

- The Rust crate lives in `daemon/`, the extension in `extension/`.
- Before opening the PR, all of these must pass in `daemon/`:
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- **Tests must not need the internet.** Bind to `127.0.0.1:0` (any free port). For Iroh
  tests, connect endpoints by direct local address, without relays or discovery services.
- Commit `Cargo.lock` (this is a binary, not a library).
- Pin new dependencies to a specific version. Prefer few, well-known crates.
- Iroh's API changes between releases. Read the docs for the exact pinned version
  (docs.rs), don't rely on memory. Keep all Iroh code behind its own module so an
  upgrade only touches one place.
- Keep the daemon bound to loopback (`127.0.0.1`) by default. Never add a default that
  listens on all interfaces.
- No `unwrap()`/`expect()` on data that comes from the network or from the browser.
- Keep the README accurate. If behaviour changes, update the README in the same PR.

## Security posture (don't weaken without logging a decision)

- Air-Gap Mode: non-`.anon` traffic is refused (SOCKS reply "host unreachable").
  Normal websites are intentionally **not** forwarded. See "Later" in the roadmap.
- The extension routes **all** browser traffic to the daemon with an empty bypass list.
  Keep it that way.
- Don't log full URLs or request contents by default. Host names are fine.
