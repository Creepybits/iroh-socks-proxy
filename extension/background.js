// Re-applies or clears the proxy when the browser starts or the extension is
// installed/updated, so the browser always matches the toggle shown in the popup.
importScripts('proxy.js');

chrome.runtime.onStartup.addListener(syncProxyWithStoredState);
chrome.runtime.onInstalled.addListener(syncProxyWithStoredState);
