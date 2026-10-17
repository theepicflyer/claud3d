"""Load a project's model.py, build it, and produce exports, renders, checks and a slice."""

from __future__ import annotations

import ast
import importlib.util
import json
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from types import ModuleType, SimpleNamespace

from build123d import Pos, Shape, export_step

from . import bambu, checks, render
from .config import ROOT, Settings, load_settings


@dataclass
class BuildResult:
    project: Path
    reports: list[checks.PartReport] = field(default_factory=list)
    assembly_findings: list[checks.Finding] = field(default_factory=list)
    spec_findings: list[checks.Finding] = field(default_factory=list)
    slice: bambu.SliceResult | None = None
    images: list[Path] = field(default_factory=list)
    files: list[Path] = field(default_factory=list)

    @property
    def status(self) -> str:
        all_f = [r.status for r in self.reports] + [f.status for f in self.assembly_findings + self.spec_findings]
        if self.slice and not self.slice.ok:
            all_f.append(checks.FAIL)
        return max(all_f, key=checks._RANK.__getitem__, default=checks.PASS)


def resolve_project(arg: str) -> Path:
    p = Path(arg)
    for cand in (p, ROOT / "projects" / arg, ROOT / arg):
        if (cand / "model.py").is_file():
            return cand.resolve()
    raise FileNotFoundError(f"No model.py found for project {arg!r}")


def load_model(project: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location(f"cadproject_{project.name.replace('-', '_')}", project / "model.py")
    mod = importlib.util.module_from_spec(spec)
    sys.path.insert(0, str(project))
    try:
        spec.loader.exec_module(mod)
    finally:
        sys.path.remove(str(project))
    return mod


def parse_overrides(pairs: list[str]) -> dict:
    out = {}
    for pair in pairs:
        k, _, v = pair.partition("=")
        try:
            out[k.strip()] = ast.literal_eval(v)
        except (ValueError, SyntaxError):
            out[k.strip()] = v
    return out


def make_params(mod: ModuleType, overrides: dict) -> SimpleNamespace:
    params = dict(getattr(mod, "PARAMS", {}))
    unknown = set(overrides) - set(params)
    if unknown:
        raise KeyError(f"Unknown parameter(s) {sorted(unknown)}; known: {sorted(params)}")
    params.update(overrides)
    return SimpleNamespace(**params)


def _as_dict(parts) -> dict[str, Shape]:
    if isinstance(parts, Shape):
        return {"part": parts}
    return dict(parts)


def _to_bed(shape: Shape) -> Shape:
    z = shape.bounding_box().min.Z
    return Pos(0, 0, -z) * shape if abs(z) > 1e-6 else shape


def build_parts(mod: ModuleType, p: SimpleNamespace, s: Settings) -> dict[str, Shape]:
    """Parts come back in print orientation; each is dropped so its lowest point sits at z=0."""
    return {name: _to_bed(shape) for name, shape in _as_dict(mod.build(p, s)).items()}


def run_build(project: Path, overrides: dict | None = None, do_render: bool = True, do_slice: bool = True,
              log=print) -> BuildResult:
    t0 = time.time()
    mod = load_model(project)
    s = load_settings(getattr(mod, "PRINT", None))
    p = make_params(mod, overrides or {})
    out = project / "out"
    out.mkdir(exist_ok=True)
    res = BuildResult(project)

    parts = build_parts(mod, p, s)
    log(f"built {len(parts)} part(s) in {time.time() - t0:.1f}s: {', '.join(parts)}")
    sections = getattr(mod, "SECTIONS", {})
    stls = []
    for name, shape in parts.items():
        mesh = render.tessellate(shape)
        step, stl = out / f"{name}.step", out / f"{name}.stl"
        export_step(shape, str(step))
        mesh.export(stl)
        stls.append(stl)
        res.files += [step, stl]
        rep = checks.check_part(name, mesh, s)
        res.reports.append(rep)
        if do_render:
            hdr = "{:.1f} x {:.1f} x {:.1f} mm".format(*rep.stats["bbox"])
            img = render.part_sheet(name, shape, mesh, out / f"{name}.png", rep.overhang_faces, rep.bridge_faces,
                                    rep.thin_points, sections.get(name), hdr)
            res.images.append(img)

    if hasattr(mod, "assembly"):
        asm = _as_dict(mod.assembly(p, s, parts))
        res.assembly_findings = checks.check_interference(asm)
        if do_render:
            res.images.append(render.assembly_sheet(asm, out / "assembly.png", sections.get("assembly")))

    if hasattr(mod, "check"):
        for ok, msg in mod.check(p, s, parts):
            res.spec_findings.append(checks.Finding(checks.PASS if ok else checks.FAIL, "spec", msg))

    if do_slice:
        res.slice = bambu.slice_files(stls, out / "slice" / f"{project.name}.gcode.3mf", s.machine_profile,
                                      s.process_profile, s.material.filament_profile, out / "slice" / "profiles",
                                      overrides=s.slicer_overrides)
        if res.slice.project_3mf:
            res.files.append(res.slice.project_3mf)

    write_report(res, p, s)
    log(f"done in {time.time() - t0:.1f}s")
    return res


def _fmt_time(sec: float) -> str:
    h, m = divmod(int(sec) // 60, 60)
    return f"{h}h {m:02d}m" if h else f"{m}m {int(sec) % 60:02d}s"


def write_report(res: BuildResult, p: SimpleNamespace, s: Settings) -> Path:
    out = res.project / "out"
    lines = [f"# Build report: {res.project.name}", "", f"Overall: **{res.status}**", "",
             f"Material: {s.material.key} ({s.material.filament_profile}); process: {s.process_profile}", "",
             "Parameters: " + ", ".join(f"{k}={v}" for k, v in vars(p).items()), ""]
    for r in res.reports:
        st = r.stats
        lines += [f"## {r.name}: {r.status}", "",
                  "bbox {} x {} x {} mm, volume {} cm^3, bed contact {} mm^2, bodies {}".format(
                      *st["bbox"], st["volume_cm3"], st["bed_contact_mm2"], st["bodies"]), ""]
        lines += [f"- {f.status} {f.check}: {f.detail}" for f in r.findings] + [""]
    if res.assembly_findings:
        lines += ["## assembly", ""] + [f"- {f.status} {f.check}: {f.detail}" for f in res.assembly_findings] + [""]
    if res.spec_findings:
        lines += ["## spec checks", ""] + [f"- {f.status} {f.detail}" for f in res.spec_findings] + [""]
    if res.slice:
        sl = res.slice
        lines += ["## slice", ""]
        if sl.ok:
            lines += [f"- time {_fmt_time(sl.print_time_s or 0)}, filament {sl.filament_g or 0:.1f} g",
                      f"- project: {sl.project_3mf.relative_to(res.project)}"]
            lines += [f"- WARN slicer: {w}" for w in sl.warnings]
        else:
            lines += [f"- FAIL slicer: {sl.message}"]
        lines.append("")
    if res.images:
        lines += ["## renders", ""] + [f"- {i.relative_to(res.project)}" for i in res.images] + [""]
    md = out / "report.md"
    md.write_text("\n".join(lines))
    data = {
        "status": res.status,
        "params": vars(p),
        "parts": {r.name: {"status": r.status, "stats": r.stats, "findings": [vars(f) for f in r.findings]} for r in res.reports},
        "assembly": [vars(f) for f in res.assembly_findings],
        "spec": [vars(f) for f in res.spec_findings],
        "slice": None if not res.slice else {
            "ok": res.slice.ok, "message": res.slice.message, "print_time_s": res.slice.print_time_s,
            "filament_g": res.slice.filament_g, "warnings": res.slice.warnings},
    }
    (out / "report.json").write_text(json.dumps(data, indent=2, default=str))
    return md
