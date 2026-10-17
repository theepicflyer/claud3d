"""Printer, material and fit settings from printer.toml, with per-project overrides."""

from __future__ import annotations

import tomllib
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PRINTER_TOML = ROOT / "printer.toml"


@dataclass
class Material:
    key: str
    filament_profile: str
    clearance_extra: float = 0.0
    notes: str = ""


@dataclass
class Nozzle:
    """Region one nozzle can print, in slicer bed coordinates (mm)."""
    name: str
    drive: str
    x: tuple[float, float]
    y: tuple[float, float]
    height: float

    @property
    def size(self) -> tuple[float, float, float]:
        return (self.x[1] - self.x[0], self.y[1] - self.y[0], self.height)


NOZZLE_NAMES = ["left (main)", "right (auxiliary)"]


def nozzles_from_profile(machine: dict) -> list[Nozzle]:
    """Per-nozzle printable regions from a flattened Bambu machine preset."""
    areas = machine.get("extruder_printable_area") or [",".join(machine["printable_area"])]
    heights = machine.get("extruder_printable_height") or [machine["printable_height"]]
    drives = machine.get("extruder_type") or ["Direct Drive"] * len(areas)
    out = []
    for i, (area, h, drive) in enumerate(zip(areas, heights, drives, strict=True)):
        pts = [tuple(float(v) for v in p.strip().split("x")) for p in area.split(",")]
        xs, ys = [p[0] for p in pts], [p[1] for p in pts]
        name = NOZZLE_NAMES[i] if len(areas) == 2 else f"nozzle {i + 1}"
        out.append(Nozzle(name, drive.lower(), (min(xs), max(xs)), (min(ys), max(ys)), float(h)))
    return out


@dataclass
class Settings:
    printer_name: str
    machine_profile: str
    process_profile: str
    nozzle: float
    layer_height: float
    nozzles: list[Nozzle]
    design: dict
    fit: dict
    materials: dict[str, Material]
    material: Material
    slicer_overrides: dict = field(default_factory=dict)

    @property
    def shared(self) -> Nozzle:
        """Region every nozzle can reach."""
        n = self.nozzles
        return Nozzle("all nozzles", "", (max(z.x[0] for z in n), min(z.x[1] for z in n)),
                      (max(z.y[0] for z in n), min(z.y[1] for z in n)), min(z.height for z in n))

    @property
    def build_volume(self) -> tuple[float, float, float]:
        """Largest single-nozzle volume (x, y, z)."""
        return max((z.size for z in self.nozzles), key=lambda v: v[0] * v[1] * v[2])

    def clearance(self, kind: str = "slide") -> float:
        """Diametral clearance for a fit class, adjusted for the selected material."""
        return self.fit[kind] + self.material.clearance_extra


def load_settings(overrides: dict | None = None) -> Settings:
    """Load printer.toml; `overrides` is a model's PRINT dict (material, process, filament, machine, slicer, design, fit)."""
    from .bambu import resolve_profile

    raw = tomllib.loads(PRINTER_TOML.read_text())
    o = overrides or {}
    pr = raw["printer"]
    mats = {
        k: Material(k, v["filament_profile"], v.get("clearance_extra", 0.0), v.get("notes", ""))
        for k, v in raw["materials"].items()
        if isinstance(v, dict)
    }
    mat_key = o.get("material", raw["materials"]["default"])
    if mat_key not in mats:
        raise KeyError(f"Unknown material {mat_key!r}; choose from {sorted(mats)} or add it to printer.toml")
    material = mats[mat_key]
    if "filament" in o:
        material = Material(material.key, o["filament"], material.clearance_extra, material.notes)
    machine = o.get("machine", pr["machine_profile"])
    return Settings(
        printer_name=pr["name"],
        machine_profile=machine,
        process_profile=o.get("process", pr["process_profile"]),
        nozzle=pr["nozzle"],
        layer_height=pr["layer_height"],
        nozzles=nozzles_from_profile(resolve_profile("machine", machine)),
        design={**raw["design"], **o.get("design", {})},
        fit={**raw["fit"], **o.get("fit", {})},
        materials=mats,
        material=material,
        slicer_overrides=o.get("slicer", {}),
    )
