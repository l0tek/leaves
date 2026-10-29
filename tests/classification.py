"""Classification integration tests; run against the local Docker service."""
import io
import re
import uuid
import zipfile
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

BASE = "http://127.0.0.1:8080"
TOKEN = uuid.uuid4().hex
created = []


def call(path, data=None, content_type=None, expected=200):
    headers = {"Content-Type": content_type} if content_type else {}
    try:
        with urlopen(Request(BASE + path, data=data, headers=headers), timeout=30) as response:
            assert response.status == expected
            return response.read().decode()
    except HTTPError as error:
        assert error.code == expected, error.read().decode()
        return error.read().decode()


def form(path, data=None, expected=200):
    return call(path, urlencode(data or {}).encode(), "application/x-www-form-urlencoded", expected)


def page(**filters):
    return call("/?" + urlencode(filters))


def upload(suffix, subjects, category="", tags=""):
    title = f"Classification-{TOKEN}-{suffix}"
    data = io.BytesIO()
    with zipfile.ZipFile(data, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("META-INF/container.xml", '<container><rootfile full-path="book.opf"/></container>')
        archive.writestr("book.opf", '<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>' + title + '</dc:title>' + ''.join('<dc:subject>' + subject + '</dc:subject>' for subject in subjects) + '</metadata></package>')
    boundary = uuid.uuid4().hex
    body = b""
    for name, value in (("category", category), ("tags", tags)):
        body += f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode()
    body += f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="test.epub"\r\nContent-Type: application/epub+zip\r\n\r\n'.encode()
    body += data.getvalue() + f"\r\n--{boundary}--\r\n".encode()
    call("/books", body, f"multipart/form-data; boundary={boundary}")
    result = page(q=title)
    book_id = re.search(r'href="/books/([a-f0-9-]+)/download"', result).group(1)
    created.append(book_id)
    return title, book_id, result


try:
    title, book_id, result = upload("automatic", ["Programming", "Rust", "rust", "MÜNCHEN"])
    assert "Vorschlag aus Metadaten" in result and "Fachbuch" in result
    assert book_id in page(q=title, category="__none")
    assert book_id not in page(q=title, category="Fachbuch")
    assert book_id not in page(q=title, tag="rust")
    form(f"/books/{book_id}/accept")
    assert book_id in page(q=title, category="Fachbuch", tag="rust")
    assert book_id in page(q=title, tag="MÜNCHEN")
    assert book_id not in page(q=title, tag="rus")
    assert "Vorschlag aus Metadaten" not in page(q=title)

    # Legacy metadata edits preserve classification.
    form(f"/books/{book_id}", dict(title=title, author="Autor", status="reading"))
    assert book_id in page(q=title, status="reading", category="Fachbuch", tag="rust")
    unique_tag = "test-" + TOKEN
    form(f"/books/{book_id}", dict(title=title, author="Autor", status="reading", category="Sachbuch", tags=f"{unique_tag}, RUST, rust; weltraum"))
    assert book_id in page(q=title, category="Sachbuch", tag=unique_tag)
    assert book_id not in page(q=title, tag="programming")
    # A newly generated suggestion must not replace a manually assigned category.
    form(f"/books/{book_id}/suggest")
    form(f"/books/{book_id}/accept")
    assert book_id in page(q=title, category="Sachbuch", tag="programming")
    form(f"/books/{book_id}", dict(title=title, author="", status="read", category="Invalid", tags=""), 400)
    form(f"/books/{book_id}", dict(title=title, author="", status="read", category="", tags="x" * 61), 400)
    assert book_id in page(q=title, category="Sachbuch", tag=unique_tag)
    form(f"/books/{book_id}", dict(title=title, author="", status="read", category="", tags=""))
    assert book_id in page(q=title, category="__none")
    assert book_id not in page(q=title, tag="rust")

    manual_title, manual_id, manual_page = upload("manual", ["Science Fiction"], "Kinderbuch", "Abenteuer")
    assert "Vorschlag aus Metadaten" not in manual_page
    assert manual_id in page(q=manual_title, category="Kinderbuch", tag="abenteuer")
    form(f"/books/{manual_id}/suggest")
    assert "Vorschlag aus Metadaten" in page(q=manual_title)
    form(f"/books/{manual_id}/dismiss")
    assert "Vorschlag aus Metadaten" not in page(q=manual_title)

    empty_title, empty_id, empty_page = upload("empty", [])
    assert "Vorschlag aus Metadaten" not in empty_page
    assert "Keine verwertbaren Themen-Metadaten" in form(f"/books/{empty_id}/suggest")
    for missing in ("accept", "dismiss", "suggest"):
        form(f"/books/{uuid.uuid4()}/{missing}", expected=404)
finally:
    for book_id in created:
        form(f"/books/{book_id}/delete")

assert TOKEN not in page()
print("OK: unconfirmed suggestions, accept/dismiss, EPUB namespaces, categories, exact tags, Unicode, combined filters, manual overrides, legacy edits, validation, cleanup")
