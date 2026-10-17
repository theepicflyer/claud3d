"""Load a project's model.py, build it, and produce exports, renders, checks and a slice."""

from __future__ import annotations

import ast
import importlib.util
import json
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from types import ModuleType, SimpleNamespace

import numpy as np
from build123d import Pos, Shape, export_step

from . import bambu, checks, render
from .config import ROOT, Settings, load_settings
from .topology import topology


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
    meshes = {}
    for name, shape in parts.items():
        mesh = meshes[name] = render.tessellate(shape)
        step, stl = out / f"{name}.step", out / f"{name}.stl"
        export_step(shape, str(step))
        mesh.export(stl)
        res.files += [step, stl]
        rep = checks.check_part(name, mesh, s)
        res.reports.append(rep)
        if do_render:
            hdr = "{:.1f} x {:.1f} x {:.1f} mm".format(*rep.stats["bbox"])
            img = render.part_sheet(name, shape, mesh, out / f"{name}.png", rep.overhang_faces, rep.bridge_faces,
                                    rep.thin_points, sections.get(name), hdr)
            res.images.append(img)

    asm = {}
    if hasattr(mod, "assembly"):
        asm = _as_dict(mod.assembly(p, s, parts))
        res.assembly_findings = checks.check_interference(asm)
        if do_render:
            res.images.append(render.assembly_sheet(asm, out / "assembly.png", sections.get("assembly")))

    if hasattr(mod, "check"):
        for ok, msg in mod.check(p, s, parts):
            res.spec_findings.append(checks.Finding(checks.PASS if ok else checks.FAIL, "spec", msg))

    if do_slice:
        placed = place_on_plate(meshes, s)
        if placed is None:
            res.slice = bambu.SliceResult(False, -1, "parts don't fit on one plate together; split them across projects")
        else:
            plate_dir = out / "slice" / "plate"
            plate_dir.mkdir(parents=True, exist_ok=True)
            plate_stls = []
            for name, m in placed.items():
                plate_stls.append(plate_dir / f"{name}.stl")
                m.export(plate_stls[-1])
            res.slice = bambu.slice_files(plate_stls, bambu.project_3mf(project), s.machine_profile,
                                          s.process_profile, s.material.filament_profile, out / "slice" / "profiles",
                                          overrides=s.slicer_overrides)
            if res.slice.project_3mf:
                res.files.append(res.slice.project_3mf)

    write_report(res, p, s)
    feats = [_feature(f, parts, p) for f in mod.features(p, s, parts)] if hasattr(mod, "features") else []
    write_view(res, s, meshes, asm, parts, feats, p)
    log(f"done in {time.time() - t0:.1f}s")
    return res


def _rigid_transform(src: np.ndarray, dst: np.ndarray) -> np.ndarray | None:
    """4x4 matrix mapping src points onto dst (same order), or None if they aren't a rigid motion of each other."""
    if src.shape != dst.shape or len(src) < 3:
        return None
    cs, cd = src.mean(axis=0), dst.mean(axis=0)
    u, _, vt = np.linalg.svd((src - cs).T @ (dst - cd))
    d = np.sign(np.linalg.det(vt.T @ u.T))
    rot = vt.T @ np.diag([1, 1, d]) @ u.T
    m = np.eye(4)
    m[:3, :3], m[:3, 3] = rot, cd - rot @ cs
    err = np.abs(src @ rot.T + m[:3, 3] - dst).max()
    return m if err < 1e-3 else None


def _git_info(project: Path) -> dict:
    def git(*args):
        r = subprocess.run(["git", *args], cwd=project, capture_output=True, text=True)
        return r.stdout.strip() if r.returncode == 0 else ""
    return {"commit": git("rev-parse", "--short", "HEAD"), "dirty": bool(git("status", "--porcelain"))}


def _feature(f: dict, parts: dict, p: SimpleNamespace) -> dict:
    """Normalize a model's feature callout: `at` may be a point, a Vector, or a face/edge (its center is used)."""
    if f["part"] not in parts:
        raise KeyError(f"feature {f['name']!r} refers to unknown part {f['part']!r}")
    at, normal = f["at"], f.get("normal")
    if isinstance(at, Shape):
        if normal is None and hasattr(at, "normal_at") and at.geom_type.name == "PLANE":
            normal = at.normal_at()
        at = at.center()
    vec = lambda v: [round(float(c), 4) for c in (tuple(v) if not hasattr(v, "X") else (v.X, v.Y, v.Z))]
    out = {"name": f["name"], "part": f["part"], "point": vec(at), "text": f.get("text", ""),
           "params": {k: getattr(p, k) for k in f.get("params", [])}}
    if normal is not None:
        out["normal"] = vec(normal)
    if f.get("view"):
        out["view"] = f["view"]
    return out


def write_view(res: BuildResult, s: Settings, meshes: dict, asm: dict, shapes: dict | None = None,
               features: list | None = None, p: SimpleNamespace | None = None) -> Path:
    """Describe the build for the review app: part STLs and topology, plate offsets, assembly transforms,
    feature callouts, parameters and a report summary."""
    out = res.project / "out"
    placed = place_on_plate(meshes, s) or {}
    parts, x = {}, 0.0
    for name, m in meshes.items():
        if name in placed:
            offset = (placed[name].bounds[0] - m.bounds[0]).tolist()
        else:
            offset = [x - m.bounds[0][0], 0.0, 0.0]
            x += m.extents[0] + 10
        parts[name] = {"stl": f"{name}.stl", "plate_offset": offset, "bbox": m.extents.round(2).tolist()}
        if shapes and name in shapes:
            (out / f"{name}.topo.json").write_text(json.dumps(topology(shapes[name])))
            parts[name]["topo"] = f"{name}.topo.json"
    assembly = {}
    asm_dir = out / "assembly"
    for name, shape in asm.items():
        am = render.tessellate(shape)
        mat = _rigid_transform(meshes[name].vertices, am.vertices) if name in meshes else None
        if mat is not None:
            assembly[name] = {"part": name, "matrix": mat.round(9).tolist()}
        else:
            asm_dir.mkdir(exist_ok=True)
            am.export(asm_dir / f"{name}.stl")
            assembly[name] = {"stl": f"assembly/{name}.stl"}
    rep = {r.name: {"status": r.status, "findings": [vars(f) for f in r.findings if f.status != checks.PASS]}
           for r in res.reports}
    view = {
        "project": res.project.name,
        "built_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        **_git_info(res.project),
        "status": res.status,
        "parts": parts,
        "assembly": assembly,
        "plate": {"nozzles": [{"name": z.name, "x": z.x, "y": z.y, "height": z.height} for z in s.nozzles]},
        "report": rep,
        "spec": [vars(f) for f in res.spec_findings],
        "features": features or [],
        "params": {k: v for k, v in vars(p).items()} if p else {},
    }
    path = out / "view.json"
    path.write_text(json.dumps(view, indent=1))
    return path


def place_on_plate(meshes: dict, s: Settings, gap: float = 5.0) -> dict | None:
    """Shelf-pack parts in bed coordinates. Uses the region every nozzle reaches when all parts fit it."""
    sizes = {n: m.bounds[1] - m.bounds[0] for n, m in meshes.items()}
    shared = s.shared
    region = shared if all(checks.fits(v, shared.size) for v in sizes.values()) else max(
        s.nozzles, key=lambda z: z.size[0] * z.size[1] * z.size[2])
    width = region.x[1] - region.x[0]
    rows, row, row_w = [], [], 0.0
    for n in sorted(sizes, key=lambda n: -sizes[n][1]):
        w = sizes[n][0]
        if row and row_w + gap + w > width:
            rows.append(row)
            row, row_w = [], 0.0
        row_w += (gap if row else 0) + w
        row.append(n)
    rows.append(row)
    depth = sum(max(sizes[n][1] for n in r) for r in rows) + gap * (len(rows) - 1)
    total_w = max(sum(sizes[n][0] for n in r) + gap * (len(r) - 1) for r in rows)
    if total_w > width + 1e-6 or depth > region.y[1] - region.y[0] + 1e-6:
        return None
    x0 = (region.x[0] + region.x[1] - total_w) / 2
    y = (region.y[0] + region.y[1] + depth) / 2
    placed = {}
    for r in rows:
        y -= max(sizes[n][1] for n in r)
        x = x0
        for n in r:
            m = meshes[n].copy()
            m.apply_translation([x - m.bounds[0][0], y - m.bounds[0][1], -m.bounds[0][2]])
            placed[n] = m
            x += sizes[n][0] + gap
        y -= gap
    return placed


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
            nozzles = ", ".join(s.nozzles[i - 1].name for i in sl.filament_map if 0 < i <= len(s.nozzles)) or "unknown"
            lines += [f"- time {_fmt_time(sl.print_time_s or 0)}, filament {sl.filament_g or 0:.1f} g, nozzle {nozzles}",
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
