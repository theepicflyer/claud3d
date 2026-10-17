import json
import threading
import urllib.request

import pytest

from cadlib.review import add_reply, apply_submission, format_threads, load_threads
from cadlib.review.server import ReviewServer

PNG = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="


@pytest.fixture
def project(tmp_path):
    p = tmp_path / "demo"
    (p / "out").mkdir(parents=True)
    (p / "out" / "view.json").write_text(json.dumps({"commit": "abc", "dirty": False, "built_at": "now", "parts": {}}))
    (p / "out" / "part.stl").write_text("solid x\nendsolid x\n")
    return p


def test_submission_creates_threads_and_replies(project):
    anchor = {"kind": "circle", "part": "lid", "point": [1, 2, 3], "normal": [0, 0, 1], "radius": 4.0,
              "parts": ["lid", "box"], "view": "assembly", "frame": "part"}
    created, _ = apply_submission(project, {"new": [{"text": "too thin", "anchor": anchor, "screenshot": PNG},
                                                    {"text": "   "}]}, {"commit": "abc"})
    assert [t["id"] for t in created] == [1]
    assert (project / "review" / "shots" / "1.png").read_bytes().startswith(b"\x89PNG")
    add_reply(project, 1, "Thickened to 2 mm", "resolved")
    _, replied = apply_submission(project, {"replies": [{"id": 1, "text": "still thin"}]}, None)
    t = load_threads(project)[0]
    assert replied and t["status"] == "open" and [m["by"] for m in t["messages"]] == ["user", "claude", "user"]
    text = format_threads(project, [t])
    assert "#1 OPEN [lid] circle r=4.0 mm at (1.0, 2.0, 3.0) (also touches box)" in text
    assert "claude: Thickened to 2 mm" in text and "shots/1.png" in text


def test_server_round_trip(project):
    srv = ReviewServer(project, 0, keep_running=False)
    port = srv.server_address[1]
    th = threading.Thread(target=srv.serve_forever, daemon=True)
    th.start()
    base = f"http://127.0.0.1:{port}"
    state = json.loads(urllib.request.urlopen(f"{base}/api/state").read())
    assert state["project"] == "demo" and state["view"]["commit"] == "abc" and state["threads"] == []
    assert urllib.request.urlopen(f"{base}/out/part.stl").read().startswith(b"solid")
    with pytest.raises(urllib.error.HTTPError):
        urllib.request.urlopen(f"{base}/out/../out/view.json/../../../etc/passwd")
    body = json.dumps({"new": [{"text": "general note", "anchor": None}]}).encode()
    req = urllib.request.Request(f"{base}/api/send", body, {"Content-Type": "application/json"})
    resp = json.loads(urllib.request.urlopen(req).read())
    assert resp["created"] == [1] and resp["closing"] and resp["threads"][0]["build"]["commit"] == "abc"
    th.join(timeout=5)
    assert not th.is_alive()
    srv.server_close()
