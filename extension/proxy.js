// Shared by popup.js and background.js: turns the daemon proxy on or off.

const DAEMON_PROXY = {
  mode: "fixed_servers",
  rules: {
    singleProxy: {
      scheme: "socks5",
      host: "127.0.0.1",
      port: 9999
    },
    // Empty bypass list guarantees local OS stack & telemetry cannot leak around the proxy
    bypassList: []
  }
};

function hasWebRTCPolicy() {
  return Boolean(chrome.privacy && chrome.privacy.network && chrome.privacy.network.webRTCIPHandlingPolicy);
}

// Route ALL browser traffic into the daemon loopback and harden WebRTC.
function applyProxy(callback) {
  chrome.proxy.settings.set({ value: DAEMON_PROXY, scope: 'regular' }, () => {
    // Harden WebRTC against direct UDP interface leaks
    if (hasWebRTCPolicy()) {
      chrome.privacy.network.webRTCIPHandlingPolicy.set({
        value: "disable_non_proxied_udp",
        scope: 'regular'
      });
    }
    if (callback) callback();
  });
}

// Revert proxy and restore default WebRTC policies.
function clearProxy(callback) {
  chrome.proxy.settings.clear({ scope: 'regular' }, () => {
    if (hasWebRTCPolicy()) {
      chrome.privacy.network.webRTCIPHandlingPolicy.clear({ scope: 'regular' });
    }
    if (callback) callback();
  });
}

// Make the browser's proxy setting match the stored toggle state.
function syncProxyWithStoredState() {
  chrome.storage.local.get(['sovereignEnabled'], (result) => {
    if (result.sovereignEnabled) {
      applyProxy();
    } else {
      clearProxy();
    }
  });
}
