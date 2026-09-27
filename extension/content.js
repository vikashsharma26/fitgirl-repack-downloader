// On FitGirl game pages: show a "Download with FitDL" button next to the
// post title and report the link count for the toolbar badge.
(function () {
  const page = globalThis.fitdlExtract(document);
  chrome.runtime.sendMessage({ type: "count", count: page.links.length });

  // The popup asks for a fresh extraction of the page.
  chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
    if (msg.type === "extract") reply(globalThis.fitdlExtract(document));
  });
  if (!page.links.length) return;

  const main = page.links.filter((l) => !l.optional);
  const bar = document.createElement("div");
  bar.className = "fitdl-bar";
  const button = document.createElement("button");
  button.type = "button";
  button.className = "fitdl-btn";
  button.textContent = `⬇ Download with FitDL (${main.length} files)`;
  const note = document.createElement("span");
  note.className = "fitdl-note";
  const optional = page.links.length - main.length;
  note.textContent = optional ? `${optional} optional files: pick them from the toolbar button` : "";
  bar.append(button, note);

  button.addEventListener("click", async () => {
    button.disabled = true;
    note.textContent = "Sending…";
    const r = await chrome.runtime.sendMessage({ type: "send", links: main, game: page.title });
    button.disabled = false;
    note.className = "fitdl-note " + (r.ok ? "ok" : "bad");
    note.textContent = r.ok
      ? `Queued ${r.result.added} file(s) in FitDL${r.result.skipped ? `, ${r.result.skipped} already queued` : ""}.`
      : r.error;
  });

  const heading = document.querySelector("h1.entry-title");
  (heading?.parentElement ?? document.body).insertBefore(bar, heading?.nextSibling ?? null);
})();
