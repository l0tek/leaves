"""Isolated deduplication and legacy migration test. Build with cargo build first."""
import concurrent.futures
import hashlib
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import tempfile
import time
import uuid
from urllib.request import Request, urlopen

binary = Path(os.environ.get("LEAVES_BINARY", "target/debug/leaves")).resolve()
with tempfile.TemporaryDirectory(prefix="leaves-dedup-") as directory:
    root = Path(directory)
    (root / "books").mkdir()
    original = b"legacy PDF bytes"
    with sqlite3.connect(root / "library.sqlite") as db:
        db.execute("CREATE TABLE books (id TEXT PRIMARY KEY, title TEXT NOT NULL, author TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'unread', format TEXT NOT NULL, size INTEGER NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)")
        for book_id in ("legacy-a", "legacy-b"):
            (root / "books" / book_id).write_bytes(original)
            db.execute("INSERT INTO books(id,title,format,size) VALUES(?,?,?,?)", (book_id, book_id, "pdf", len(original)))
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    base = f"http://127.0.0.1:{port}"

    def call(path, data=None, content_type=None):
        headers = {"Content-Type": content_type} if content_type else {}
        with urlopen(Request(base + path, data=data, headers=headers), timeout=30) as response:
            return response.read().decode()

    def upload(data, filename="renamed.pdf", batch=False):
        boundary = uuid.uuid4().hex
        name = "files" if batch else "file"
        body = f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"; filename="{filename}"\r\n\r\n'.encode() + data + f"\r\n--{boundary}--\r\n".encode()
        return call("/books/import" if batch else "/books", body, f"multipart/form-data; boundary={boundary}")

    def start():
        process = subprocess.Popen([str(binary)], env={**os.environ, "DATA_DIR": directory, "LISTEN_ADDR": f"127.0.0.1:{port}"}, stdout=subprocess.DEVNULL)
        for _ in range(100):
            try:
                call("/health")
                return process
            except OSError:
                if process.poll() is not None:
                    raise AssertionError("Service failed to start")
                time.sleep(0.05)
        process.terminate()
        process.wait()
        raise AssertionError("Service did not become ready")

    process = start()
    try:
        with sqlite3.connect(root / "library.sqlite") as db:
            hashes = db.execute("SELECT content_hash FROM books").fetchall()
        assert hashes == [(hashlib.sha256(original).hexdigest(),)] * 2
        assert "bereits in deiner Bibliothek" in upload(original)
        assert "0 importiert · 1 übersprungen · 0 fehlgeschlagen" in upload(original, batch=True)
        call("/books/legacy-a/delete", b"")
        assert "bereits in deiner Bibliothek" in upload(original)
        new = b"unique concurrent test " + uuid.uuid4().bytes
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            list(pool.map(lambda i: upload(new, f"copy-{i}.pdf"), range(8)))
        assert "0 importiert · 1 übersprungen · 0 fehlgeschlagen" in upload(new, batch=True)
        # Same title/filename with different bytes must remain a distinct book.
        upload(new + b"changed", "copy-0.pdf")
        with sqlite3.connect(root / "library.sqlite") as db:
            assert db.execute("SELECT count(*) FROM books").fetchone()[0] == 3
            assert db.execute("SELECT count(*) FROM books WHERE content_hash=?", (hashlib.sha256(new).hexdigest(),)).fetchone()[0] == 1
        assert len(list((root / "books").iterdir())) == 3, "Orphan files after duplicate uploads"
        call("/books/legacy-b/delete", b"")
        upload(original)
        assert "bereits in deiner Bibliothek" in upload(original)
        process.terminate()
        process.wait(timeout=10)
        process = start()
        assert "bereits in deiner Bibliothek" in upload(original)
        print("OK: legacy backfill, retained copies, renamed files, batch duplicates, concurrent uploads, cleanup, changed bytes, deletion, restart")
    finally:
        process.terminate()
        process.wait(timeout=10)
