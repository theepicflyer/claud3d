"""Bambu Studio integration: profile flattening, CLI slicing, opening projects in the GUI."""

from __future__ import annotations

import json
from itertools import islice
import re
import shutil
import subprocess
from dataclasses import dataclass, field
from functools import cache
from pathlib import Path

FLATPAK_ID = "com.bambulab.BambuStudio"
USER_CONFIG = Path.home() / ".var/app" / FLATPAK_ID / "config/BambuStudio"
KINDS = ("machine", "process", "filament")


def _flatpak_profiles_dir() -> Path | None:
    root = Path("/var/lib/flatpak/app") / FLATPAK_ID / "x86_64/stable/active/files/share/BambuStudio/profiles/BBL"
    if root.is_dir():
        return root
    hits = sorted(Path("/var/lib/flatpak/app", FLATPAK_ID).glob("x86_64/stable/*/files/share/BambuStudio/profiles/BBL"))
    return hits[-1] if hits else None


def bambu_command() -> list[str] | None:
    if shutil.which("bambu-studio"):
        return ["bambu-studio"]
    if shutil.which("flatpak") and _flatpak_profiles_dir():
        return ["flatpak", "run", "--command=bambu-studio", FLATPAK_ID]
    return None


@cache
def _index() -> dict[tuple[str, str], Path]:
    """Map (kind, profile name) -> json file, user profiles shadowing system ones."""
    idx: dict[tuple[str, str], Path] = {}
    dirs: list[Path] = []
    sys_dir = _flatpak_profiles_dir()
    if sys_dir:
        dirs.append(sys_dir)
    user_root = USER_CONFIG / "user"
    if user_root.is_dir():
        dirs += [d for d in sorted(user_root.iterdir()) if d.name == "default"]
        dirs += [d for d in sorted(user_root.iterdir()) if d.name != "default"]
    for base in dirs:
        for kind in KINDS:
            for f in (base / kind).rglob("*.json"):
                try:
                    name = json.loads(f.read_text()).get("name") or f.stem
                except (json.JSONDecodeError, UnicodeDecodeError):
                    continue
                idx[(kind, name)] = f
    return idx


def list_profiles(kind: str, pattern: str = "") -> list[str]:
    rx = re.compile(pattern, re.I)
    return sorted(n for (k, n) in _index() if k == kind and rx.search(n))


def resolve_profile(kind: str, name: str) -> dict:
    """Return a fully flattened profile (inherits chain merged, child wins)."""
    path = _index().get((kind, name))
    if path is None:
        raise KeyError(f"No {kind} profile named {name!r}. Try: uv run cad profiles {kind}")
    data = json.loads(path.read_text())
    parent = data.get("inherits")
    merged = resolve_profile(kind, parent) if parent else {}
    system_parent = merged.pop("_system_name", None)
    merged.update(data)
    merged.pop("inherits", None)
    if "user" in path.parts:
        merged["from"] = "User"
        merged["_system_name"] = system_parent
    else:
        merged["from"] = "system"
        merged["_system_name"] = name
    return merged


def write_flat_profiles(machine: str, process: str, filaments: list[str], out_dir: Path) -> tuple[Path, Path, list[Path]]:
    out_dir.mkdir(parents=True, exist_ok=True)

    def dump(kind: str, name: str) -> Path:
        data = resolve_profile(kind, name)
        system_name = data.pop("_system_name", None)
        # The CLI matches user presets to processes/filaments through their system parent.
        if data["from"] == "User" and system_name:
            data["inherits"] = system_name
        # The CLI checks compatibility by exact printer name, which user printer presets don't match.
        if kind != "machine" and data.get("compatible_printers") and machine not in data["compatible_printers"]:
            data["compatible_printers"] = [*data["compatible_printers"], machine]
        p = out_dir / f"{kind}_{re.sub(r'[^A-Za-z0-9.]+', '_', name)}.json"
        p.write_text(json.dumps(data, indent=1))
        return p

    m = dump("machine", machine)
    pr = dump("process", process)
    fs = [dump("filament", f) for f in filaments]
    return m, pr, fs


@dataclass
class SliceResult:
    ok: bool
    returncode: int
    message: str
    project_3mf: Path | None = None
    print_time_s: float | None = None
    filament_g: float | None = None
    feature_times: dict[str, float] = field(default_factory=dict)
    filament_map: list[int] = field(default_factory=list)  # 1-based extruder per filament
    warnings: list[str] = field(default_factory=list)
    log_tail: str = ""


def _parse_result_json(rj: dict, res: SliceResult) -> None:
    plates = rj.get("sliced_plates") or []
    if not plates:
        return
    res.print_time_s = sum(p.get("total_predication", 0) for p in plates)
    res.filament_g = sum(f.get("total_used_g", 0) for p in plates for f in p.get("filaments", []))
    res.feature_times = {}
    for p in plates:
        for k, v in (p.get("feature_type_times") or {}).items():
            res.feature_times[k] = res.feature_times.get(k, 0) + v
        if p.get("warning_message"):
            res.warnings.append(p["warning_message"])


def slice_files(
    models: list[Path],
    out_3mf: Path,
    machine: str,
    process: str,
    filament: str,
    settings_dir: Path,
    slice_plates: bool = True,
    overrides: dict | None = None,
    arrange: bool = False,
    timeout: int = 600,
) -> SliceResult:
    """Slice STL/3MF models with the Bambu Studio CLI and export a project 3MF.

    With arrange=False the models' own XY positions (bed coordinates) are kept. The CLI's auto-arrange only
    places objects where every nozzle reaches, so it rejects parts that need the full single-nozzle area.
    """
    cmd = bambu_command()
    if not cmd:
        return SliceResult(False, -1, "Bambu Studio not found")
    m, pr, fs = write_flat_profiles(machine, process, [filament], settings_dir)
    if overrides:
        data = json.loads(pr.read_text())
        data.update({k: str(v) for k, v in overrides.items()})
        pr.write_text(json.dumps(data, indent=1))
    out_3mf.parent.mkdir(parents=True, exist_ok=True)
    result_json = out_3mf.parent / "result.json"
    for stale in [out_3mf, result_json, *out_3mf.parent.glob("plate_*.gcode")]:
        stale.unlink(missing_ok=True)
    args = cmd + [
        "--load-settings", f"{m};{pr}",
        "--load-filaments", ";".join(str(f) for f in fs),
        "--arrange", "1" if arrange else "0",
        "--orient", "0",
        "--outputdir", str(out_3mf.parent),
        "--export-3mf", out_3mf.name,
    ]
    if slice_plates:
        args += ["--slice", "0"]
    args += [str(p) for p in models]
    proc = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    log = proc.stdout + proc.stderr
    msg, rj = "", {}
    if result_json.exists():
        try:
            rj = json.loads(result_json.read_text())
            msg = rj.get("error_string") or ""
        except json.JSONDecodeError:
            pass
    ok = proc.returncode == 0 and out_3mf.exists()
    if not msg:
        errs = [l for l in log.splitlines() if re.search(r"\[(error|fatal)\]", l)]
        msg = errs[-1] if errs else ("ok" if ok else f"exit code {proc.returncode}")
    res = SliceResult(ok, proc.returncode, msg, out_3mf if out_3mf.exists() else None,
                      log_tail="\n".join(log.splitlines()[-40:]))
    if ok and slice_plates:
        _parse_result_json(rj, res)
        for gcode in sorted(out_3mf.parent.glob("plate_*.gcode")):
            with gcode.open() as fh:
                for line in islice(fh, 5000):
                    if line.startswith("; filament_map = "):
                        res.filament_map = [int(x) for x in line.split("=", 1)[1].split(",")]
                        break
            break
    return res


def open_in_studio(path: Path) -> None:
    cmd = bambu_command()
    if not cmd:
        raise RuntimeError("Bambu Studio not found")
    subprocess.Popen(cmd + [str(path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
