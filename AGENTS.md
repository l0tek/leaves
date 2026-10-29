# Arbeitsanweisungen für Codex und andere Coding-Agenten

## Projektkontext

Leaves ist eine E-Book-Verwaltung mit Rust (Edition 2024), Axum, Dioxus SSR
und SQLite. Lies für Bedienung und Betrieb `README.md`, für den dokumentierten
Stand `PROJEKTSTATUS.md`. Kommunikation, Oberfläche und Projektdokumentation
sollen auf Deutsch bleiben; bestehende Code-Bezeichner folgen dem vorhandenen Stil.

## Orientierung im Code

- `src/main.rs`: Start, App-Zustand, Datenbankinitialisierung, HTTP-Routen und SSR.
- `src/metadata.rs`: Metadaten und Textauszüge aus E-Book-Dateien.
- `src/classification.rs`: Kategorien, Schlagwörter und Vorschläge.
- `src/dedup.rs`: Dateihashes und Duplikaterkennung.
- `src/ollama.rs`: Ollama-Anfragen, Vorschauen und Metadaten-Sammelläufe.
- `src/reader.rs`, `src/reader.js`: EPUB-Reader mit lokal ausgelieferten Bibliotheken
  in `src/vendor` (Versionen und Lizenzen dort dokumentiert).
- `src/style.css`: Darstellung.
- `tests/*.py`: Integrationstests mit Python-Standardbibliothek.
- `compose.yaml` und `Dockerfile`: Containerbetrieb.

## Regeln für Änderungen

- Bestehende Änderungen des Nutzers erhalten und Änderungen auf den Auftrag begrenzen.
- Den SSR-Ansatz und ohne JavaScript nutzbare Formulare beibehalten, sofern der
  Auftrag keine andere Architektur verlangt.
- Originaldateien und vorhandene Buchdaten erhalten. Schemaänderungen müssen
  bestehende Datenbanken berücksichtigen und dürfen keine Bibliothek zurücksetzen.
- Parametrisierte SQL-Abfragen, sichere interne Dateipfade und Uploadgrenzen erhalten.
- Manuell gepflegte Metadaten bei automatischen Ergänzungen schützen. Die
  Einzelvorschau benötigt Übernehmen; der Sammellauf speichert fehlende Angaben
  automatisch. Diese unterschiedlichen Abläufe berücksichtigen.
- Buchinhalte nur im Rahmen der beauftragten Funktion an den konfigurierten
  Ollama-Dienst senden; für Entwicklung und Tests synthetische Dateien verwenden.
- Keine echten Bibliotheken oder produktiven Dienste als Testdaten verwenden.
- `docker compose down -v` löscht die Bibliothek und darf nicht als routinemäßiger
  Entwicklungs- oder Prüfschritt verwendet werden.
- Bei Änderungen am Verhalten die README und bei geändertem Projektstand
  `PROJEKTSTATUS.md` mitpflegen. Ungeprüfte Annahmen als solche kennzeichnen.

## Entwicklung und Prüfung

Lokale Entwicklung benötigt laut README Rust 1.90 oder neuer. Übliche Befehle:

```sh
cargo run
cargo test
cargo build
docker compose up --build -d
```

Wähle Prüfungen passend zur Änderung. Für reine Dokumentationsänderungen genügen
Inhalts- und Diff-Prüfung. Für Rust-Änderungen die relevanten Unit-Tests und bei
geändertem HTTP- oder Speicherverhalten passende Integrationstests ausführen.

- Gegen eine gestartete, isolierte Testinstanz: `tests/smoke.py`,
  `tests/metadata.py`, `tests/classification.py`, `tests/batch.py`.
- Mit lokal gebautem Binary und temporären Daten: `tests/dedup.py`,
  `tests/ollama.py`, `tests/ollama_batch.py`, `tests/reader.py`.
  Mit `LEAVES_BROWSER_TEST=1` führt der Reader-Test zusätzlich Playwright aus
  (Node.js, Playwright und Chromium erforderlich).
- Mit Docker und passendem Image: `tests/migration.py` für Altdatenmigration
  und Persistenz nach Neustart.

Vor der Ausführung die Voraussetzungen des jeweiligen Skripts lesen. Ausführliche
Befehle stehen in der README. Fehlende Werkzeuge oder nicht ausgeführte Tests
im Ergebnis ausdrücklich nennen; vorhandene Tests sind kein Erfolgsnachweis.
