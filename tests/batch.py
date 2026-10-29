"""Batch import integration test; creates and removes its own books."""
import io
import os
import re
import uuid
import zipfile
from html.parser import HTMLParser
from urllib.parse import quote
from urllib.request import Request, urlopen

BASE = os.environ.get("BASE_URL", "http://127.0.0.1:8080")
token = "Batch-" + uuid.uuid4().hex


def call(path, data=None, content_type=None):
    headers = {"Content-Type": content_type} if content_type else {}
    with urlopen(Request(BASE + path, data=data, headers=headers), timeout=60) as response:
        return response.read()


class DirectoryInputParser(HTMLParser):
    found = False

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "input" and attrs.get("name") == "files":
            self.found = attrs.get("type") == "file" and "webkitdirectory" in attrs


parser = DirectoryInputParser()
parser.feed(call("/").decode())
assert parser.found, "Ordnerauswahl fehlt im gerenderten HTML"

output = io.BytesIO()
with zipfile.ZipFile(output, "w") as archive:
    archive.writestr("META-INF/container.xml", '<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>')
    archive.writestr("book.opf", f'<package xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>{token}-Metadaten</dc:title><dc:creator>Batch Autor</dc:creator></metadata></package>')
books = [
    (f"Ordner/{token}-Dateiname.PDF", b"PDF test data"),
    (f"Ordner/Unterordner/{token}.epub", output.getvalue()),
    (f"Ordner/Kopie/{token}-umbenannt.epub", output.getvalue()),
    ("Ordner/notizen.txt", b"skip me"),
    ("Ordner/leer.mobi", b""),
    ("Ordner/zu-gross.pdf", b"x" * (53 * 1024 * 1024)),
    (f"Ordner/{token}-Danach.azw3", b"AZW3 test data"),
]
boundary = uuid.uuid4().hex
body = bytearray()
for filename, data in books:
    body.extend(f'--{boundary}\r\nContent-Disposition: form-data; name="files"; filename="{filename}"\r\nContent-Type: application/octet-stream\r\n\r\n'.encode())
    body.extend(data)
    body.extend(b"\r\n")
body.extend(f"--{boundary}--\r\n".encode())
try:
    report = call("/books/import", bytes(body), f"multipart/form-data; boundary={boundary}").decode()
    assert "3 importiert · 2 übersprungen · 2 fehlgeschlagen" in report, report
    assert "Davon 1 bereits vorhanden." in report, report
    repeated = call("/books/import", bytes(body), f"multipart/form-data; boundary={boundary}").decode()
    assert "0 importiert · 5 übersprungen · 2 fehlgeschlagen" in repeated, repeated
    page = call("/?q=" + quote(token)).decode()
    ids = re.findall(r'href="/books/([a-f0-9-]+)/download"', page)
    assert len(ids) == 3, page
    assert f"{token}-Metadaten" in page and "Batch Autor" in page, page
    assert f"{token}-Dateiname" in page, page
    downloads = [call(f"/books/{book_id}/download") for book_id in ids]
    assert all(data in downloads for filename, data in books if data and len(data) <= 50 * 1024 * 1024 and not filename.endswith(".txt"))
    print("Batch import: OK (recursive paths, formats, metadata, skipped files, partial failure, downloads)")
finally:
    page = call("/?q=" + quote(token)).decode()
    for book_id in re.findall(r'href="/books/([a-f0-9-]+)/download"', page):
        call(f"/books/{book_id}/delete", b"")
