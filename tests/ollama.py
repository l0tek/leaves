"""Isolated Ollama integration test using a local mock (no real model required)."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import re
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

proposal = {"title": "Neuer Titel", "author": "Neue Autorin", "category": "Sachbuch", "tags": ["Geschichte"]}
requests = []
mode = "valid"

class Mock(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        assert self.path == "/api/chat"
        requests.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
        self.send_response(503 if mode == "unavailable" else 200)
        self.end_headers()
        content = json.dumps(proposal if mode != "invalid" else {**proposal, "category": "Falsch"})
        self.wfile.write(json.dumps({"message": {"content": content if mode != "malformed" else "not json"}}).encode())

server = ThreadingHTTPServer(("127.0.0.1", 0), Mock)
threading.Thread(target=server.serve_forever, daemon=True).start()
with tempfile.TemporaryDirectory(prefix="leaves-ollama-") as directory:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    base = f"http://127.0.0.1:{port}"
    binary = Path(os.environ.get("LEAVES_BINARY", "target/debug/leaves")).resolve()
    process = subprocess.Popen([str(binary)], env={**os.environ, "DATA_DIR": directory, "LISTEN_ADDR": f"127.0.0.1:{port}", "OLLAMA_URL": f"http://127.0.0.1:{server.server_port}", "OLLAMA_MODEL": "test-model"}, stdout=subprocess.DEVNULL)

    def call(path, data=None, content_type=None):
        with urlopen(Request(base + path, data=data, headers={"Content-Type": content_type} if content_type else {}), timeout=20) as response:
            return response.read().decode()

    def stored():
        with sqlite3.connect(Path(directory) / "library.sqlite") as db:
            return db.execute("SELECT title,author,category FROM books WHERE id=?", (book_id,)).fetchone()

    try:
        for _ in range(100):
            try:
                call("/health")
                break
            except OSError:
                time.sleep(0.05)
        output = io.BytesIO()
        with zipfile.ZipFile(output, "w") as archive:
            archive.writestr("META-INF/container.xml", '<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>')
            archive.writestr("book.opf", '<package xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>Alter Titel</dc:title><dc:creator>Alter Autor</dc:creator></metadata></package>')
            archive.writestr("page.xhtml", '<html><body><p>Ein historischer Testauszug. Dieses Buch beschreibt die Geschichte einer Stadt und ihrer Bewohner über viele Jahre mit historischen Quellen und ausführlichen Berichten.</p></body></html>')
        body = b'--test\r\nContent-Disposition: form-data; name="file"; filename="test.epub"\r\n\r\n' + output.getvalue() + b'\r\n--test--\r\n'
        page = call("/books", body, "multipart/form-data; boundary=test")
        book_id = re.search(r'href="/books/([a-f0-9-]+)/download"', page)[1]
        assert "Metadaten mit Ollama ergänzen" in page
        before = stored()
        preview = call(f"/books/{book_id}/ollama", b"")
        assert 'value="Alter Titel"' in preview and 'value="Alter Autor"' in preview and "Übernehmen" in preview
        assert 'value="Neuer Titel"' not in preview and 'value="Neue Autorin"' not in preview
        assert "Verwendete Quellen prüfen" in preview
        assert stored() == before, "Preview modified the book"
        request = requests[-1]
        assert request["model"] == "test-model" and request["stream"] is False
        assert set(request["format"]["properties"]) == {"category", "tags"}
        context = json.loads(request["messages"][1]["content"])
        assert "historischer Testauszug" in context["excerpt"] and context["file_author"] == "Alter Autor"
        for mode in ("invalid", "malformed", "unavailable"):
            try:
                call(f"/books/{book_id}/ollama", b"")
                raise AssertionError("Expected a model error")
            except HTTPError as error:
                assert error.code == 502
            assert stored() == before
        mode = "valid"
        proposal = {"title": "", "author": "", "category": "", "tags": []}
        fallback = call(f"/books/{book_id}/ollama", b"")
        assert 'value="Alter Titel"' in fallback and 'value="Alter Autor"' in fallback
        apply = urlencode({"title": "Neuer Titel", "author": "Neue Autorin", "category": "Sachbuch", "tags": "geschichte", "status": "unread"}).encode()
        call(f"/books/{book_id}", apply, "application/x-www-form-urlencoded")
        assert stored() == ("Neuer Titel", "Neue Autorin", "Sachbuch")
        assert "geschichte" in call("/")
        # Known title + author may be classified, explicitly marked as provisional.
        with sqlite3.connect(Path(directory) / "library.sqlite") as db:
            db.execute("UPDATE books SET title=?,author=? WHERE id=?", ("Der heimliche Rebell", "Dick, Philip K", book_id))
        proposal = {"category": "Belletristik", "tags": ["Science-Fiction", "Gesellschaft"]}
        # Clear the excerpt by using an unreadable PDF for this isolated fixture.
        with sqlite3.connect(Path(directory) / "library.sqlite") as db:
            db.execute("UPDATE books SET format='pdf' WHERE id=?", (book_id,))
        preview = call(f"/books/{book_id}/ollama", b"")
        assert "Vorläufige Einordnung" in preview
        assert 'value="Der heimliche Rebell"' in preview and 'value="Dick, Philip K"' in preview
        assert 'value="Belletristik" selected=true' in preview and "science-fiction" in preview
        # A scanned/empty PDF without an author still cannot be identified.
        body = b'--empty\r\nContent-Disposition: form-data; name="file"; filename="scan.pdf"\r\n\r\nno readable text\r\n--empty--\r\n'
        page = call("/books", body, "multipart/form-data; boundary=empty")
        with sqlite3.connect(Path(directory) / "library.sqlite") as db:
            scan_id = db.execute("SELECT id FROM books WHERE title='scan'").fetchone()[0]
        count = len(requests)
        assert "Zu wenig auswertbarer Buchinhalt" in call(f"/books/{scan_id}/ollama", b"")
        assert len(requests) == count, "Model called without evidence"
        print("OK: Ollama context/excerpt, schema request, preview without writes, acceptance, empty values, invalid JSON/category, upstream errors")
    finally:
        process.terminate()
        process.wait(timeout=10)
        server.shutdown()
