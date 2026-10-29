/* Leaves EPUB reader. epub.js 0.3.93 + JSZip 3.10.1 are served locally. */
(async () => {
  "use strict";
  const root = document.getElementById("reader");
  const status = document.getElementById("reader-status");
  const previous = document.getElementById("previous");
  const next = document.getElementById("next");
  const chapters = document.getElementById("chapters");
  const fontSize = document.getElementById("font-size");
  const key = `leaves:reader:${root.dataset.bookId}`;
  let book, rendition, busy = false;
  let saved = {};
  try { saved = JSON.parse(localStorage.getItem(key) || "{}") || {}; } catch (_) {}
  const fail = () => { status.textContent = "Das Buch konnte nicht angezeigt werden. Bitte erneut laden oder herunterladen. Bei verschlüsselten oder beschädigten EPUBs ist Lesen eventuell nicht möglich."; };
  const navigate = async (action) => {
    if (!rendition || busy) return;
    busy = true;
    try { await action(); } catch (_) { fail(); }
    finally { busy = false; }
  };
  try {
    if (typeof ePub !== "function") throw new Error("Reader fehlt");
    const response = await fetch(root.dataset.bookUrl);
    if (!response.ok) throw new Error("Download fehlgeschlagen");
    book = ePub(await response.arrayBuffer());
    await book.opened;
    await book.ready;
    rendition = book.renderTo("viewer", {
      width: "100%", height: "100%", spread: "none", flow: "paginated",
      allowScriptedContent: false, allowPopups: false
    });
    rendition.on("displayError", fail);
    rendition.on("relocated", (location) => {
      previous.disabled = location.atStart;
      next.disabled = location.atEnd;
      const item = book.navigation.get(location.start.href);
      status.textContent = item ? item.label.trim() : "Buch geöffnet";
      const option = Array.from(chapters.options).find(o => o.value.split("#")[0] === location.start.href);
      if (option) chapters.value = option.value;
      saved.cfi = location.start.cfi;
      try { localStorage.setItem(key, JSON.stringify(saved)); } catch (_) {}
    });
    const addChapters = (items, depth = 0) => {
      for (const item of items) {
        const option = document.createElement("option");
        option.value = item.href;
        option.textContent = "— ".repeat(depth) + item.label.trim();
        chapters.appendChild(option);
        addChapters(item.subitems || [], depth + 1);
      }
    };
    const navigation = await book.loaded.navigation;
    addChapters(navigation.toc);
    chapters.disabled = navigation.toc.length === 0;
    if (["90", "110", "130", "150"].includes(saved.fontSize)) fontSize.value = saved.fontSize;
    rendition.themes.fontSize(`${fontSize.value}%`);
    try { await rendition.display(saved.cfi || undefined); }
    catch (_) { await rendition.display(); }
    previous.addEventListener("click", () => navigate(() => rendition.prev()));
    next.addEventListener("click", () => navigate(() => rendition.next()));
    chapters.addEventListener("change", () => { if (chapters.value) navigate(() => rendition.display(chapters.value)); });
    fontSize.addEventListener("change", () => {
      saved.fontSize = fontSize.value;
      rendition.themes.fontSize(`${fontSize.value}%`);
      try { localStorage.setItem(key, JSON.stringify(saved)); } catch (_) {}
    });
    const keyboard = (event) => {
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || /INPUT|SELECT|TEXTAREA/.test(event.target.tagName) || event.target.isContentEditable) return;
      if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        navigate(() => event.key === "ArrowLeft" ? rendition.prev() : rendition.next());
      }
    };
    document.addEventListener("keydown", keyboard);
    rendition.on("keydown", keyboard);
    window.addEventListener("pagehide", () => book.destroy(), { once: true });
  } catch (_) { fail(); }
})();
