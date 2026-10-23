"""`cad` command line: new, build, open, review, comments, reply, profiles, list."""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

from . import bambu
from .config import PROJECT_DIRS, ROOT
from .project import parse_overrides, resolve_project, run_build

TEMPLATE = Path(__file__).parent / "template"


def cmd_new(a) -> int:
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", a.name):
        print("project names are lowercase-kebab-case", file=sys.stderr)
        return 2
    dest = (ROOT / "private" if a.private else ROOT / "projects") / a.name
    if dest.exists():
        print(f"{dest} already exists", file=sys.stderr)
        return 1
    shutil.copytree(TEMPLATE, dest)
    for f in dest.rglob("*"):
        if f.is_file():
            f.write_text(f.read_text().replace("{{name}}", a.name))
    print(dest.relative_to(ROOT))
    return 0


def cmd_build(a) -> int:
    project = resolve_project(a.project)
    res = run_build(project, parse_overrides(a.param or []), do_render=not a.no_render, do_slice=not a.no_slice)
    print((project / "out" / "report.md").read_text())
    if a.open and res.slice and res.slice.project_3mf:
        bambu.open_in_studio(res.slice.project_3mf)
    return 1 if res.status == "FAIL" else 0


def cmd_open(a) -> int:
    project = resolve_project(a.project)
    out = project / "out"
    if a.step:
        steps = sorted(out.glob("*.step"))
        if not steps or not shutil.which("freecad"):
            print("no STEP files (run `uv run cad build` first) or FreeCAD not installed", file=sys.stderr)
            return 1
        subprocess.Popen(["freecad", *map(str, steps)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         start_new_session=True)
        return 0
    target = bambu.project_3mf(project)
    if not target.exists():
        print("no sliced project yet; run `uv run cad build` first", file=sys.stderr)
        return 1
    bambu.open_in_studio(target)
    print(f"opened {target.relative_to(ROOT)}")
    return 0


def cmd_review(a) -> int:
    from .review.server import serve
    project = resolve_project(a.project)
    if not (project / "out" / "view.json").exists():
        print("no build yet; run `uv run cad build` first", file=sys.stderr)
        return 1
    try:
        serve(project, a.port, open_browser=not a.no_open, keep_running=a.keep)
    except OSError as e:
        print(f"could not start the review server: {e}. Is another review of this project still running?",
              file=sys.stderr)
        return 1
    return 0


def cmd_comments(a) -> int:
    from .review import format_threads, load_threads
    project = resolve_project(a.project)
    threads = [t for t in load_threads(project) if a.all or t["status"] == "open"]
    print(format_threads(project, threads) if threads else "no open comments")
    return 0


def cmd_reply(a) -> int:
    from .review import add_reply, format_thread
    project = resolve_project(a.project)
    status = "resolved" if a.resolve else "open" if a.reopen else None
    print(format_thread(project, add_reply(project, a.id, a.text, status)))
    return 0


def cmd_profiles(a) -> int:
    for n in bambu.list_profiles(a.kind, a.pattern or ""):
        print(n)
    return 0


def cmd_list(a) -> int:
    for d in sorted(d for base in PROJECT_DIRS if base.is_dir() for d in base.iterdir()):
        if (d / "model.py").exists():
            rep = d / "out" / "report.md"
            status = re.search(r"Overall: \*\*(\w+)\*\*", rep.read_text()).group(1) if rep.exists() else "not built"
            print(f"{d.name:30s} {status:10s} {d.parent.name}/")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="cad", description="Code-driven CAD for 3D printing")
    sub = ap.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("new", help="create projects/<name> (or private/<name>) from the template")
    s.add_argument("name")
    s.add_argument("--private", action="store_true", help="create it under the gitignored private/ folder")
    s.set_defaults(fn=cmd_new)

    s = sub.add_parser("build", help="build, export, check, render and slice a project")
    s.add_argument("project", help="project name under projects/ or private/, or a path")
    s.add_argument("-p", "--param", action="append", metavar="KEY=VALUE", help="override a PARAMS entry")
    s.add_argument("--no-render", action="store_true")
    s.add_argument("--no-slice", action="store_true")
    s.add_argument("--open", action="store_true", help="open the sliced project in Bambu Studio afterwards")
    s.set_defaults(fn=cmd_build)

    s = sub.add_parser("open", help="open the latest build in Bambu Studio (or STEP files in FreeCAD)")
    s.add_argument("project")
    s.add_argument("--step", action="store_true", help="open STEP files in FreeCAD instead")
    s.set_defaults(fn=cmd_open)

    s = sub.add_parser("review", help="open the 3D review app; exits with the comments when you press Send")
    s.add_argument("project")
    s.add_argument("--port", type=int, help="default is stable per project")
    s.add_argument("--no-open", action="store_true", help="don't launch a browser")
    s.add_argument("--keep", action="store_true", help="keep serving after comments are sent")
    s.set_defaults(fn=cmd_review)

    s = sub.add_parser("comments", help="print review threads (open ones by default)")
    s.add_argument("project")
    s.add_argument("--all", action="store_true")
    s.set_defaults(fn=cmd_comments)

    s = sub.add_parser("reply", help="reply to a review thread as Claude")
    s.add_argument("project")
    s.add_argument("id", type=int)
    s.add_argument("text")
    g = s.add_mutually_exclusive_group()
    g.add_argument("--resolve", action="store_true")
    g.add_argument("--reopen", action="store_true")
    s.set_defaults(fn=cmd_reply)

    s = sub.add_parser("profiles", help="list Bambu Studio presets")
    s.add_argument("kind", choices=bambu.KINDS)
    s.add_argument("pattern", nargs="?", help="regex filter")
    s.set_defaults(fn=cmd_profiles)

    s = sub.add_parser("list", help="list projects and their last build status")
    s.set_defaults(fn=cmd_list)

    a = ap.parse_args(argv)
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())
