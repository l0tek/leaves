# Projektstatus – Leaves

Stand: 29. Oktober 2026. Grundlage: Quellcode, Konfiguration und README im Repository.

## Überblick

Leaves ist eine selbst gehostete E-Book-Verwaltung mit deutscher Oberfläche.
Das Projekt liegt in Version `0.1.0` vor. Der vorhandene Funktionsumfang ist
implementiert. Build, Unit-Tests und die unten aufgeführten Integrationstests
sind erfolgreich; eine Produktionsfreigabe ist damit nicht verbunden.

## Vorhandene Funktionen

- Upload und Download von EPUB, PDF, MOBI und AZW3, Metadatenbearbeitung und Löschen.
- Suche, Lesestatus sowie Filter für Autor, Kategorien und Schlagwörter.
  Autoren und Schlagwörter sind direkt im Regal als Filter verlinkt.
- EPUB-Reader mit epub.js, Kapitelwahl, Blättern, Schriftgröße und lokal
  gespeicherter Leseposition; anklickbare Autoren öffnen die gefilterte Bibliothek.
- Automatisches Auslesen von Titel und Autor aus Datei-Metadaten.
- Ordnerimport mit Einzeldatei-Ergebnissen und SHA-256-Duplikaterkennung.
- Klassifizierungsvorschläge aus EPUB-Themen mit Übernehmen und Verwerfen.
- Ollama-Anbindung für eine bearbeitbare Einzelvorschau und automatische
  Ergänzung fehlender Metadaten im Sammellauf mit Fortschritt und Stoppfunktion.
- SQLite-Schemaerweiterungen beim Start und persistente Speicherung im Docker-Volume.

## Technischer Aufbau

| Bereich | Umsetzung |
| --- | --- |
| Server | Rust, Axum, Tokio |
| Oberfläche | Dioxus SSR, CSS; Formulare ohne JavaScript |
| Datenhaltung | SQLite über SQLx; Originaldateien im Buchverzeichnis |
| KI-Anbindung | Serverseitige Anfragen an konfigurierbares Ollama |
| Betrieb | Dockerfile und Docker Compose, Port 8080 |

`src/main.rs` enthält Server, Routen und Oberfläche; `src/metadata.rs` die
Dateiauswertung, `src/classification.rs` die Klassifizierung, `src/dedup.rs`
die Duplikaterkennung und `src/ollama.rs` die Modellanbindung und Sammelläufe.
`src/reader.rs` und `src/reader.js` enthalten den EPUB-Reader; lokale
Reader-Bibliotheken und Lizenzen liegen in `src/vendor`. Die Gestaltung liegt in `src/style.css`.

## Betrieb und Grenzen

- Start mit `docker compose up --build -d`; Oberfläche unter `http://localhost:8080`.
- Das Volume `library` enthält `/data/library.sqlite` und `/data/books`.
- Lokal sind `DATA_DIR` und `LISTEN_ADDR` konfigurierbar.
- `OLLAMA_URL` und `OLLAMA_MODEL` werden über die Umgebung konfiguriert;
  Compose enthält eine voreingestellte Adresse im lokalen Netzwerk.
- Keine Benutzeranmeldung: Netzwerkzugriff benötigt eine entsprechend
  geschützte Bereitstellung, wie in der README beschrieben.
- Der Reader unterstützt EPUB und benötigt JavaScript; keine DRM-Entfernung und keine OCR.
- Uploadgrenzen: 50 MiB pro Buch, 1 GiB pro Ordnerimport-Anfrage.
- Ollama-Sammelläufe liegen im Arbeitsspeicher und überstehen keinen Neustart;
  bereits gespeicherte Ergänzungen bleiben erhalten.
- Uploads werden anhand der Endung eingeschränkt; eine Schadsoftwareprüfung
  ist nicht implementiert. Die Suche hat Einschränkungen bei Unicode-Fallvergleichen.

## Prüfstand

Am 29. Oktober 2026 gegen isolierte temporäre Testdaten geprüft:

- `cargo build` und `cargo test` erfolgreich; drei Rust-Unit-Tests bestanden.
- `tests/smoke.py`, `tests/metadata.py`, `tests/classification.py` und
  `tests/batch.py` erfolgreich gegen eine isolierte lokale Instanz.
- `tests/reader.py` erfolgreich: exakter Autorfilter mit Unicode und
  Sonderzeichen, kombinierte Filter, Reader-Routen, lokale JavaScript-Dateien,
  Originaldownload und Fehlerantworten für fehlende/nicht unterstützte Bücher.
- Schlagwortlinks zusätzlich gegen eine isolierte Instanz geprüft: Unicode,
  Sonderzeichen und exakt passende Ergebnisliste; Klassifizierungstests erneut erfolgreich.
- Zusätzlicher Chromium-Test über `LEAVES_BROWSER_TEST=1` erfolgreich:
  EPUB-Darstellung, Blättern, Kapitelwahl, Schriftgröße, Wiederherstellung der
  Leseposition, Sandbox ohne EPUB-Skripte, mobile Ansicht, Autorenlink und
  Betrieb ohne zugänglichen Local Storage.
- Rust wurde für die Prüfung installiert; C-Werkzeugkette, Node.js und fehlende
  Browserbibliotheken wurden außerhalb des Projekts temporär bereitgestellt.
- Docker-Build und Docker-Migrationstest nicht ausgeführt: Zugriff auf den
  Docker-Socket ist in dieser Umgebung nicht erlaubt.
- Erreichbarkeit des echten Ollama-Dienstes und Zustand einer laufenden
  Installation wurden nicht geprüft.

## Nächste sinnvolle Schritte

Die folgenden Punkte sind Empfehlungen, keine bereits vereinbarten Aufgaben:

1. Container mit `docker compose up --build -d` neu bauen und starten, um den
   Reader in der Docker-Installation verfügbar zu machen.
2. Den Docker-Migrationstest bei verfügbarem Docker-Zugriff ausführen.
3. Vor einer Bereitstellung Netzwerkzugriff, Ollama-Konfiguration und Sicherung
   des Datenvolumes prüfen.

Diese Datei bei Änderungen am Funktionsumfang, Betriebsmodell oder Prüfstand
aktualisieren. Bedienung und ausführliche Startanweisungen stehen in `README.md`.
