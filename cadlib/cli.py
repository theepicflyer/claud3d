"""`cad` command line: new, build, open, profiles, list."""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

from . import bambu
from .config import ROOT
from .project import parse_overrides, resolve_project, run_build

TEMPLATE = Path(__file__).parent / "template"


def cmd_new(a) -> int:
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", a.name):
        print("project names are lowercase-kebab-case", file=sys.stderr)
        return 2
    dest = ROOT / "projects" / a.name
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
    target = out / "slice" / f"{project.name}.gcode.3mf"
    if not target.exists():
        print("no sliced project yet; run `uv run cad build` first", file=sys.stderr)
        return 1
    bambu.open_in_studio(target)
    print(f"opened {target.relative_to(ROOT)}")
    return 0


def cmd_profiles(a) -> int:
    for n in bambu.list_profiles(a.kind, a.pattern or ""):
        print(n)
    return 0


def cmd_list(a) -> int:
    for d in sorted((ROOT / "projects").iterdir()):
        if (d / "model.py").exists():
            rep = d / "out" / "report.md"
            status = re.search(r"Overall: \*\*(\w+)\*\*", rep.read_text()).group(1) if rep.exists() else "not built"
            print(f"{d.name:30s} {status}")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="cad", description="Code-driven CAD for 3D printing")
    sub = ap.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("new", help="create projects/<name> from the template")
    s.add_argument("name")
    s.set_defaults(fn=cmd_new)

    s = sub.add_parser("build", help="build, export, check, render and slice a project")
    s.add_argument("project", help="project name under projects/ or a path")
    s.add_argument("-p", "--param", action="append", metavar="KEY=VALUE", help="override a PARAMS entry")
    s.add_argument("--no-render", action="store_true")
    s.add_argument("--no-slice", action="store_true")
    s.add_argument("--open", action="store_true", help="open the sliced project in Bambu Studio afterwards")
    s.set_defaults(fn=cmd_build)

    s = sub.add_parser("open", help="open the latest build in Bambu Studio (or STEP files in FreeCAD)")
    s.add_argument("project")
    s.add_argument("--step", action="store_true", help="open STEP files in FreeCAD instead")
    s.set_defaults(fn=cmd_open)

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
