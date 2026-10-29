"""Metadata integration tests against a running service. Test books are removed."""
import io
import re
import struct
import uuid
import zipfile
from urllib.parse import quote
from urllib.request import Request, urlopen

BASE = "http://127.0.0.1:8080"
TOKEN = uuid.uuid4().hex


def call(path, data=None, content_type=None):
    headers = {"Content-Type": content_type} if content_type else {}
    with urlopen(Request(BASE + path, data=data, headers=headers), timeout=30) as r:
        return r.read()


def upload(filename, data, title="", author=""):
    boundary = uuid.uuid4().hex
    parts = []
    for name, value in (("title", title), ("author", author)):
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode())
    parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{filename}"\r\nContent-Type: application/octet-stream\r\n\r\n'.encode())
    parts.extend([data, f"\r\n--{boundary}--\r\n".encode()])
    return call("/books", b"".join(parts), f"multipart/form-data; boundary={boundary}").decode()


def check(filename, data, expected_title, expected_author, title="", author=""):
    page = upload(filename, data, title, author)
    book_id = None
    try:
        page = call("/?q=" + quote(expected_title)).decode()
        matches = re.findall(r'href="/books/([a-f0-9-]+)/download"', page)
        assert len(matches) == 1, (filename, "Expected one matching book", page)
        book_id = matches[0]
        assert expected_author in page, (filename, "Author missing", page)
        assert call(f"/books/{book_id}/download") == data
    finally:
        if book_id:
            call(f"/books/{book_id}/delete", b"")


def epub():
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("META-INF/container.xml", '<container><rootfiles><rootfile full-path="OPS/book.opf"/></rootfiles></container>')
        z.writestr("OPS/book.opf", f'<package xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>EPUB-{TOKEN} &amp; Meer</dc:title><dc:creator>Jörg Müller</dc:creator><dc:creator>Anna Beispiel</dc:creator></metadata></package>')
    return output.getvalue()


def pdf():
    chunks = [b"%PDF-1.4\n"]
    offsets = [0]
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Kids [] /Count 0 >>",
               f"<< /Title (PDF-{TOKEN}) /Author (PDF Author) >>".encode()]
    for i, obj in enumerate(objects, 1):
        offsets.append(sum(map(len, chunks)))
        chunks.append(f"{i} 0 obj\n".encode() + obj + b"\nendobj\n")
    xref = sum(map(len, chunks))
    chunks.append(b"xref\n0 4\n0000000000 65535 f \n")
    chunks.extend(f"{offset:010d} 00000 n \n".encode() for offset in offsets[1:])
    chunks.append(f"trailer\n<< /Size 4 /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())
    return b"".join(chunks)


def mobi():
    header = bytearray(86)
    header[60:68] = b"BOOKMOBI"
    struct.pack_into(">H", header, 76, 1)
    struct.pack_into(">I", header, 78, 86)
    record = bytearray(16 + 232)
    record[16:20] = b"MOBI"
    struct.pack_into(">I", record, 20, 232)
    struct.pack_into(">I", record, 28, 65001)
    struct.pack_into(">I", record, 128, 0x40)
    records = []
    for kind, value in ((100, "MOBI Autor"), (503, f"MOBI-{TOKEN}")):
        data = value.encode()
        records.append(struct.pack(">II", kind, 8 + len(data)) + data)
    exth = b"EXTH" + struct.pack(">II", 12 + sum(map(len, records)), len(records)) + b"".join(records)
    return bytes(header + record) + exth


check("book.epub", epub(), f"EPUB-{TOKEN} & Meer", "Jörg Müller, Anna Beispiel")
check("book.epub", epub(), f"Manual-{TOKEN}", "Manual Author", f"Manual-{TOKEN}", "Manual Author")
check("book.epub", epub(), f"Partial-{TOKEN}", "Jörg Müller", f"Partial-{TOKEN}")
check("book.pdf", pdf(), f"PDF-{TOKEN}", "PDF Author")
check("book.mobi", mobi(), f"MOBI-{TOKEN}", "MOBI Autor")
check("book.azw3", mobi(), f"MOBI-{TOKEN}", "MOBI Autor")
check(f"Fallback-{TOKEN}.epub", b"broken zip", f"Fallback-{TOKEN}", "")
print("OK: EPUB, PDF, MOBI, AZW3, manual overrides, partial override, malformed fallback, original downloads")
