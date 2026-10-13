// applyProxy and clearProxy come from proxy.js, loaded first by popup.html.
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
  const change = enable ? applyProxy : clearProxy;

  change(() => {
    chrome.storage.local.set({ sovereignEnabled: enable });
    updateStatusText(enable);
  });
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
