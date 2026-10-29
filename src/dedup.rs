use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::path::Path;
use tokio::io::AsyncReadExt;

pub fn hash(bytes: &[u8]) -> String { hex::encode(Sha256::digest(bytes)) }

// Preserve existing copies while reserving each hash once for future imports.
pub async fn migrate(db: &SqlitePool, files: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = db.begin().await?;
    let columns = sqlx::query("PRAGMA table_info(books)").fetch_all(&mut *tx).await?;
    if !columns.iter().any(|r| r.get::<String, _>("name") == "content_hash") {
        sqlx::query("ALTER TABLE books ADD COLUMN content_hash TEXT").execute(&mut *tx).await?;
        sqlx::query("ALTER TABLE books ADD COLUMN legacy_duplicate INTEGER NOT NULL DEFAULT 0").execute(&mut *tx).await?;
    }
    sqlx::query("CREATE INDEX IF NOT EXISTS books_content_hash ON books(content_hash)").execute(&mut *tx).await?;
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM books WHERE content_hash IS NULL ORDER BY created_at,id").fetch_all(&mut *tx).await?;
    for id in ids {
        let mut file = tokio::fs::File::open(files.join(&id)).await?;
        let mut digest = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).await?;
            if count == 0 { break; }
            digest.update(&buffer[..count]);
        }
        let hash = hex::encode(digest.finalize());
        sqlx::query("UPDATE books SET content_hash=?, legacy_duplicate=EXISTS(SELECT 1 FROM books WHERE content_hash=?) WHERE id=?")
            .bind(&hash).bind(&hash).bind(&id).execute(&mut *tx).await?;
    }
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS books_unique_content_hash ON books(content_hash) WHERE legacy_duplicate=0").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
