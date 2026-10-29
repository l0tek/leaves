use crate::{AppError, bad};
use sqlx::{Row, SqlitePool};
use std::collections::BTreeSet;

pub const CATEGORIES: [&str; 4] = ["Belletristik", "Sachbuch", "Fachbuch", "Kinderbuch"];

pub async fn migrate(db: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    let columns = sqlx::query("PRAGMA table_info(books)").fetch_all(&mut *tx).await?;
    for column in ["category", "suggested_category", "suggested_tags"] {
        if !columns.iter().any(|r| r.get::<String, _>("name") == column) {
            sqlx::query(&format!("ALTER TABLE books ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"))
                .execute(&mut *tx).await?;
        }
    }
    sqlx::query("CREATE TABLE IF NOT EXISTS book_tags (book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE, name TEXT NOT NULL, PRIMARY KEY(book_id,name))").execute(&mut *tx).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS book_tags_name ON book_tags(name)").execute(&mut *tx).await?;
    tx.commit().await
}

pub fn tags(text: &str) -> Result<Vec<String>, AppError> {
    let tags: BTreeSet<String> = text.split([',', ';']).map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase())
        .filter(|t| !t.is_empty()).collect();
    if tags.len() > 20 || tags.iter().any(|t| t.chars().count() > 60 || t.chars().any(char::is_control)) {
        return Err(bad("Maximal 20 Schlagwörter mit jeweils 60 Zeichen erlaubt"));
    }
    Ok(tags.into_iter().collect())
}

pub fn validate_category(category: &str) -> Result<(), AppError> {
    if category.is_empty() || CATEGORIES.contains(&category) { Ok(()) }
    else { Err(bad("Ungültige Hauptkategorie")) }
}

pub fn suggest(subjects: &[String]) -> (String, String) {
    let mut values = BTreeSet::new();
    for subject in subjects.iter().take(100) {
        for tag in subject.split([',', ';']) {
            let tag: String = tag.split_whitespace().collect::<Vec<_>>().join(" ").chars().filter(|c| !c.is_control()).take(60).collect::<String>().to_lowercase();
            if !tag.is_empty() { values.insert(tag); }
        }
    }
    let tags: Vec<_> = values.into_iter().take(20).collect();
    let has = |terms: &[&str]| tags.iter().any(|tag| terms.iter().any(|term| tag.contains(term)));
    let category = if has(&["children", "juvenile", "kinderbuch", "jugendbuch"]) { "Kinderbuch" }
        else if has(&["programming", "programmierung", "informatik", "computer", "engineering", "medizin", "mathematik"]) { "Fachbuch" }
        else if has(&["nonfiction", "non-fiction", "non fiction", "sachbuch", "history", "geschichte", "biography", "biografie", "ratgeber"]) { "Sachbuch" }
        else if has(&["fiction", "belletristik", "roman", "fantasy", "krimi", "thriller"]) { "Belletristik" }
        else { "" };
    (category.to_string(), tags.join(", "))
}

pub async fn replace_tags(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, id: &str, tags: &[String]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM book_tags WHERE book_id=?").bind(id).execute(&mut **tx).await?;
    for tag in tags {
        sqlx::query("INSERT INTO book_tags(book_id,name) VALUES(?,?)").bind(id).bind(tag).execute(&mut **tx).await?;
    }
    Ok(())
}
