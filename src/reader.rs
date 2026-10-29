use super::*;

pub async fn page(State(s): State<AppState>, Path(id): Path<String>) -> Result<Html<String>, AppError> {
    let row = sqlx::query("SELECT id,title,author,format FROM books WHERE id=?")
        .bind(&id).fetch_optional(&s.db).await?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "Buch nicht gefunden".into()))?;
    let id: String = row.get("id");
    let title: String = row.get("title");
    let author: String = row.get("author");
    let format: String = row.get("format");
    if format != "epub" {
        return Err(AppError(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Der Reader unterstützt EPUB. Andere Formate bitte herunterladen.".into()));
    }
    let content = dioxus_ssr::render_element(rsx! {
        head {
            meta { charset: "utf-8" }
            meta { name: "viewport", content: "width=device-width, initial-scale=1" }
            title { "{title} · Leaves Reader" }
            link { rel: "stylesheet", href: "/style.css" }
            script { src: "/vendor/jszip.min.js", defer: true }
            script { src: "/vendor/epub.min.js", defer: true }
            script { src: "/reader.js", defer: true }
        }
        body { class: "reader-page",
            header { class: "reader-header",
                a { href: "/", "← Bibliothek" }
                div { h1 { "{title}" } if !author.is_empty() { a { href: author_url(&author), "{author}" } } }
                a { href: "/books/{id}/download", "Herunterladen" }
            }
            main { class: "reader-main", id: "reader", "data-book-id": "{id}", "data-book-url": "/books/{id}/download",
                nav { class: "reader-controls", "aria-label": "Lesesteuerung",
                    button { id: "previous", disabled: true, "← Zurück" }
                    label { "Kapitel", select { id: "chapters", disabled: true, option { value: "", "Kapitel auswählen" } } }
                    label { "Schriftgröße", select { id: "font-size",
                        option { value: "90", "Klein" }
                        option { value: "110", selected: true, "Normal" }
                        option { value: "130", "Groß" }
                        option { value: "150", "Sehr groß" }
                    } }
                    button { id: "next", disabled: true, "Weiter →" }
                }
                p { id: "reader-status", role: "status", "Buch wird geladen …" }
                noscript { p { "Zum Lesen von EPUBs bitte JavaScript aktivieren. Du kannst das Buch auch herunterladen." } }
                div { id: "viewer", "aria-label": "Buchinhalt" }
            }
        }
    });
    Ok(Html(format!("<!DOCTYPE html><html lang=\"de\">{content}</html>")))
}

pub async fn script() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], include_str!("reader.js"))
}
pub async fn epub_script() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], include_str!("vendor/epub.min.js"))
}
pub async fn zip_script() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], include_str!("vendor/jszip.min.js"))
}
