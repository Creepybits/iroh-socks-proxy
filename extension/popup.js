const toggle = document.getElementById('toggle');
const statusText = document.getElementById('status');

// Load current state on popup open
chrome.storage.local.get(['sovereignEnabled'], (result) => {
  const enabled = result.sovereignEnabled || false;
  toggle.checked = enabled;
  updateStatusText(enabled);
});

// Listen for toggle changes
toggle.addEventListener('change', () => {
  const enable = toggle.checked;

  if (enable) {
    // Configure browser to route all traffic to local SOCKS5 loopback
    const config = {
      mode: "fixed_servers",
      rules: {
        singleProxy: {
          scheme: "socks5",
          host: "127.0.0.1",
          port: 9999
        },
        bypassList: ["localhost", "127.0.0.1"]
      }
    };

    chrome.proxy.settings.set({ value: config, scope: 'regular' }, () => {
      chrome.storage.local.set({ sovereignEnabled: true });
      updateStatusText(true);
    });
  } else {
    // Revert back to direct connection
    chrome.proxy.settings.clear({ scope: 'regular' }, () => {
      chrome.storage.local.set({ sovereignEnabled: false });
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
