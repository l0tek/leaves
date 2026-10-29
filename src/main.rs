use axum::{Router, extract::{State, Path, Query, Multipart, DefaultBodyLimit}, response::{Html, IntoResponse, Response, Redirect}, routing::{get, post}, Form, http::{StatusCode, header}};
use dioxus::prelude::*;
use serde::Deserialize;
use sqlx::{SqlitePool, Row, sqlite::{SqliteConnectOptions, SqlitePoolOptions}};
use std::path::PathBuf;
mod metadata;
mod dedup;
mod ollama;
mod classification;
mod reader;

#[derive(Clone)]
struct AppState { db: SqlitePool, files: PathBuf, ollama_client: reqwest::Client, ollama_url: String, ollama_model: String, ollama_slots: std::sync::Arc<tokio::sync::Semaphore>, ollama_batch: std::sync::Arc<tokio::sync::Mutex<Option<ollama::BatchJob>>> }
struct AppError(StatusCode, String);
impl IntoResponse for AppError {
    fn into_response(self) -> Response { (self.0, self.1).into_response() }
}
impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self { tracing::error!("Database: {e}"); Self(StatusCode::INTERNAL_SERVER_ERROR, "Datenbankfehler".into()) }
}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { tracing::error!("Storage: {e}"); Self(StatusCode::INTERNAL_SERVER_ERROR, "Speicherfehler".into()) }
}
fn bad(message: &str) -> AppError { AppError(StatusCode::BAD_REQUEST, message.into()) }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let data = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into()));
    let files = data.join("books");
    tokio::fs::create_dir_all(&files).await?;
    let options = SqliteConnectOptions::new().filename(data.join("library.sqlite"))
        .create_if_missing(true).foreign_keys(true).journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let db = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS books (id TEXT PRIMARY KEY, title TEXT NOT NULL, author TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'unread' CHECK(status IN ('unread','reading','read')), format TEXT NOT NULL, size INTEGER NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)").execute(&db).await?;
    classification::migrate(&db).await?;
    dedup::migrate(&db, &files).await?;
    let ollama_client = reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(5)).timeout(std::time::Duration::from_secs(180)).build()?;
    let ollama_url = std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://192.168.113.103:11434".into());
    let ollama_model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:latest".into());
    let app = Router::new()
        .route("/", get(index))
        .route("/books", post(upload))
        .route("/books/import", post(batch_upload).layer(DefaultBodyLimit::max(1024 * 1024 * 1024)))
        .route("/books/{id}", post(update))
        .route("/books/{id}/delete", post(delete))
        .route("/books/{id}/download", get(download))
        .route("/books/{id}/read", get(reader::page))
        .route("/reader.js", get(reader::script))
        .route("/vendor/epub.min.js", get(reader::epub_script))
        .route("/vendor/jszip.min.js", get(reader::zip_script))
        .route("/books/{id}/suggest", post(suggest_classification))
        .route("/books/{id}/ollama", post(ollama::suggest))
        .route("/ollama/batch", get(ollama::progress).post(ollama::start_batch))
        .route("/ollama/batch/cancel", post(ollama::cancel_batch))
        .route("/books/{id}/accept", post(accept_classification))
        .route("/books/{id}/dismiss", post(dismiss_classification))
        .route("/style.css", get(|| async { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], include_str!("style.css")) }))
        .route("/health", get(health))
        .layer(DefaultBodyLimit::max(52 * 1024 * 1024))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(AppState { db, files, ollama_client, ollama_url, ollama_model, ollama_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(1)), ollama_batch: std::sync::Arc::new(tokio::sync::Mutex::new(None)) });
    let addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Leaves läuft auf http://{addr}");
    axum::serve(listener, app).with_graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; }).await?;
    Ok(())
}

#[derive(Deserialize, Default)]
struct Search { #[serde(default)] author: String, #[serde(default)] q: String, #[serde(default)] status: String, #[serde(default)] category: String, #[serde(default)] tag: String, #[serde(default)] notice: String }
#[derive(Clone)]
struct Book { id: String, title: String, author: String, status: String, format: String, size: i64, category: String, tags: Vec<String>, suggested_category: String, suggested_tags: String }

async fn index(State(s): State<AppState>, Query(search): Query<Search>) -> Result<Html<String>, AppError> {
    let rows = sqlx::query("SELECT books.id,books.title,books.author,books.status,books.format,books.size,books.category,books.suggested_category,books.suggested_tags, (SELECT group_concat(name, ', ') FROM (SELECT name FROM book_tags WHERE book_id=books.id ORDER BY name)) AS tags FROM books WHERE (instr(lower(title),lower(?)) > 0 OR instr(lower(author),lower(?)) > 0) AND (? = '' OR status = ?) AND (? = '' OR category = ? OR (? = '__none' AND category = '')) AND (? = '' OR EXISTS(SELECT 1 FROM book_tags WHERE book_id=books.id AND name=?)) AND (? = '' OR author = ?) ORDER BY created_at DESC, id")
        .bind(&search.q).bind(&search.q).bind(&search.status).bind(&search.status)
        .bind(&search.category).bind(&search.category).bind(&search.category)
        .bind(&search.tag).bind(search.tag.trim().to_lowercase())
        .bind(&search.author).bind(&search.author).fetch_all(&s.db).await?;
    let books: Vec<Book> = rows.iter().map(|r| Book { id: r.get("id"), title: r.get("title"), author: r.get("author"), status: r.get("status"), format: r.get("format"), size: r.get("size"), category: r.get("category"), tags: r.get::<Option<String>, _>("tags").unwrap_or_default().split(", ").filter(|t| !t.is_empty()).map(str::to_string).collect(), suggested_category: r.get("suggested_category"), suggested_tags: r.get("suggested_tags") }).collect();
    let available_tags: Vec<String> = sqlx::query_scalar("SELECT DISTINCT name FROM book_tags ORDER BY name").fetch_all(&s.db).await?;
    let stats = sqlx::query("SELECT count(*) AS total, coalesce(sum(status='read'),0) AS finished, coalesce(sum(status='reading'),0) AS reading FROM books").fetch_one(&s.db).await?;
    let total: i64 = stats.get("total");
    let finished: i64 = stats.get("finished");
    let reading: i64 = stats.get("reading");
    let count = books.len();
    let missing_metadata: i64 = sqlx::query_scalar("SELECT count(*) FROM books WHERE trim(category)='' OR trim(author)='' OR NOT EXISTS(SELECT 1 FROM book_tags WHERE book_id=books.id)").fetch_one(&s.db).await?;
    let batch_running = s.ollama_batch.lock().await.as_ref().is_some_and(|job| job.running);
    let content = dioxus_ssr::render_element(rsx! {
            head {
                meta { charset: "utf-8" }
                meta { name: "viewport", content: "width=device-width, initial-scale=1" }
                title { "Leaves · Deine Bibliothek" }
                link { rel: "stylesheet", href: "/style.css" }
            }
            body {
                header { class: "topbar", a { class: "brand", href: "/", span { "◒" } " leaves" } span { class: "tagline", "Ein Zuhause für deine Bücher" } a { class: "button primary", href: "#upload", "+ Buch hinzufügen" } }
                main {
                    section { class: "intro", div { class: "eyebrow", "DEINE PERSÖNLICHE SAMMLUNG" } h1 { "Zwischen den Zeilen." } p { "Alle deine E-Books. Ein Ort, um Neues zu entdecken und Geschichten aufzubewahren." } }
                    section { class: "stats", "aria-label": "Bibliotheksübersicht",
                        div { strong { "{total}" } span { "Bücher im Regal" } }
                        div { strong { "{reading}" } span { "Gerade am Lesen" } }
                        div { strong { "{finished}" } span { "Zu Ende gelesen" } }
                    }
                    section { class: "upload",
                        h2 { "Fehlende Metadaten ergänzen" }
                        p { "{missing_metadata} Bücher ohne Hauptkategorie, Schlagwörter oder Autor. Ollama ergänzt fehlende Einordnungen automatisch; Autoren werden nur aus Datei-Metadaten übernommen. Vorhandene Angaben bleiben erhalten." }
                        p { class: "field-hint", "Auch vorläufige Einordnungen anhand von Titel und Autor werden gespeichert. Bitte die Ergebnisse prüfen. Die Verarbeitung läuft im Hintergrund weiter, wenn du die Seite verlässt." }
                        if batch_running { a { class: "button primary", href: "/ollama/batch", "Fortschritt anzeigen" } }
                        else { form { method: "post", action: "/ollama/batch", button { class: "primary", disabled: missing_metadata == 0, "Fehlende Metadaten mit Ollama ergänzen" } } }
                        a { href: "/ollama/batch", "Letzten Sammellauf anzeigen" }
                    }
                    div { class: "library-heading", h2 { "Dein Bücherregal" } span { "{count} Ergebnisse" } }
                    if search.notice == "duplicate" { p { class: "notice", role: "status", "Diese Datei ist bereits in deiner Bibliothek. Der Import wurde übersprungen." } }
                    if search.notice == "no-subjects" { p { class: "notice", role: "status", "Keine verwertbaren Themen-Metadaten gefunden. Kategorie und Schlagwörter kannst du unter „Buch verwalten“ selbst festlegen." } }
                    if !search.author.is_empty() { p { class: "notice", "Bücher von {search.author}" } }
                    form { class: "search", method: "get", action: "/",
                        input { name: "author", r#type: "hidden", value: "{search.author}" }
                        input { name: "q", r#type: "search", value: "{search.q}", placeholder: "Nach Titel oder Autor suchen …", "aria-label": "Titel oder Autor" }
                        select { name: "status", "aria-label": "Lesestatus",
                            option { value: "", selected: search.status.is_empty(), "Alle Bücher" }
                            option { value: "unread", selected: search.status == "unread", "Ungelesen" }
                            option { value: "reading", selected: search.status == "reading", "Lese ich gerade" }
                            option { value: "read", selected: search.status == "read", "Gelesen" }
                        }
                        select { name: "category", "aria-label": "Kategorie filtern",
                            option { value: "", selected: search.category.is_empty(), "Alle Kategorien" }
                            option { value: "__none", selected: search.category == "__none", "Ohne Kategorie" }
                            for category in classification::CATEGORIES { option { value: category, selected: search.category == category, "{category}" } }
                        }
                        select { name: "tag", "aria-label": "Schlagwort filtern",
                            option { value: "", selected: search.tag.is_empty(), "Alle Schlagwörter" }
                            for tag in &available_tags { option { value: "{tag}", selected: search.tag == *tag, "{tag}" } }
                        }
                        button { r#type: "submit", "Filtern" }
                        a { href: "/", "Zurücksetzen" }
                    }
                    section { class: "books",
                        if books.is_empty() { div { class: "empty", h3 { "Platz für die nächste Geschichte." } p { "Lade dein erstes Buch hoch oder passe deine Suche an." } a { href: "#upload", "Ein Buch hinzufügen →" } } }
                        for (i, book) in books.iter().enumerate() {
                            article { class: "book",
                                div { class: "cover cover-{i % 4}",
                                    span { class: "cover-format", "{book.format}" }
                                    div { class: "cover-ornament", "aria-hidden": "true" }
                                    h3 { class: if book.title.chars().count() > 55 { "long-title" } else { "" }, lang: "de", "{book.title}" }
                                    p { class: "cover-author", if !book.author.is_empty() { a { href: author_url(&book.author), "{book.author}" } } }
                                }
                                div { class: "book-info",
                                    span { class: "badge", "{status_label(&book.status)}" }
                                    h3 { "{book.title}" }
                                    p { if !book.author.is_empty() { a { class: "author-link", href: author_url(&book.author), "{book.author}" } } }
                                    div { class: "classification",
                                        span { class: "category", if book.category.is_empty() { "Ohne Kategorie" } else { "{book.category}" } }
                                        for tag in &book.tags { a { class: "tag", href: filter_url("tag", tag), "{tag}" } }
                                    }
                                    if !book.suggested_category.is_empty() || !book.suggested_tags.is_empty() {
                                        div { class: "suggestion",
                                            strong { "Vorschlag aus Metadaten" }
                                            if !book.suggested_category.is_empty() { p { "Kategorie: {book.suggested_category}" } }
                                            if !book.suggested_tags.is_empty() { p { "Schlagwörter: {book.suggested_tags}" } }
                                            form { method: "post", action: "/books/{book.id}/accept",
                                                button { "Übernehmen" }
                                                button { class: "secondary", formaction: "/books/{book.id}/dismiss", "Verwerfen" }
                                            }
                                        }
                                    }
                                    small { "{book.format} · {book.size / 1024} KB" }
                                    if book.format == "epub" { a { class: "button primary read-link", href: "/books/{book.id}/read", "Jetzt lesen" } }
                                    a { class: "download", href: "/books/{book.id}/download", "Herunterladen ↓" }
                                    details { summary { "Buch verwalten" }
                                        form { method: "post", action: "/books/{book.id}",
                                            label { "Titel", input { name: "title", value: "{book.title}", required: true, maxlength: "300" } }
                                            label { "Autor", input { name: "author", value: "{book.author}", maxlength: "200" } }
                                            label { "Lesestatus", select { name: "status",
                                                option { value: "unread", selected: book.status == "unread", "Ungelesen" }
                                                option { value: "reading", selected: book.status == "reading", "Lese ich gerade" }
                                                option { value: "read", selected: book.status == "read", "Gelesen" }
                                            } }
                                            label { "Hauptkategorie", select { name: "category",
                                                option { value: "", selected: book.category.is_empty(), "Ohne Kategorie" }
                                                for category in classification::CATEGORIES { option { value: category, selected: book.category == category, "{category}" } }
                                            } }
                                            label { "Schlagwörter", input { name: "tags", value: book.tags.join(", "), maxlength: "1300", placeholder: "z. B. weltraum, abenteuer" } }
                                            span { class: "field-hint", "Mit Komma trennen. Speichern bestätigt deine Einordnung und verwirft offene Vorschläge." }
                                            button { "Speichern" }
                                        }
                                        form { method: "post", action: "/books/{book.id}/ollama",
                                            p { class: "field-hint", "Ollama erhält Metadaten und einen kurzen Textauszug. Vorschläge vor dem Speichern prüfen. Kann bis zu drei Minuten dauern." }
                                            button { "Metadaten mit Ollama ergänzen" }
                                        }
                                        form { method: "post", action: "/books/{book.id}/suggest", button { "Vorschläge aus Datei ermitteln" } }
                                        form { method: "post", action: "/books/{book.id}/delete", label { class: "confirm", input { r#type: "checkbox", required: true } "Löschen bestätigen" } button { class: "danger", "Buch endgültig löschen" } }
                                    }
                                }
                            }
                        }
                    }
                    section { id: "batch", class: "upload",
                        div { class: "eyebrow", "VIELE GESCHICHTEN AUF EINMAL" }
                        h2 { "Ordner importieren" }
                        p { "Wähle einen Ordner auf deinem Rechner. Unterordner werden einbezogen; EPUB, PDF, MOBI und AZW3 werden importiert. Andere Dateien werden übersprungen." }
                        form { method: "post", action: "/books/import", enctype: "multipart/form-data",
                            label { "Bücherordner", input { r#type: "file", name: "files", multiple: true, directory: true, required: true } }
                            p { class: "field-hint", "Maximal 50 MiB pro Buch und 1 GiB pro Durchlauf. Identische Dateien werden übersprungen, auch bei anderem Dateinamen. Bitte warte nach dem Absenden auf den Importbericht." }
                            button { class: "primary", r#type: "submit", "Alle Bücher im Ordner importieren" }
                        }
                    }
                    section { id: "upload", class: "upload",
                        div { class: "eyebrow", "DEIN REGAL WÄCHST" } h2 { "Eine neue Geschichte hinzufügen" }
                        p { "EPUB, PDF, MOBI oder AZW3 · maximal 50 MiB pro Buch" }
                        form { method: "post", action: "/books", enctype: "multipart/form-data",
                            label { "E-Book-Datei", input { r#type: "file", name: "file", accept: ".epub,.pdf,.mobi,.azw3", required: true } }
                            div { class: "fields",
                                label { "Titel", input { name: "title", placeholder: "Optional: wird beim Upload automatisch erkannt", maxlength: "300" } }
                                label { "Autor", input { name: "author", placeholder: "Optional: wird beim Upload automatisch erkannt", maxlength: "200" } }
                            }
                            div { class: "fields",
                                label { "Hauptkategorie", select { name: "category",
                                    option { value: "", "Noch nicht eingeordnet" }
                                    for category in classification::CATEGORIES { option { value: category, "{category}" } }
                                } }
                                label { "Schlagwörter", input { name: "tags", maxlength: "1300", placeholder: "z. B. weltraum, abenteuer" } }
                            }
                            p { class: "field-hint", "Schlagwörter mit Komma trennen. Themen aus EPUB-Metadaten erscheinen nach dem Upload als Vorschläge." }
                            button { class: "primary", r#type: "submit", "+ Zur Bibliothek hinzufügen" }
                        }
                    }
                }
                footer { "leaves · Gute Geschichten bleiben. Deine Dateien auch." }
            }
    });
    Ok(Html(format!("<!DOCTYPE html><html lang=\"de\">{content}</html>")))
}
fn author_url(author: &str) -> String { filter_url("author", author) }

fn filter_url(field: &str, value: &str) -> String {
    let encoded: String = value.as_bytes().iter().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (*b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect();
    format!("/?{field}={encoded}")
}

fn status_label(s: &str) -> &str { match s { "reading" => "Lese ich gerade", "read" => "Gelesen", _ => "Ungelesen" } }

async fn upload(State(s): State<AppState>, mut multipart: Multipart) -> Result<Redirect, AppError> {
    let (mut title, mut author) = (String::new(), String::new());
    let (mut category, mut tag_text) = (String::new(), String::new());
    let mut file = None;
    while let Some(field) = multipart.next_field().await.map_err(|_| bad("Upload ungültig oder zu groß"))? {
        let name = field.name().unwrap_or("").to_owned();
        if name == "file" {
            if file.is_some() { return Err(bad("Bitte nur eine Datei hochladen")); }
            let filename = field.file_name().unwrap_or("").replace('\\', "/");
            let filename = filename.rsplit('/').next().unwrap_or("").to_string();
            let format = filename.rsplit('.').next().unwrap_or("").to_lowercase();
            if !["epub", "pdf", "mobi", "azw3"].contains(&format.as_str()) { return Err(bad("Nicht unterstütztes Dateiformat")); }
            let bytes = field.bytes().await.map_err(|_| bad("Datei zu groß oder unvollständig"))?;
            if bytes.is_empty() || bytes.len() > 50 * 1024 * 1024 { return Err(bad("Dateigröße muss zwischen 1 Byte und 50 MiB liegen")); }
            file = Some((filename, format, bytes));
        } else if ["title", "author", "category", "tags"].contains(&name.as_str()) {
            let value = field.text().await.map_err(|_| bad("Ungültiges Textfeld"))?;
            if value.chars().count() > if name == "tags" { 1300 } else { 300 } { return Err(bad("Textfeld zu lang")); }
            match name.as_str() { "title" => title = value, "author" => author = value, "category" => category = value, "tags" => tag_text = value, _ => () }
        }
    }
    let (filename, format, bytes) = file.ok_or_else(|| bad("Bitte eine Datei auswählen"))?;
    classification::validate_category(&category)?;
    let tags = classification::tags(&tag_text)?;
    let imported = store_book(&s, filename, format, bytes.to_vec(), title, author, category, tags).await?;
    Ok(Redirect::to(if imported { "/" } else { "/?notice=duplicate" }))
}

async fn store_book(s: &AppState, filename: String, format: String, bytes: Vec<u8>, mut title: String, mut author: String, category: String, tags: Vec<String>) -> Result<bool, AppError> {
    let hash = dedup::hash(&bytes);
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM books WHERE content_hash=?)").bind(&hash).fetch_one(&s.db).await?;
    if exists { return Ok(false); }
    let (suggested_category, suggested_tags) = {
        let input = bytes.clone();
        let kind = format.clone();
        let detected = tokio::task::spawn_blocking(move || metadata::extract(&kind, &input)).await.unwrap_or_default();
        if title.trim().is_empty() { title = detected.title; }
        if author.trim().is_empty() { author = detected.author; }
        let (suggested_category, suggested_tags) = classification::suggest(&detected.subjects);
        (if category.is_empty() { suggested_category } else { String::new() }, if tags.is_empty() { suggested_tags } else { String::new() })
    };
    if title.trim().is_empty() { title = filename.rsplit_once('.').map(|v| v.0).unwrap_or(&filename).chars().take(300).collect(); }
    validate(&title, &author, "unread")?;
    let id = uuid::Uuid::new_v4().to_string();
    let path = s.files.join(&id);
    tokio::fs::write(&path, &bytes).await?;
    let result: Result<bool, sqlx::Error> = async {
        let mut tx = s.db.begin().await?;
        let inserted = sqlx::query("INSERT INTO books(id,title,author,format,size,category,suggested_category,suggested_tags,content_hash) SELECT ?,?,?,?,?,?,?,?,? WHERE NOT EXISTS(SELECT 1 FROM books WHERE content_hash=?) ON CONFLICT DO NOTHING")
            .bind(&id).bind(title.trim()).bind(author.trim()).bind(&format).bind(bytes.len() as i64)
            .bind(category).bind(suggested_category).bind(suggested_tags).bind(&hash).bind(&hash).execute(&mut *tx).await?;
        if inserted.rows_affected() == 0 { return Ok(false); }
        classification::replace_tags(&mut tx, &id, &tags).await?;
        tx.commit().await?;
        Ok(true)
    }.await;
    match result {
        Ok(true) => Ok(true),
        Ok(false) => { tokio::fs::remove_file(path).await?; Ok(false) }
        Err(e) => { let _ = tokio::fs::remove_file(path).await; Err(e.into()) }
    }
}
// Read one file at a time and discard oversized files without retaining their contents.
async fn batch_upload(State(s): State<AppState>, mut multipart: Multipart) -> Result<Html<String>, AppError> {
    let (mut imported, mut skipped, mut failed, mut duplicates) = (0usize, 0usize, 0usize, 0usize);
    let mut errors = Vec::new();
    let mut incomplete = false;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => { incomplete = true; break; }
        };
        let mut field = field;
        if field.name() != Some("files") { continue; }
        let filename = field.file_name().unwrap_or("").replace('\\', "/");
        let filename = filename.rsplit('/').next().unwrap_or("").to_string();
        let format = filename.rsplit('.').next().unwrap_or("").to_lowercase();
        if !["epub", "pdf", "mobi", "azw3"].contains(&format.as_str()) { skipped += 1; continue; }
        let mut bytes = Vec::new();
        let mut oversized = false;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    if !oversized && bytes.len() + chunk.len() <= 50 * 1024 * 1024 { bytes.extend_from_slice(&chunk); }
                    else { oversized = true; bytes.clear(); }
                }
                Ok(None) => break,
                Err(_) => { incomplete = true; break; }
            }
        }
        if incomplete { failed += 1; errors.push(format!("{filename}: Übertragung unvollständig")); break; }
        let result = if oversized || bytes.is_empty() {
            Err(bad("Dateigröße muss zwischen 1 Byte und 50 MiB liegen"))
        } else {
            store_book(&s, filename.clone(), format, bytes, String::new(), String::new(), String::new(), Vec::new()).await
        };
        match result {
            Ok(true) => imported += 1,
            Ok(false) => { skipped += 1; duplicates += 1; }
            Err(error) => { failed += 1; errors.push(format!("{filename}: {}", error.1)); }
        }
    }
    let content = dioxus_ssr::render_element(rsx! {
        head { meta { charset: "utf-8" } meta { name: "viewport", content: "width=device-width, initial-scale=1" } title { "Leaves · Importbericht" } link { rel: "stylesheet", href: "/style.css" } }
        body { main { section { class: "upload",
            h1 { "Importbericht" }
            p { role: "status", "{imported} importiert · {skipped} übersprungen · {failed} fehlgeschlagen" }
            p { "Davon {duplicates} bereits vorhanden." }
            if incomplete { p { "Die Übertragung wurde abgebrochen oder das Gesamtlimit von 1 GiB überschritten. Bereits importierte Bücher bleiben erhalten; weitere Dateien wurden nicht verarbeitet." } }
            if imported + skipped + failed == 0 && !incomplete { p { "Keine Dateien empfangen. Bitte wähle einen Ordner mit Büchern." } }
            ul { for error in &errors { li { "{error}" } } }
            a { class: "button primary", href: "/", "Zur Bibliothek" }
        } } }
    });
    Ok(Html(format!("<!DOCTYPE html><html lang=\"de\">{content}</html>")))
}

#[derive(Deserialize)]
struct Edit { title: String, author: String, status: String, category: Option<String>, tags: Option<String> }
fn validate(title: &str, author: &str, status: &str) -> Result<(), AppError> {
    if title.trim().is_empty() || title.chars().count() > 300 || author.chars().count() > 200 { return Err(bad("Titel erforderlich (max. 300 Zeichen); Autor max. 200 Zeichen")); }
    if !["unread", "reading", "read"].contains(&status) { return Err(bad("Ungültiger Lesestatus")); }
    Ok(())
}
async fn update(State(s): State<AppState>, Path(id): Path<String>, Form(edit): Form<Edit>) -> Result<Redirect, AppError> {
    validate(&edit.title, &edit.author, &edit.status)?;
    if let Some(category) = &edit.category { classification::validate_category(category)?; }
    let tags = edit.tags.as_deref().map(classification::tags).transpose()?;
    let classified = edit.category.is_some() || edit.tags.is_some();
    let mut tx = s.db.begin().await?;
    let result = sqlx::query("UPDATE books SET title=?,author=?,status=?,category=coalesce(?,category),suggested_category=CASE WHEN ? THEN '' ELSE suggested_category END,suggested_tags=CASE WHEN ? THEN '' ELSE suggested_tags END WHERE id=?")
        .bind(edit.title.trim()).bind(edit.author.trim()).bind(edit.status).bind(edit.category).bind(classified).bind(classified).bind(&id).execute(&mut *tx).await?;
    if result.rows_affected() == 0 { return Err(AppError(StatusCode::NOT_FOUND, "Buch nicht gefunden".into())); }
    if let Some(tags) = tags { classification::replace_tags(&mut tx, &id, &tags).await?; }
    tx.commit().await?;
    Ok(Redirect::to("/"))
}

async fn suggest_classification(State(s): State<AppState>, Path(id): Path<String>) -> Result<Redirect, AppError> {
    let row = sqlx::query("SELECT id,format FROM books WHERE id=?").bind(&id).fetch_optional(&s.db).await?.ok_or_else(|| AppError(StatusCode::NOT_FOUND, "Buch nicht gefunden".into()))?;
    let stored_id: String = row.get("id");
    let format: String = row.get("format");
    let bytes = tokio::fs::read(s.files.join(stored_id)).await?;
    let detected = tokio::task::spawn_blocking(move || metadata::extract(&format, &bytes)).await.unwrap_or_default();
    let (category, tags) = classification::suggest(&detected.subjects);
    let empty = category.is_empty() && tags.is_empty();
    sqlx::query("UPDATE books SET suggested_category=?,suggested_tags=? WHERE id=?").bind(category).bind(tags).bind(id).execute(&s.db).await?;
    Ok(Redirect::to(if empty { "/?notice=no-subjects" } else { "/" }))
}

async fn accept_classification(State(s): State<AppState>, Path(id): Path<String>) -> Result<Redirect, AppError> {
    let mut tx = s.db.begin().await?;
    let row = sqlx::query("SELECT category,suggested_category,suggested_tags FROM books WHERE id=?").bind(&id).fetch_optional(&mut *tx).await?.ok_or_else(|| AppError(StatusCode::NOT_FOUND, "Buch nicht gefunden".into()))?;
    let existing: Vec<String> = sqlx::query_scalar("SELECT name FROM book_tags WHERE book_id=?").bind(&id).fetch_all(&mut *tx).await?;
    let suggested_tags: String = row.get("suggested_tags");
    let mut merged: std::collections::BTreeSet<String> = existing.into_iter().collect();
    for tag in classification::tags(&suggested_tags)? { if merged.len() < 20 { merged.insert(tag); } }
    let merged: Vec<_> = merged.into_iter().collect();
    classification::replace_tags(&mut tx, &id, &merged).await?;
    sqlx::query("UPDATE books SET category=CASE WHEN category='' THEN suggested_category ELSE category END,suggested_category='',suggested_tags='' WHERE id=?").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Redirect::to("/"))
}

async fn dismiss_classification(State(s): State<AppState>, Path(id): Path<String>) -> Result<Redirect, AppError> {
    let result = sqlx::query("UPDATE books SET suggested_category='',suggested_tags='' WHERE id=?").bind(id).execute(&s.db).await?;
    if result.rows_affected() == 0 { return Err(AppError(StatusCode::NOT_FOUND, "Buch nicht gefunden".into())); }
    Ok(Redirect::to("/"))
}
async fn download(State(s): State<AppState>, Path(id): Path<String>) -> Result<Response, AppError> {
    let row = sqlx::query("SELECT id,format FROM books WHERE id=?").bind(id).fetch_optional(&s.db).await?.ok_or_else(|| AppError(StatusCode::NOT_FOUND, "Buch nicht gefunden".into()))?;
    let id: String = row.get("id");
    let format: String = row.get("format");
    let bytes = tokio::fs::read(s.files.join(&id)).await?;
    Ok(([(header::CONTENT_TYPE, "application/octet-stream".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{id}.{format}\"")), (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string())], bytes).into_response())
}
async fn delete(State(s): State<AppState>, Path(id): Path<String>) -> Result<Redirect, AppError> {
    let row = sqlx::query("SELECT id FROM books WHERE id=?").bind(&id).fetch_optional(&s.db).await?.ok_or_else(|| AppError(StatusCode::NOT_FOUND, "Buch nicht gefunden".into()))?;
    let stored_id: String = row.get("id");
    // Delete the file first: a filesystem failure must not silently orphan it.
    match tokio::fs::remove_file(s.files.join(stored_id)).await {
        Ok(()) => (), Err(e) if e.kind() == std::io::ErrorKind::NotFound => (), Err(e) => return Err(e.into()),
    }
    sqlx::query("DELETE FROM books WHERE id=?").bind(id).execute(&s.db).await?;
    Ok(Redirect::to("/"))
}
async fn health(State(s): State<AppState>) -> Result<&'static str, AppError> { sqlx::query("SELECT 1").execute(&s.db).await?; Ok("ok") }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_metadata() {
        assert!(validate("Buch", "Autor", "reading").is_ok());
        assert!(validate("  ", "", "read").is_err());
        assert!(validate("Buch", "", "invalid").is_err());
        assert!(validate(&"a".repeat(301), "", "unread").is_err());
    }
}
