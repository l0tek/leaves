# Leaves – E-Book-Verwaltung

Rust, Dioxus SSR, Axum und SQLite. Deutsche, responsive Oberfläche mit Upload, Titel-/Autorsuche, Lesestatus, Metadatenbearbeitung, Download und bestätigtem Löschen. Unterstützt EPUB, PDF, MOBI und AZW3 bis 50 MiB. Die Buchcover sind typografische Platzhalter. Kein integrierter Reader, keine DRM-Entfernung.

Beim Hochladen werden leere Titel-/Autorfelder automatisch aus Metadaten ergänzt: EPUB (OPF), PDF (Info-Dictionary) und MOBI/AZW3 (MOBI-/EXTH-Header). Mehrere Autoren werden zusammengeführt. Manuelle Eingaben haben pro Feld Vorrang. Fehlen lesbare Metadaten, dient der Dateiname als Titel und der Autor bleibt leer. Die Erkennung erfolgt beim Absenden des Uploads, ohne externe Dienste; fehlerhafte oder verschlüsselte Metadaten können unerkannt bleiben. Bereits gespeicherte Bücher bleiben unverändert.

## Kategorien und Schlagwörter

Jedes Buch erhält optional eine Hauptkategorie: Belletristik, Sachbuch, Fachbuch oder Kinderbuch. Dazu sind bis zu 20 freie Schlagwörter mit jeweils 60 Zeichen möglich. Schlagwörter werden mit Komma oder Semikolon getrennt, in Kleinbuchstaben gespeichert und ohne Duplikate zusammengeführt. Kategorien und Schlagwörter lassen sich beim Upload oder unter „Buch verwalten“ setzen. Die Bibliothek bietet kombinierbare Filter für Kategorie, Schlagwort, Lesestatus und Suchtext. „Ohne Kategorie“ zeigt noch nicht eingeordnete Bücher.

EPUB-Themen (`dc:subject`) liefern Vorschläge für Schlagwörter und anhand einfacher Themenbegriffe für eine Kategorie. Diese gelten erst nach „Übernehmen“ als Einordnung. „Verwerfen“ entfernt den Vorschlag. Eigene Angaben beim Upload haben Vorrang. Beim Übernehmen weiterer Vorschläge bleiben eine bereits gewählte Kategorie und vorhandene Schlagwörter erhalten (insgesamt höchstens 20). Manuelles Speichern bestätigt die eigene Einordnung und verwirft offene Vorschläge. Bei vorhandenen Büchern kann „Vorschläge aus Datei ermitteln“ die EPUB-Themen erneut lesen. Für PDF/MOBI/AZW3 erfolgt die Klassifizierung derzeit manuell; Titel und Autor werden weiterhin automatisch erkannt.

Bestehende Datenbanken werden beim Start transaktional und ohne Löschen vorhandener Bücher erweitert. Bestehende Bücher starten ohne Kategorie und Schlagwörter.

## Docker

```sh
docker compose up --build -d
```

Öffnen: http://localhost:8080

```sh
docker compose logs -f
docker compose down
```

Das benannte Volume `library` enthält `/data/library.sqlite` und die Originaldateien in `/data/books`. `docker compose down` erhält diese Daten; `down -v` löscht sie. Der Container läuft als Benutzer 10001. Die Portfreigabe ist auf den lokalen Rechner beschränkt. Es gibt keine Benutzeranmeldung: Für Zugriff über das Netzwerk einen Reverse Proxy mit Authentifizierung und HTTPS vorschalten.

## Lokal entwickeln

Rust 1.90 oder neuer:

```sh
cargo run
cargo test
```

Ende-zu-Ende-Test gegen den gestarteten Dienst (Python 3, erzeugt und entfernt ein Testbuch):

```sh
python tests/smoke.py
python tests/metadata.py
python tests/classification.py
```

Ein separater Docker-Test prüft die Migration einer alten Datenbank und den Datenerhalt nach einem Neustart in einem temporären Container:

```sh
python tests/migration.py
```

`DATA_DIR` (Standard `data`) und `LISTEN_ADDR` (Standard `127.0.0.1:8080`) sind konfigurierbar. Im Container wird auf `0.0.0.0:8080` gelauscht. Dioxus rendert die Oberfläche auf dem Server; Formulare funktionieren ohne JavaScript oder WASM-Build. Schriftarten werden optional über Google Fonts geladen; lokale Ersatzschriften sind vorhanden.

## Datensicherung

Für eine konsistente Sicherung den Dienst anhalten und das gesamte Volume inklusive SQLite-WAL-Dateien und Buchverzeichnis kopieren. Anschließend wieder starten. Einzelne Datenbankdateien nicht im laufenden Betrieb kopieren.

## Schnittstellen

- `GET /health`: SQLite-Verbindungsprüfung
- `GET /?q=…&status=reading`: Suche und Filter (`unread`, `reading`, `read`)
- `POST /books`: Multipart-Felder `file`, `title`, `author`
- `POST /books/{id}`: Formularfelder `title`, `author`, `status`
- `GET /books/{id}/download`: Originaldatei
- `POST /books/{id}/delete`: Buch und Datei entfernen
- `POST /books/{id}/suggest`: EPUB-Themen als Vorschläge ermitteln
- `POST /books/{id}/accept`: Klassifizierungsvorschläge bestätigen
- `POST /books/{id}/dismiss`: Klassifizierungsvorschläge verwerfen

Upload und Bearbeitung unterstützen zusätzlich `category` und `tags`; die Suche unterstützt `category` (oder `__none`) und `tag`. Wenn ältere Clients diese Felder beim Bearbeiten weglassen, bleibt die vorhandene Klassifizierung erhalten.

Uploads werden anhand der Dateiendung eingeschränkt, nicht inhaltlich auf Schadsoftware oder valide E-Book-Strukturen geprüft. Dateinamen werden nicht als Speicherpfade verwendet. SQL-Abfragen sind parametrisiert; Dioxus maskiert Textausgaben. Suche verwendet SQLite `lower()`: Unicode-Groß-/Kleinschreibung außerhalb von ASCII ist eingeschränkt.
