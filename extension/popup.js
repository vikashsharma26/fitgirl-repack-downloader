const $ = (id) => document.getElementById(id);
let links = [];
let title = null;

async function init() {
  const { port } = await chrome.storage.sync.get({ port: 7878 });
  $("port").value = port;
  $("port").addEventListener("change", async () => {
    await chrome.storage.sync.set({ port: Number($("port").value) || 7878 });
    checkApp();
  });
  checkApp();

  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  let page = null;
  if (tab?.url?.startsWith("https://fitgirl-repacks.site/")) {
    // The content script already defined fitdlExtract in the page.
    page = await chrome.tabs
      .sendMessage(tab.id, { type: "extract" })
      .catch(() => null);
  }
  if (!page || !page.links.length) {
    $("empty").hidden = false;
    return;
  }
  links = page.links;
  title = page.title;
  render();
}

async function checkApp() {
  const r = await chrome.runtime.sendMessage({ type: "ping" });
  const el = $("status");
  el.className = "status " + (r.ok ? "ok" : "bad");
  el.textContent = r.ok ? `Connected to FitDL ${r.info.version}` : "FitDL app is not running";
}

function render() {
  $("page").hidden = false;
  $("title").textContent = title || "Downloads on this page";
  const list = $("list");
  list.replaceChildren(
    ...links.map((l, i) => {
      const li = document.createElement("li");
      const label = document.createElement("label");
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = !l.optional;
      box.dataset.index = i;
      box.addEventListener("change", updateCount);
      const name = document.createElement("span");
      name.textContent = l.filename;
      name.title = l.filename;
      label.append(box, name);
      if (l.optional) {
        const tag = document.createElement("em");
        tag.className = "tag";
        tag.textContent = "optional";
        label.append(tag);
      }
      li.append(label);
      return li;
    }),
  );
  updateCount();
}

function boxes() {
  return [...document.querySelectorAll("#list input")];
}

function updateCount() {
  const n = boxes().filter((b) => b.checked).length;
  $("send").textContent = `Send ${n} file${n === 1 ? "" : "s"} to FitDL`;
  $("send").disabled = n === 0;
}

document.querySelectorAll("[data-select]").forEach((btn) =>
  btn.addEventListener("click", () => {
    const mode = btn.dataset.select;
    boxes().forEach((b) => (b.checked = mode === "all" || (mode === "main" && !links[b.dataset.index].optional)));
    updateCount();
  }),
);

$("send").addEventListener("click", async () => {
  const selected = boxes().filter((b) => b.checked).map((b) => links[b.dataset.index]);
  $("send").disabled = true;
  const r = await chrome.runtime.sendMessage({ type: "send", links: selected, game: title });
  const out = $("result");
  out.className = "result " + (r.ok ? "ok" : "bad");
  out.textContent = r.ok
    ? `Queued ${r.result.added} file(s)${r.result.skipped ? `, ${r.result.skipped} were already queued` : ""}.`
    : r.error;
  updateCount();
});

init();
