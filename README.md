# iroh-socks-proxy
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.21499415.svg)](https://doi.org/10.5281/zenodo.21499415)

A local-first, decentralized P2P web overlay for digital sovereignty. This repository hosts a proof-of-concept decoupled architecture designed to bypass the execution constraints of Manifest V3 by running a native local daemon in Rust alongside a lightweight browser toggle switch.

> **Socio-Political Context & Architectural Feasibility:** For the deep dive into the threat landscape (such as EU Chat Control and biometric transport surveillance), traffic obfuscation strategies, and the detailed cryptographic design behind this protocol, read the full research paper on my digital workbench: **[https://zanno.se/](https://zanno.se/blueprint-for-a-sovereign-web/)** 

---

## Architectural Blueprint

Unlike legacy P2P browsers that try to run heavy network stacks directly in sandboxed browser profiles, `iroh-socks-proxy` uses a completely decoupled architecture:

```text
[ Browser (Chrome/Brave) ] --(Proxy Toggle ON)--> [ SOCKS5 Loopback (Port 9999) ]
                                                             |
  [ Standard Clearnet ] <---X--(Air-Gap: blocked)-- [ Local Rust Daemon (Tokio) ]
                                                             |
                                                   [ .anon Custom TLD ]
                                                             |
                                                   [ P2P Overlay (Iroh, planned) ]
```

1. **The Browser Extension (Manifest V3):** Purely acts as a configuration switch, utilizing `chrome.proxy` to route browser traffic to the local loopback interface on port `9999`.
2. **The Local Daemon (Rust / Tokio):** A high-performance background service that acts as a SOCKS5 proxy, intercepts requests, and handles alternative domain resolution locally.
3. **P2P Integration (Iroh, planned):** Will serve as the decentralized backbone, bypassing centralized DNS registries and streaming content securely from peers.

---

## Current Features

- **Async SOCKS5 Proxy:** Built natively in Rust using the asynchronous `tokio` runtime.
- **Sleek MV3 Toggle:** A minimalist, dark-mode browser extension configured with custom branding.
- **Custom TLD Interception:** Automatic interception and local resolution of `.anon` domains, bypassed entirely from standard DNS. `testsite.anon` shows a built-in landing page that works even offline.
- **Peer Addresses over Iroh:** Every daemon has its own `.anon` address: its public key, written as 52 lowercase letters and digits, e.g. `zf6cpab6btawcun738ayciuyufppjj5gaxshs97srgu6f6kkfs7o.anon`. Visiting such an address opens an encrypted [Iroh](https://iroh.computer) connection to that peer and passes the browser's traffic through it, much like a Tor onion service. The key is created on first start and kept in a data folder, so the address never changes. Other `.anon` names that are not valid keys are refused.
- **Publish Your Own Site:** `--serve <folder>` publishes a folder of web pages at your `.anon` address, using a small built-in file server. `--forward 127.0.0.1:<port>` publishes a web server already running on your computer instead. See [Publish your own .anon site](#publish-your-own-anon-site).
- **Air-Gap Mode:** Everything that is not `.anon` (normal websites, raw IP addresses) is refused with the SOCKS reply "host unreachable". Normal websites are intentionally **not** forwarded: reaching them through other nodes in the network is a later, still undecided design step. While the toggle is ON, only `.anon` sites load.
- **WebRTC Leak Protection:** While the toggle is ON, the extension tells the browser not to send WebRTC traffic outside the proxy (where it could reveal your real IP address).

---

## Quickstart (How to Run and Test)

### Option A: Download a ready-made build (no Rust needed)
Open the repository's **Releases** page on GitHub (right-hand side of the main page) and download:
* the daemon for your system: `...-x86_64-pc-windows-msvc.zip` (Windows), `...-aarch64-apple-darwin.tar.gz` (Mac with Apple chip), `...-x86_64-apple-darwin.tar.gz` (older Intel Mac) or `...-x86_64-unknown-linux-gnu.tar.gz` (Linux);
* `sovereign-web-extension-....zip`, the browser extension.

Unpack both. Start the daemon by running `sovereign-daemon` (Windows: `sovereign-daemon.exe`) from a terminal, then continue with Step 2 below, selecting the unpacked extension folder. The builds are not signed, so Windows SmartScreen or macOS Gatekeeper may warn you the first time (on macOS: right-click the file, then choose **Open**).

### Option B: Build from source

### Prerequisites
* **Rust Toolchain:** Ensure you have the Rust compiler and `cargo` installed natively.
* **Chromium Browser:** Chrome, Brave, Edge, or Vivaldi.

### Step 1: Run the Local Daemon
1. Navigate to the `/daemon` directory:
   ```bash
   cd daemon
   ```
2. Build and run the asynchronous service:
   ```bash
   cargo run
   ```

The console will print:

```text
⚡ Sovereign daemon bound strictly to 127.0.0.1:9999
🛡️  Air-Gap Mode Active: Non-.anon traffic will be instantly dropped.
🔑 Your .anon address: <52 characters>.anon
```

The secret key behind that address is stored in a `secret.key` file in your app data folder (`~/.local/share/sovereign-web` on Linux, `~/Library/Application Support/sovereign-web` on macOS, `%APPDATA%\sovereign-web` on Windows). Keep it private: anyone with that file can pretend to be your address. Use `--data-dir <folder>` to keep it somewhere else. If Iroh can't start, the daemon says so and keeps serving `testsite.anon`.

To listen on a different address or port, pass `--listen` (the extension expects `127.0.0.1:9999`, so only change this if you know why):

```bash
cargo run -- --listen 127.0.0.1:9050
```

The daemon only accepts connections from your own computer by default. If you pass an address other than `127.0.0.1`, it prints a warning, because other machines could then use it. Run `cargo run -- --help` to see all options.

### Step 2: Load the Browser Extension
1. Open your browser and navigate to `chrome://extensions/`
2. Toggle Developer mode to ON in the top-right corner.
3. Click Load unpacked in the top-left, and select the `/extension` folder from this repository.

### Step 3: Test Local Domain Interception
1. Click the Sovereign Web Toggle icon in your browser toolbar and switch it to ON (it will display green Isolated Sandbox).
2. In your browser bar, navigate to: `http://testsite.anon/`
3. Your browser will instantly display your decentralized landing page, served directly from your local Rust daemon.
4. Normal websites (like Google or Wikipedia) will **not** load while the toggle is ON: the daemon refuses them (Air-Gap Mode). Switch the toggle OFF to browse normally again.

### Publish your own .anon site
You need two computers, each with the daemon (see Option A or B above). Call them the **publisher** (hosts the site) and the **visitor** (browses it). They can be on different networks; Iroh finds a way through, using a relay server if needed.

**On the publisher:**
1. Make a folder with an `index.html` in it, for example `my-site/index.html` containing `<h1>Hello from my .anon site</h1>`.
2. Start the daemon with that folder (from the `daemon` folder if you build from source):
   ```bash
   cargo run -- --serve path/to/my-site
   ```
   With a downloaded build, run `sovereign-daemon --serve path/to/my-site` instead (Windows: `sovereign-daemon.exe --serve path\to\my-site`).
3. The daemon prints your address, for example:
   ```text
   🔑 Your .anon address: qm5i78frnziufjtqjbkzqbqxr9qq9mo9sc5zm8skrfddz6gqmqzo.anon
   📂 Publishing /home/you/my-site at that address.
   ```
   Copy that address and send it to the visitor. Keep the daemon running; the site is online only while it runs.

**On the visitor:**
1. Start the daemon normally (`cargo run` or `sovereign-daemon`) and turn the extension toggle ON.
2. Open `http://<the address you received>/` in the browser. Note: `http://`, not `https://`.
3. The first visit can take a few seconds while the two computers find each other.

**Good to know:**
* Only files inside the folder are served (requests like `/../secret` are refused), and only plain `GET` requests are answered. Subfolders work; a folder's `index.html` is its default page.
* To publish something more dynamic, run any web server on the publisher (for a Python user: `python -m http.server 8000`) and start the daemon with `--forward 127.0.0.1:8000`. For safety, only addresses on the same computer are allowed.
* Your address stays the same as long as you keep the `secret.key` file (see Step 1).
* If the page doesn't load, check that both daemons print a `.anon` address (no Iroh warning) and that the visitor typed the address exactly.

### Running the Tests
```bash
cd daemon
cargo test
```

The tests never use the internet: they start the daemon on a free local port and talk SOCKS5 to it. The same checks (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`) run automatically on Linux, Windows and macOS for every pull request.
### Making a Release (maintainers)
On GitHub, open **Releases → Draft a new release**, type a new tag such as `v0.2.0` in **Choose a tag** and pick **Create new tag on publish**, then press **Publish release**. A workflow then builds the daemon for Windows, macOS and Linux and attaches the files plus the extension zip to that release within about 10 minutes (progress is under the **Actions** tab).
___
## Security & Privacy

What this prototype protects against, and what it doesn't (reviewed in roadmap task 6):

* **Normal websites can't leak around it.** While the toggle is ON, the browser sends everything to the daemon, with nothing exempt. The daemon refuses every non-`.anon` host and never looks up names in DNS itself: the browser hands the name to the proxy, and `.anon` names are decoded locally. WebRTC is told to stay inside the proxy. If the daemon isn't running, pages fail to load instead of going around it.
* **Browser limits to know about:** private/incognito windows are **not** covered unless you allow the extension in incognito, and Chrome always connects to `localhost` directly (that stays on your computer).
* **The proxy is only reachable from your computer** (`127.0.0.1`), unless you deliberately pass another `--listen` address.
* **Limits against overload:** at most 256 browser connections and 64 visiting peers at once; a client that doesn't finish its handshake within 10 seconds is disconnected; requests to a published site are limited to 8 KiB of headers and 10 seconds.
* **Published sites only expose what you put in the folder:** no paths outside it (including through shortcuts/symlinks), and no hidden files such as `.git` or `.env`. `--forward` only reaches web servers on your own computer.
* **What others can see:** traffic between peers is end-to-end encrypted by Iroh. n0.computer's relay and lookup servers (used by default) can see your IP address and your key, but not your traffic. A site you visit sees your key-based connection, not your browsing elsewhere. Like any website, the publisher of a `.anon` site sees what you request from it.
* **Not done yet** (see "Later" in `docs/ROADMAP.md`): no HTTPS inside the tunnel (it's already encrypted end to end by Iroh, but the browser shows "Not Secure"), no protection against a peer that keeps a connection open but idle, and n0's servers are used by default.
___
## Known Issues & Workarounds

### ⚠️ Brave Browser: Permanent Proxy Latching Bug
There is a profile-level state bug specifically affecting **Brave Browser** where the browser permanently locks its internal proxy routing to the local loopback interface (`127.0.0.1:9999`) once the extension is initialized.

*   **The Symptom:** The extension toggle becomes unresponsive, and Brave remains permanently locked to the proxy even if the extension is disabled or completely uninstalled from the browser.
*   **The Behavior:** 
    *   If the local Rust daemon is **running**, only `.anon` domains load; normal websites are refused (Air-Gap Mode).
    *   If the local daemon is **stopped**, all internet connectivity inside that specific Brave profile will be completely blocked.
*   **Possible Fix (unconfirmed):** Since version 1.1, the extension re-applies or clears the proxy setting every time the browser starts, to match the toggle. This may unstick Brave; it has not been confirmed yet.
*   **Observed Scope:** This issue is **not** present in Google Chrome (where the proxy properly detaches on toggle/uninstall). Testing is currently pending on Vivaldi, Edge, and Firefox.

#### 💡 Temporary Workaround
If you are developing, testing, or running the prototype in Brave, **do not load the extension in your primary browsing profile.** Instead, use a dedicated profile:
1. Click your profile icon in Brave and select **Add** to create a new, clean user profile.
2. Load the unpacked extension *only* within this dedicated test profile.
3. This keeps your main Brave profile completely untouched and allows you to safely test the SOCKS5 daemon's routing behavior.
___
## Technical Roadmap
- [x] Phase 1: Core async SOCKS5 proxy loopback bound to 127.0.0.1:9999
- [x] Phase 2: Manifest V3 extension proxy-management interface and local `.anon` domain interception
- [ ] Phase 3: Integrate the Iroh P2P protocol: a `.anon` address is a peer's public key, and visiting it tunnels the browser's traffic to that peer over an encrypted Iroh connection (similar to a Tor onion service). Content-addressed storage (`iroh-blobs`) comes later. See `docs/ROADMAP.md`.
- [ ] Phase 4: Implement automatic, local Root CA certificate generation to handle dynamic local TLS handshakes (resolving the browser's "Not Secure" warning over loopback).

___
## Contributing

This project is a collaborative, open-source effort to build a truly sovereign web overlay. Since this is currently a conceptual MVP, **contributions, feedback, and pull requests are highly welcomed!** 

Specifically, we are looking for help with:
*   **Systems Programming (Rust):** Building the local Root CA certificate manager and handling dynamic TLS handshakes (Phase 4).
*   **P2P Architecture:** Integrating the `iroh` node for content routing and block streaming (Phase 3).
*   **Browser Extension (JS):** Optimizing the Manifest V3 background port and PAC configurations.

Feel free to open an issue, start a discussion, or submit a pull request!
___
## License
This project is open-source and licensed under the permissive MIT License
