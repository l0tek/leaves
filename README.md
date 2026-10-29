# Leaves – E-Book-Verwaltung

Rust, Dioxus SSR, Axum und SQLite. Deutsche, responsive Oberfläche mit Upload, Titel-/Autorsuche, Lesestatus, Metadatenbearbeitung, Download und bestätigtem Löschen. Unterstützt EPUB, PDF, MOBI und AZW3 bis 50 MiB. Die Buchcover sind typografische Platzhalter. Kein integrierter Reader, keine DRM-Entfernung.

Beim Hochladen werden leere Titel-/Autorfelder automatisch aus Metadaten ergänzt: EPUB (OPF), PDF (Info-Dictionary) und MOBI/AZW3 (MOBI-/EXTH-Header). Mehrere Autoren werden zusammengeführt. Manuelle Eingaben haben pro Feld Vorrang. Fehlen lesbare Metadaten, dient der Dateiname als Titel und der Autor bleibt leer. Die Erkennung erfolgt beim Absenden des Uploads, ohne externe Dienste; fehlerhafte oder verschlüsselte Metadaten können unerkannt bleiben. Bereits gespeicherte Bücher bleiben unverändert.

## Ordnerimport

Unter „Ordner importieren“ einen lokalen Ordner im Browser auswählen und den Import starten. Alle Dateien einschließlich Unterordnern werden übertragen; EPUB, PDF, MOBI und AZW3 (auch mit großgeschriebener Endung) werden importiert. Andere Dateien werden übersprungen. Maximal 50 MiB pro Buch und 1 GiB für die gesamte Anfrage. Größere Sammlungen in mehreren Durchläufen importieren. Die Ordnerauswahl muss vom Browser unterstützt werden; es ist kein Serverpfad und kein Docker-Mount erforderlich.

Titel, Autor und Klassifizierungsvorschläge werden wie beim Einzelupload ermittelt. Die Dateien werden nacheinander verarbeitet. Der Importbericht zählt erfolgreiche, übersprungene und fehlgeschlagene Dateien und nennt Fehler pro Buch. Ein fehlerhaftes Buch stoppt die übrigen Importe nicht. Bei einer abgebrochenen Übertragung bleiben bereits importierte Bücher erhalten. Identische Dateien werden anhand ihres SHA-256-Hashes übersprungen, auch bei anderem Namen oder Ordner. Der Bericht nennt die Anzahl bereits vorhandener Dateien. Auch Einzeluploads erkennen Duplikate und zeigen einen Hinweis. Unterschiedliche Dateiinhalte (etwa EPUB und PDF desselben Buchs) gelten als verschiedene Bücher. Beim Start werden Hashes vorhandener Bücher nachgetragen; bereits vorhandene Duplikate bleiben erhalten. Ist eine vorhandene Datei nicht lesbar, bricht der Start mit einem Speicherfehler ab, damit die Erkennung vollständig bleibt. Ein Datenbankindex verhindert auch bei gleichzeitigen Importen neue Duplikate. Die Quelldateien bleiben unverändert.

## Metadaten mit Ollama ergänzen

Unter „Buch verwalten“ auf „Metadaten mit Ollama ergänzen“ klicken. Leaves sendet vorhandene Angaben, Datei-Metadaten und bei EPUB/PDF einen kurzen Textauszug an deinen Ollama-Dienst. EPUB: bis zu sechs HTML-Dateien in Lesereihenfolge mit jeweils bis zu 2000 Zeichen; PDF: bis zu drei Seiten, ohne OCR; maximal 8000 Zeichen Text. Für MOBI/AZW3 werden nur Metadaten verwendet. Die Originaldatei bleibt unverändert.

Die Antwort erscheint als bearbeitbare Vorschau für Titel, Autor, Kategorie und Schlagwörter. Erst „Übernehmen“ speichert sie; „Verwerfen“ kehrt ohne Änderung zur Bibliothek zurück. Vorhandene Titel und Autoren werden beibehalten. Fehlende Angaben werden bevorzugt aus Datei-Metadaten ergänzt; Titel und Autor werden vom Modell nicht erzeugt. Das Modell erzeugt ausschließlich Hauptkategorie und bis zu sechs Schlagwörter. Bei vorhandenem Buchtext (mindestens 20 Wörter) oder Datei-Themen verwendet es diese als Grundlage. Andernfalls kann es ein bekanntes Werk anhand von Titel und Autor vorläufig einordnen; die Vorschau kennzeichnet diese schwächere Grundlage. Ohne Buchinhalt und vollständige Titel-/Autorangaben erfolgt kein Modellaufruf. Unbekannte Werke dürfen ohne Einordnung bleiben; die Oberfläche fordert dann zur manuellen Auswahl auf. Die Vorschau zeigt die verwendeten Datei-Metadaten, Themen und den Textauszug. Leere Modellwerte behalten die vorhandenen Angaben. Das Modell kann sich irren: Vorschläge insbesondere für Titel und Autor prüfen. Anfragen laufen einzeln mit maximal drei Minuten Wartezeit; bei Fehlern bleiben die Buchdaten erhalten.

Auf der Bibliotheksseite startet „Fehlende Metadaten mit Ollama ergänzen“ einen automatischen Sammellauf für alle Bücher, bei denen Hauptkategorie, Schlagwörter oder Autor fehlen. Der Lauf arbeitet die beim Start gefundene Liste nacheinander ab und speichert ausschließlich fehlende Angaben. Vorhandene Titel, Autoren, Kategorien, Schlagwörter und Lesestatus bleiben erhalten, auch bei manuellen Änderungen während einer Modellanfrage. Fehlende Autoren kommen ausschließlich aus Datei-Metadaten; bei Büchern, denen nur der Autor fehlt, wird kein Modellaufruf benötigt. Auch vorläufige Einordnungen anhand von Titel und Autor werden im Sammellauf automatisch gespeichert und im Ergebnis gekennzeichnet.

Die Fortschrittsseite zeigt das aktuelle Buch, einen Fortschrittsbalken mit Prozentangabe und Zähler für ergänzte, unveränderte und fehlgeschlagene Bücher. Sie aktualisiert sich alle drei Sekunden ohne JavaScript und zeigt die letzten 100 Ergebnisse. Die Verarbeitung läuft auch nach Schließen der Seite weiter. „Nach aktuellem Buch stoppen“ beendet den Lauf nach der laufenden Anfrage (bis zu drei Minuten). Einzelne Fehler stoppen die übrigen Bücher nicht. Doppelte Starts öffnen den laufenden Fortschritt; Einzelanfragen melden währenddessen „beschäftigt“ und können danach erneut gestartet werden. Fortschritt und Auftragsliste liegen im Arbeitsspeicher: Ein Dienstneustart beendet den Lauf, bereits gespeicherte Angaben bleiben erhalten. Ein erneuter Start ermittelt wieder alle noch unvollständigen Bücher. Vollständige Bücher werden nicht erneut verarbeitet; unbekannte oder unlesbare Bücher können unvollständig bleiben.

Docker-Vorgaben: `OLLAMA_URL=http://192.168.113.103:11434`, `OLLAMA_MODEL=llama3.2:latest`. Über `.env` oder Umgebungsvariablen anpassbar; danach `docker compose up -d` ausführen. Die URL muss aus dem Container erreichbar sein. Die Anbindung verwendet die Ollama-[Chat-API](https://docs.ollama.com/api/chat) mit strukturierten JSON-Antworten. Alle Modellaufrufe erfolgen serverseitig, direkt an die konfigurierte Adresse.

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

Das benannte Volume `library` enthält `/data/library.sqlite` und die Originaldateien in `/data/books`. `docker compose down` erhält diese Daten; `down -v` löscht sie. Der Container läuft als Benutzer 10001. Port 8080 ist auf allen Netzwerkschnittstellen freigegeben. Es gibt keine Benutzeranmeldung: Für Zugriff über das Netzwerk einen Reverse Proxy mit Authentifizierung und HTTPS vorschalten.

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
python tests/batch.py
# Isolierter Test mit temporärer Datenbank (lokales Binary erforderlich):
cargo build
python tests/dedup.py
python tests/ollama.py
python tests/ollama_batch.py
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
- `POST /books/import`: Wiederholtes Multipart-Feld `files` mit Dateien; HTML-Importbericht
- `POST /books/{id}`: Formularfelder `title`, `author`, `status`
- `GET /books/{id}/download`: Originaldatei
- `POST /books/{id}/delete`: Buch und Datei entfernen
- `POST /ollama/batch`: Automatischen Sammellauf für unvollständige Bücher starten
- `GET /ollama/batch`: Fortschrittsanzeige und letzte Ergebnisse
- `POST /ollama/batch/cancel`: Nach aktuellem Buch stoppen
- `POST /books/{id}/ollama`: Ollama-Vorschau erzeugen (Übernehmen über `POST /books/{id}`)
- `POST /books/{id}/suggest`: EPUB-Themen als Vorschläge ermitteln
- `POST /books/{id}/accept`: Klassifizierungsvorschläge bestätigen
- `POST /books/{id}/dismiss`: Klassifizierungsvorschläge verwerfen

Upload und Bearbeitung unterstützen zusätzlich `category` und `tags`; die Suche unterstützt `category` (oder `__none`) und `tag`. Wenn ältere Clients diese Felder beim Bearbeiten weglassen, bleibt die vorhandene Klassifizierung erhalten.

Uploads werden anhand der Dateiendung eingeschränkt, nicht inhaltlich auf Schadsoftware oder valide E-Book-Strukturen geprüft. Dateinamen werden nicht als Speicherpfade verwendet. SQL-Abfragen sind parametrisiert; Dioxus maskiert Textausgaben. Suche verwendet SQLite `lower()`: Unicode-Groß-/Kleinschreibung außerhalb von ASCII ist eingeschränkt.
