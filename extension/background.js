// Talks to the FitDL desktop app's local API. Requests from the extension
// carry an chrome-extension:// Origin, which the app trusts.

const DEFAULT_PORT = 7878;

async function port() {
  const { port } = await chrome.storage.sync.get({ port: DEFAULT_PORT });
  return port;
}

async function call(path, body) {
  const res = await fetch(`http://127.0.0.1:${await port()}${path}`, {
    method: body ? "POST" : "GET",
    headers: body ? { "Content-Type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `FitDL returned HTTP ${res.status}`);
  return data;
}

async function send(links, game) {
  try {
    const result = await call("/api/add", {
      links: links.map(({ url, label, filename }) => ({ url, label, filename })),
      game,
    });
    return { ok: true, result };
  } catch (e) {
    const offline = e instanceof TypeError; // fetch failed: app not running
    return { ok: false, offline, error: offline ? "FitDL is not running. Start the app and try again." : e.message };
  }
}

chrome.runtime.onMessage.addListener((msg, sender, reply) => {
  if (msg.type === "send") {
    send(msg.links, msg.game).then(reply);
    return true;
  }
  if (msg.type === "ping") {
    call("/api/ping").then(
      (info) => reply({ ok: true, info }),
      () => reply({ ok: false }),
    );
    return true;
  }
  if (msg.type === "count" && sender.tab) {
    chrome.action.setBadgeBackgroundColor({ color: "#5b4cf0", tabId: sender.tab.id });
    chrome.action.setBadgeText({ text: msg.count ? String(msg.count) : "", tabId: sender.tab.id });
  }
  return false;
});

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "fitdl-link",
    title: "Download with FitDL",
    contexts: ["link"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== "fitdl-link" || !info.linkUrl) return;
  const r = await send([{ url: info.linkUrl, label: info.selectionText || "" }], null);
  if (tab?.id) {
    chrome.action.setBadgeBackgroundColor({ color: r.ok ? "#1a9a5b" : "#d23b3b", tabId: tab.id });
    chrome.action.setBadgeText({ text: r.ok ? "+1" : "!", tabId: tab.id });
  }
});
