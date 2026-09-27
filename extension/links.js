// Shared by the content script and the popup: find every <a> on the page
// whose href points at fuckingfast.co and keep its text.
// (Mirrors crates/scraper/src/lib.rs.)
globalThis.fitdlExtract = function (doc) {
  const seen = new Set();
  const links = [];
  for (const a of doc.querySelectorAll("a[href]")) {
    let url;
    try {
      url = new URL(a.href);
    } catch {
      continue;
    }
    const host = url.hostname.toLowerCase();
    if (host !== "fuckingfast.co" && !host.endsWith(".fuckingfast.co")) continue;
    if (seen.has(a.href)) continue;
    seen.add(a.href);

    const label = a.textContent.replace(/\s+/g, " ").trim();
    // The real file name is in the #fragment ("--"); the text shows "–".
    let filename = "";
    try {
      filename = decodeURIComponent(url.hash.slice(1));
    } catch {
      filename = url.hash.slice(1);
    }
    filename = (filename.trim() || label).replace(/[<>:"/\\|?*\x00-\x1f]/g, "_");
    links.push({ url: a.href, label, filename, optional: filename.toLowerCase().startsWith("fg-optional") });
  }
  const title = doc.querySelector("h1.entry-title")?.textContent.replace(/\s+/g, " ").trim() || null;
  return { title, links };
};
