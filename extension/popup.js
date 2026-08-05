const toggle = document.getElementById('toggle');
const statusText = document.getElementById('status');

// Sync UI state on popup activation
chrome.storage.local.get(['sovereignEnabled'], (result) => {
  const enabled = result.sovereignEnabled || false;
  toggle.checked = enabled;
  updateStatusText(enabled);
});

toggle.addEventListener('change', () => {
  const enable = toggle.checked;

  if (enable) {
    // Single proxy mode: Route ALL browser traffic into the daemon loopback
    const config = {
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

    chrome.proxy.settings.set({ value: config, scope: 'regular' }, () => {
      chrome.storage.local.set({ sovereignEnabled: true });
      
      // Harden WebRTC against direct UDP interface leaks
      if (chrome.privacy && chrome.privacy.network && chrome.privacy.network.webRTCIPHandlingPolicy) {
        chrome.privacy.network.webRTCIPHandlingPolicy.set({
          value: "disable_non_proxied_udp"
        });
      }
      
      updateStatusText(true);
    });
  } else {
    // Revert proxy and restore default WebRTC policies
    chrome.proxy.settings.clear({ scope: 'regular' }, () => {
      chrome.storage.local.set({ sovereignEnabled: false });
      
      if (chrome.privacy && chrome.privacy.network && chrome.privacy.network.webRTCIPHandlingPolicy) {
        chrome.privacy.network.webRTCIPHandlingPolicy.clear({ scope: 'regular' });
      }
      
      updateStatusText(false);
    });
  }
});

function updateStatusText(enabled) {
  if (enabled) {
    statusText.textContent = "Isolated Sandbox";
    statusText.style.color = "#10b981"; // Jade Green
  } else {
    statusText.textContent = "Clearnet Mode";
    statusText.style.color = "#cccccc";
  }
}
