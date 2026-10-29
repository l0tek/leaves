"""Isolated reader routes and exact author filtering. Requires cargo build."""
import io
import os
from pathlib import Path
import re
import json
import socket
import subprocess
import tempfile
import time
from urllib.error import HTTPError
from urllib.parse import quote, urlencode
from urllib.request import Request, urlopen
import uuid
import zipfile

binary = Path(os.environ.get("LEAVES_BINARY", "target/debug/leaves")).resolve()


def epub():
    data = io.BytesIO()
    with zipfile.ZipFile(data, "w") as archive:
        archive.writestr("mimetype", "application/epub+zip")
        archive.writestr("META-INF/container.xml", '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
        archive.writestr("content.opf", '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-test</dc:identifier><dc:title>Reader-Test</dc:title><dc:language>de</dc:language></metadata><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="chapter2" href="chapter2.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="chapter"/><itemref idref="chapter2"/></spine></package>')
        archive.writestr("chapter.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Kapitel eins</title></head><body><h1>Eine Geschichte</h1><p>Ein Buch zum Testen des Readers.</p></body></html>')
        archive.writestr("chapter2.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Kapitel zwei</title></head><body><h1>Die Fortsetzung</h1><p>Hier geht die Geschichte weiter.</p></body></html>')
        archive.writestr("nav.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Inhalt</title></head><body><nav epub:type="toc"><ol><li><a href="chapter.xhtml">Kapitel eins</a></li><li><a href="chapter2.xhtml">Kapitel zwei</a></li></ol></nav></body></html>')
    return data.getvalue()


def run():
    with tempfile.TemporaryDirectory(prefix="leaves-reader-") as directory:
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"

        def request(path, data=None, headers=None):
            with urlopen(Request(base + path, data=data, headers=headers or {}), timeout=30) as response:
                return response.read()

        def upload(title, author, extension="epub"):
            boundary = uuid.uuid4().hex
            body = b""
            for name, value in (("title", title), ("author", author)):
                body += f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode()
            content = epub() + title.encode() if extension == "epub" else b"%PDF-1.4 test"
            body += f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="test.{extension}"\r\n\r\n'.encode() + content + f"\r\n--{boundary}--\r\n".encode()
            request("/books", body, {"Content-Type": f"multipart/form-data; boundary={boundary}"})
            page = request("/?" + urlencode({"q": title})).decode()
            return re.search(r'href="/books/([a-f0-9-]+)/download"', page)[1]

        process = subprocess.Popen([str(binary)], env={**os.environ, "DATA_DIR": directory, "LISTEN_ADDR": f"127.0.0.1:{port}"}, stdout=subprocess.DEVNULL)
        try:
            for _ in range(100):
                try:
                    request("/health")
                    break
                except OSError:
                    if process.poll() is not None:
                        raise AssertionError("Server failed to start")
                    time.sleep(0.05)
            author = 'Änne & Co + "Test" / ?'
            first = upload("Erstes Buch", author)
            second = upload("Zweites Buch", author)
            other = upload("Drittes Buch", author + " Junior")
            pdf = upload("PDF-Buch", "Andere", "pdf")
            page = request("/?" + urlencode({"author": author})).decode()
            assert first in page and second in page and other not in page and pdf not in page
            assert '/?author=' + quote(author, safe="") in page
            assert 'name="author"' in page and "Bücher von" in page
            filtered = request("/?" + urlencode({"author": author, "q": "Zweites", "status": "unread"})).decode()
            assert second in filtered and first not in filtered
            assert first not in request("/?" + urlencode({"author": author, "status": "read"})).decode()
            reader = request(f"/books/{first}/read").decode()
            assert "Erstes Buch" in reader and 'id="viewer"' in reader
            assert f'data-book-url="/books/{first}/download"' in reader
            assert '/vendor/epub.min.js' in reader and '/vendor/jszip.min.js' in reader
            assert request(f"/books/{first}/download").startswith(b"PK")
            for path in ("/reader.js", "/vendor/epub.min.js", "/vendor/jszip.min.js"):
                with urlopen(base + path) as response:
                    assert "javascript" in response.headers["Content-Type"]
                    assert len(response.read()) > 100
            for path, code in ((f"/books/{pdf}/read", 415), ("/books/missing/read", 404)):
                try:
                    request(path)
                    raise AssertionError("Expected error")
                except HTTPError as error:
                    assert error.code == code
            if os.environ.get("LEAVES_BROWSER_TEST"):
                subprocess.run([os.environ.get("NODE", "node"), str(Path(__file__).with_name("reader_browser.cjs")), base, first, author], check=True)
            print("OK: exact Unicode author filter, combined filters, EPUB reader, assets, download, unsupported/missing books")
        finally:
            process.terminate()
            process.wait(timeout=10)


if __name__ == "__main__":
    run()
