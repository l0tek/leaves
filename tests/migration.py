"""Verify legacy SQLite migration and persistence using an isolated Docker container."""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen
import uuid

docker = str(Path(os.environ["LOCALAPPDATA"]) / "Programs/DockerDesktop/resources/bin/docker.exe") if os.name == "nt" else "docker"
name = "leaves-migration-" + uuid.uuid4().hex[:12]
book_id = str(uuid.uuid4())


def run(*args):
    return subprocess.check_output([docker, *args], text=True, stderr=subprocess.STDOUT).strip()


with tempfile.TemporaryDirectory(prefix="leaves-migration-") as folder:
    data = Path(folder)
    (data / "books").mkdir()
    original = b"%PDF-1.4\nLegacy book fixture"
    (data / "books" / book_id).write_bytes(original)
    with sqlite3.connect(data / "library.sqlite") as db:
        db.execute("CREATE TABLE books (id TEXT PRIMARY KEY, title TEXT NOT NULL, author TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'unread', format TEXT NOT NULL, size INTEGER NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)")
        db.execute("INSERT INTO books(id,title,author,format,size) VALUES(?,?,?,?,?)", (book_id, "Legacy title", "Legacy author", "pdf", len(original)))
    db.close()
    try:
        run("run", "-d", "--name", name, "-p", "127.0.0.1::8080", "--mount", f"type=bind,source={folder},target=/data", "leaves-leaves")
        info = json.loads(run("inspect", name))[0]
        port = info["NetworkSettings"]["Ports"]["8080/tcp"][0]["HostPort"]
        base = "http://127.0.0.1:" + port

        def ready():
            global base
            # Docker can assign a different ephemeral host port after restart.
            info = json.loads(run("inspect", name))[0]
            port = info["NetworkSettings"]["Ports"]["8080/tcp"][0]["HostPort"]
            base = "http://127.0.0.1:" + port
            for _ in range(60):
                try:
                    with urlopen(base + "/health", timeout=2) as response:
                        if response.read() == b"ok":
                            return
                except OSError:
                    time.sleep(.25)
            raise AssertionError(run("logs", name))

        ready()
        with urlopen(base) as response:
            html = response.read().decode()
            assert "Legacy title" in html and "Legacy author" in html and "Ohne Kategorie" in html
        edit = urlencode(dict(title="Legacy title", author="Legacy author", status="reading", category="Fachbuch", tags="Rust, rust, computer")).encode()
        with urlopen(Request(base + f"/books/{book_id}", data=edit, headers={"Content-Type": "application/x-www-form-urlencoded"})) as response:
            assert response.status == 200
        run("restart", name)
        ready()
        with urlopen(base + "/?category=Fachbuch&tag=rust&status=reading") as response:
            assert book_id in response.read().decode()
        with urlopen(base + f"/books/{book_id}/download") as response:
            assert response.read() == original
        with sqlite3.connect(data / "library.sqlite") as db:
            assert db.execute("SELECT count(*) FROM books").fetchone()[0] == 1
            assert db.execute("SELECT count(*) FROM book_tags").fetchone()[0] == 2
        db.close()
        print("OK: legacy migration, preserved metadata/files, classification persistence, repeat startup")
    finally:
        run("rm", "-f", name)
