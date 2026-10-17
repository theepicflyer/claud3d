"""Local web server for the 3D review app."""

from __future__ import annotations

import json
import mimetypes
import threading
import time
import webbrowser
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlparse

from .. import bambu
from . import apply_submission, format_threads, load_threads, review_dir

STATIC = Path(__file__).parent / "static"
mimetypes.add_type("text/javascript", ".js")
mimetypes.add_type("model/stl", ".stl")


def default_port(project_name: str) -> int:
    """Stable per project, so an open browser tab reconnects when the review restarts."""
    return 8700 + zlib.crc32(project_name.encode()) % 200


def _mtime(p: Path) -> float:
    return p.stat().st_mtime if p.exists() else 0.0


def app_version() -> str:
    """Changes whenever the viewer's own files change, so open tabs reload themselves."""
    return f"{max(_mtime(f) for f in STATIC.rglob('*') if f.is_file()):.3f}"


class ReviewServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def __init__(self, project: Path, port: int, keep_running: bool):
        self.project = project
        self.keep_running = keep_running
        self.last_request = 0.0
        self.submissions: list[str] = []
        super().__init__(("127.0.0.1", port), Handler)

    def version(self) -> str:
        out = self.project / "out" / "view.json"
        comments = review_dir(self.project) / "comments.json"
        return f"{_mtime(out):.3f}-{_mtime(comments):.3f}"

    def state(self) -> dict:
        view_file = self.project / "out" / "view.json"
        view = json.loads(view_file.read_text()) if view_file.exists() else None
        return {"project": self.project.name, "view": view, "threads": load_threads(self.project),
                "version": self.version()}


class Handler(BaseHTTPRequestHandler):
    server: ReviewServer

    def log_message(self, *args) -> None:
        pass

    def _send(self, code: int, body: bytes, ctype: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _json(self, data, code: int = 200) -> None:
        self._send(code, json.dumps(data).encode(), "application/json")

    def _file(self, base: Path, rel: str) -> None:
        target = (base / rel).resolve()
        if not target.is_relative_to(base.resolve()) or not target.is_file():
            self._send(404, b"not found", "text/plain")
            return
        ctype = mimetypes.guess_type(target.name)[0] or "application/octet-stream"
        self._send(200, target.read_bytes(), ctype)

    def do_GET(self) -> None:
        self.server.last_request = time.time()
        path = unquote(urlparse(self.path).path)
        if path in ("/", "/index.html"):
            self._file(STATIC, "index.html")
        elif path.startswith("/static/"):
            self._file(STATIC, path[len("/static/"):])
        elif path.startswith("/out/"):
            self._file(self.server.project / "out", path[len("/out/"):])
        elif path.startswith("/review/"):
            self._file(review_dir(self.server.project), path[len("/review/"):])
        elif path == "/api/state":
            self._json(self.server.state())
        elif path == "/api/version":
            self._json({"project": self.server.project.name, "version": self.server.version(), "app": app_version()})
        else:
            self._send(404, b"not found", "text/plain")

    def do_POST(self) -> None:
        self.server.last_request = time.time()
        path = urlparse(self.path).path
        if path == "/api/open-studio":
            self._open_studio()
            return
        if path != "/api/send":
            self._send(404, b"not found", "text/plain")
            return
        payload = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        view = self.server.state()["view"] or {}
        build = {k: view.get(k) for k in ("commit", "dirty", "built_at")}
        created, replied = apply_submission(self.server.project, payload, build)
        summary = format_threads(self.server.project, created + replied)
        self.server.submissions.append(summary)
        self._json({"ok": True, "created": [t["id"] for t in created], "replied": [t["id"] for t in replied],
                    "threads": load_threads(self.server.project), "version": self.server.version(),
                    "closing": not self.server.keep_running})
        print(f"REVIEW {self.server.project.name}: {len(created)} new comment(s), {len(replied)} updated thread(s). "
              "Coordinates are in each part's model frame (mm).", flush=True)
        print(summary or "(nothing new)", flush=True)
        if not self.server.keep_running:
            threading.Thread(target=self.server.shutdown, daemon=True).start()


    def _open_studio(self) -> None:
        target = bambu.project_3mf(self.server.project)
        if not target.exists():
            self._json({"ok": False, "error": "No sliced project yet. Run `uv run cad build` first."}, 404)
            return
        try:
            bambu.open_in_studio(target)
        except RuntimeError as e:
            self._json({"ok": False, "error": str(e)}, 500)
            return
        self._json({"ok": True})


def serve(project: Path, port: int | None = None, open_browser: bool = True, keep_running: bool = False) -> None:
    port = port or default_port(project.name)
    srv = ReviewServer(project, port, keep_running)
    url = f"http://127.0.0.1:{port}/"
    print(f"review app for {project.name} at {url}", flush=True)

    def maybe_open() -> None:
        # An open tab polls every 2 s; only launch a browser if nothing reconnects.
        time.sleep(4)
        if srv.last_request == 0.0:
            webbrowser.open(url)

    if open_browser:
        threading.Thread(target=maybe_open, daemon=True).start()
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        srv.server_close()
