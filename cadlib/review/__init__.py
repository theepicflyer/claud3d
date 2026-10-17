"""Review comments: threads anchored to points or circled regions on a project's parts."""

from __future__ import annotations

import base64
import json
import time
from pathlib import Path


def review_dir(project: Path) -> Path:
    return project / "review"


def load_threads(project: Path) -> list[dict]:
    f = review_dir(project) / "comments.json"
    return json.loads(f.read_text())["threads"] if f.exists() else []


def save_threads(project: Path, threads: list[dict]) -> None:
    d = review_dir(project)
    d.mkdir(exist_ok=True)
    (d / "comments.json").write_text(json.dumps({"threads": threads}, indent=1))


def _now() -> str:
    return time.strftime("%Y-%m-%d %H:%M:%S")


def _save_png(project: Path, name: str, data_url: str | None) -> str | None:
    if not data_url or not data_url.startswith("data:image/png;base64,"):
        return None
    shots = review_dir(project) / "shots"
    shots.mkdir(parents=True, exist_ok=True)
    (shots / name).write_bytes(base64.b64decode(data_url.split(",", 1)[1]))
    return f"shots/{name}"


def apply_submission(project: Path, payload: dict, build: dict | None) -> tuple[list[dict], list[dict]]:
    """Add new threads and user replies from the review app. Returns (new threads, threads that got replies)."""
    threads = load_threads(project)
    by_id = {t["id"]: t for t in threads}
    next_id = max(by_id, default=0) + 1
    created = []
    for c in payload.get("new", []):
        text = (c.get("text") or "").strip()
        if not text:
            continue
        t = {
            "id": next_id,
            "status": "open",
            "created": _now(),
            "build": build,
            "anchor": c.get("anchor"),
            "camera": c.get("camera"),
            "screenshot": _save_png(project, f"{next_id}.png", c.get("screenshot")),
            "messages": [{"by": "user", "text": text, "at": _now()}],
        }
        threads.append(t)
        created.append(t)
        next_id += 1
    replied = []
    for r in payload.get("replies", []):
        t = by_id.get(r.get("id"))
        if not t:
            continue
        text = (r.get("text") or "").strip()
        if text:
            t["messages"].append({"by": "user", "text": text, "at": _now()})
        if r.get("status") in ("open", "resolved"):
            t["status"] = r["status"]
        elif text:
            t["status"] = "open"
        if text or r.get("status"):
            replied.append(t)
    save_threads(project, threads)
    return created, replied


def add_reply(project: Path, thread_id: int, text: str, status: str | None = None) -> dict:
    threads = load_threads(project)
    t = next((t for t in threads if t["id"] == thread_id), None)
    if t is None:
        raise KeyError(f"no thread #{thread_id}")
    if text:
        t["messages"].append({"by": "claude", "text": text, "at": _now()})
    if status:
        t["status"] = status
    save_threads(project, threads)
    return t


def _fmt_vec(v) -> str:
    return "(" + ", ".join(f"{x:.1f}" for x in v) + ")"


def format_thread(project: Path, t: dict) -> str:
    a = t.get("anchor")
    if not a:
        where = "general comment on the view in the screenshot"
    elif a["kind"] == "measure":
        snap_a = f" {a['snap_a']}" if a.get("snap_a") else ""
        snap_b = f" {a['snap_b']}" if a.get("snap_b") else ""
        where = (f"measured {a['distance']:.2f} mm from [{a['part']}]{snap_a} {_fmt_vec(a['point'])} to [{a['part_b']}]"
                 f"{snap_b} {_fmt_vec(a['point_b'])}, delta {_fmt_vec(a['delta'])} in {a.get('view', '?')} view frame")
        if a.get("axis"):
            where += f", along {a['axis']}"
        if a.get("face_distance") is not None:
            where += f", face to face {a['face_distance']:.2f} mm"
    elif a["kind"] == "circle":
        others = [p for p in a.get("parts", []) if p != a["part"]]
        where = f"[{a['part']}] circle r={a['radius']:.1f} mm at {_fmt_vec(a['point'])}"
        if others:
            where += f" (also touches {', '.join(others)})"
    else:
        where = f"[{a['part']}] point {_fmt_vec(a['point'])}"
    if a and a.get("feature"):
        where += f" near feature '{a['feature']}'"
    if a and a["kind"] != "measure":
        where += f" normal {_fmt_vec(a['normal'])}, seen in {a.get('view', '?')} view"
    b = t.get("build") or {}
    lines = [f"#{t['id']} {t['status'].upper()} {where}  (build {b.get('commit', '?')}{'+dirty' if b.get('dirty') else ''} {b.get('built_at', '')})"]
    for m in t["messages"]:
        lines.append(f"   {m['by']}: {m['text']}")
    if t.get("screenshot"):
        lines.append(f"   screenshot: {(review_dir(project) / t['screenshot']).as_posix()}")
    return "\n".join(lines)


def format_threads(project: Path, threads: list[dict]) -> str:
    return "\n".join(format_thread(project, t) for t in threads)
