"""End-to-end test against a running Leaves instance; uses only the standard library."""
import re
import sys
import urllib.parse
import urllib.request
import uuid

base = (sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080").rstrip("/")


def request(path, data=None, headers=None):
    req = urllib.request.Request(base + path, data=data, headers=headers or {})
    with urllib.request.urlopen(req, timeout=30) as response:
        return response.read()


assert request("/health") == b"ok"
title = "Smoke-" + uuid.uuid4().hex
boundary = "leaves" + uuid.uuid4().hex
original = b"%PDF-1.4\n% Leaves smoke test fixture\n"
body = (
    f'--{boundary}\r\nContent-Disposition: form-data; name="title"\r\n\r\n{title}\r\n'
    f'--{boundary}\r\nContent-Disposition: form-data; name="author"\r\n\r\nTestautor\r\n'
    f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="test.pdf"\r\n'
    'Content-Type: application/pdf\r\n\r\n'
).encode() + original + f"\r\n--{boundary}--\r\n".encode()
request("/books", body, {"Content-Type": f"multipart/form-data; boundary={boundary}"})
page = request("/?q=" + title).decode()
match = re.search(r'href="/books/([a-f0-9-]+)/download"', page)
assert match, "Uploaded book missing from search"
book_id = match.group(1)
try:
    assert request(f"/books/{book_id}/download") == original
    request(f"/books/{book_id}", urllib.parse.urlencode({
        "title": title, "author": "Neuer Autor", "status": "reading"
    }).encode(), {"Content-Type": "application/x-www-form-urlencoded"})
    page = request("/?q=" + title + "&status=reading").decode()
    assert "Neuer Autor" in page and book_id in page
    assert book_id not in request("/?q=" + title + "&status=read").decode()
finally:
    request(f"/books/{book_id}/delete", b"")
assert book_id not in request("/?q=" + title).decode()
print("OK: health, upload, search, download, update, status filter, delete")
