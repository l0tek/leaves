# Lokale Reader-Bibliotheken

Unveränderte Distributionen, zur Buildzeit über `include_str!` eingebunden und
vom Leaves-Server ausgeliefert. Kein npm-Build im Projekt erforderlich.

| Datei | Version | Quelle | Lizenz |
| --- | --- | --- | --- |
| `epub.min.js` | 0.3.93 | https://cdn.jsdelivr.net/npm/epubjs@0.3.93/dist/epub.min.js | BSD-2-Clause, siehe `epub.LICENSE` |
| `jszip.min.js` | 3.10.1 | https://cdn.jsdelivr.net/npm/jszip@3.10.1/dist/jszip.min.js | MIT oder GPLv3; hier unter MIT verwendet, siehe `jszip.LICENSE` |

API und Sicherheitsvorgaben:
https://github.com/futurepress/epub.js/tree/v0.3.93

Bei Updates Versionsangaben und Lizenzen prüfen sowie den Reader-Browsertest
wiederholen. EPUB-Skripte dürfen nicht durch `allowScriptedContent` aktiviert werden.
