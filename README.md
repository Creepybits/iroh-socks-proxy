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

## Current MVP Features (Phase 1 & 2)

- **Async SOCKS5 Proxy:** Built natively in Rust using the asynchronous `tokio` runtime.
- **Sleek MV3 Toggle:** A minimalist, dark-mode browser extension configured with custom branding.
- **Custom TLD Interception:** Automatic interception and local resolution of `.anon` domains, bypassed entirely from standard DNS. For now every `.anon` name shows the same built-in landing page.
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
```

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

### Running the Tests
```bash
cd daemon
cargo test
```

The tests never use the internet: they start the daemon on a free local port and talk SOCKS5 to it. The same checks (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`) run automatically on Linux, Windows and macOS for every pull request.
### Making a Release (maintainers)
On GitHub, open **Releases → Draft a new release**, type a new tag such as `v0.2.0` in **Choose a tag** and pick **Create new tag on publish**, then press **Publish release**. A workflow then builds the daemon for Windows, macOS and Linux and attaches the files plus the extension zip to that release within about 10 minutes (progress is under the **Actions** tab).
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
