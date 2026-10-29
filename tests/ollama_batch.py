"""Isolated batch test: background progress, partial fills, failures, edits and cancel."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import tempfile
import threading
import time
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen
import zipfile

entered = threading.Event()
release = threading.Event()
calls = []
block_title = "A In Arbeit"

class Mock(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        context = json.loads(payload["messages"][1]["content"])
        title = context["title"]
        calls.append(title)
        if title == block_title:
            entered.set()
            assert release.wait(10), "test did not release request"
        self.send_response(503 if title == "F Fehler" else 200)
        self.end_headers()
        self.wfile.write(json.dumps({"message": {"content": json.dumps({"category": "Belletristik", "tags": ["Science-Fiction", "Gesellschaft"]})}}).encode())

server = ThreadingHTTPServer(("127.0.0.1", 0), Mock)
threading.Thread(target=server.serve_forever, daemon=True).start()
with tempfile.TemporaryDirectory(prefix="leaves-batch-ollama-") as directory:
    root = Path(directory)
    (root / "books").mkdir()
    database = root / "library.sqlite"
    with sqlite3.connect(database) as db:
        db.execute("CREATE TABLE books (id TEXT PRIMARY KEY, title TEXT NOT NULL, author TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'unread', format TEXT NOT NULL, size INTEGER NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    base = f"http://127.0.0.1:{port}"
    binary = Path(os.environ.get("LEAVES_BINARY", "target/debug/leaves")).resolve()
    process = subprocess.Popen([str(binary)], env={**os.environ, "DATA_DIR": directory, "LISTEN_ADDR": f"127.0.0.1:{port}", "OLLAMA_URL": f"http://127.0.0.1:{server.server_port}"}, stdout=subprocess.DEVNULL)

    def call(path, data=None, content_type=None):
        with urlopen(Request(base + path, data=data, headers={"Content-Type": content_type} if content_type else {}), timeout=10) as response:
            assert response.headers.get("Cache-Control") == "no-store" if path.startswith("/ollama/batch") else True
            return response.read().decode()

    def insert(book_id, title, category="", tags=(), author="Originalautor", data=None, format="pdf"):
        data = data if data is not None else f"unreadable PDF {book_id}".encode()
        (root / "books" / book_id).write_bytes(data)
        with sqlite3.connect(database) as db:
            db.execute("INSERT INTO books(id,title,author,format,size,category) VALUES(?,?,?,?,?,?)", (book_id, title, author, format, len(data), category))
            db.executemany("INSERT INTO book_tags(book_id,name) VALUES(?,?)", [(book_id, tag) for tag in tags])

    def finish():
        for _ in range(150):
            page = call("/ollama/batch")
            if 'http-equiv="refresh"' not in page:
                return page
            time.sleep(0.02)
        raise AssertionError("Batch did not finish")

    try:
        for _ in range(100):
            try:
                call("/health")
                break
            except OSError:
                time.sleep(0.05)
        assert "noch kein Sammellauf" in call("/ollama/batch")
        assert "Sammellauf abgeschlossen" in call("/ollama/batch", b"")
        insert("a", "A In Arbeit")
        insert("b", "B Fehlend")
        insert("c", "C Kategorie", "Sachbuch")
        insert("d", "D Schlagwort", tags=["eigenes"])
        insert("e", "E Vollständig", "Fachbuch", ["manuell"])
        insert("f", "F Fehler")
        insert("g", "G Keine Grundlage", author="")
        output = io.BytesIO()
        with zipfile.ZipFile(output, "w") as archive:
            archive.writestr("META-INF/container.xml", '<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>')
            archive.writestr("book.opf", '<package xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:creator>Dateiautor</dc:creator></metadata></package>')
        insert("h", "H Autor fehlt", "Kinderbuch", ["vorhanden"], author="", data=output.getvalue(), format="epub")
        start = time.monotonic()
        page = call("/ollama/batch", b"")
        assert time.monotonic() - start < 3, "Start blocked on inference"
        assert entered.wait(3)
        assert "Sammellauf läuft" in page and 'http-equiv="refresh"' in page
        assert 'value="0" max="7"' in page and "Aktuelles Buch: A In Arbeit" in call("/ollama/batch")
        # Double-click must open existing progress, not launch a second worker.
        call("/ollama/batch", b"")
        try:
            call("/books/b/ollama", b"")
            raise AssertionError("Single request was allowed during batch")
        except HTTPError as error:
            assert error.code == 503
        # User completes the current book while the model is generating.
        call("/books/a", urlencode({"title": "A Manuell", "author": "Manueller Autor", "status": "reading", "category": "Kinderbuch", "tags": "manuell"}).encode(), "application/x-www-form-urlencoded")
        release.set()
        page = finish()
        assert "7 von 7 Büchern verarbeitet · 100 %" in page, page
        assert "4 ergänzt · 2 unverändert · 1 fehlgeschlagen" in page, page
        assert "vorläufig" in page and "F Fehler" in page
        with sqlite3.connect(database) as db:
            rows = {row[0]: row[1:] for row in db.execute("SELECT id,title,author,status,category FROM books")}
            tags = {book_id: [r[0] for r in db.execute("SELECT name FROM book_tags WHERE book_id=? ORDER BY name", (book_id,))] for book_id in rows}
        assert rows["a"] == ("A Manuell", "Manueller Autor", "reading", "Kinderbuch")
        assert tags["a"] == ["manuell"]
        assert rows["b"][-1] == "Belletristik" and tags["b"] == ["gesellschaft", "science-fiction"]
        assert rows["c"][-1] == "Sachbuch" and tags["c"] == ["gesellschaft", "science-fiction"]
        assert rows["d"][-1] == "Belletristik" and tags["d"] == ["eigenes"]
        assert rows["e"][-1] == "Fachbuch" and tags["e"] == ["manuell"]
        assert rows["h"][1] == "Dateiautor" and tags["h"] == ["vorhanden"]
        assert calls == ["A In Arbeit", "B Fehlend", "C Kategorie", "D Schlagwort", "F Fehler"], calls
        # A second run sees only the still-incomplete books; cancellation stops at a boundary.
        insert("0", "Stopptest")
        block_title = "Stopptest"
        entered.clear()
        release.clear()
        call("/ollama/batch", b"")
        assert entered.wait(3)
        page = call("/ollama/batch/cancel", b"")
        assert "Abbruch angefordert" in page
        release.set()
        page = finish()
        assert "Sammellauf abgebrochen" in page and "1 von 3 Büchern verarbeitet" in page, page
        assert calls[-1] == "Stopptest" and calls.count("F Fehler") == 1
        print("OK: automatic batch, live progress, double start, manual edits, missing-only fields, author recovery, errors, no evidence, restart selection, cancellation")
    finally:
        release.set()
        process.terminate()
        process.wait(timeout=10)
        server.shutdown()
