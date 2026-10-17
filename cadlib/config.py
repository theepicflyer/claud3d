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
class Settings:
    printer_name: str
    machine_profile: str
    process_profile: str
    nozzle: float
    layer_height: float
    build_volume: tuple[float, float, float]
    design: dict
    fit: dict
    materials: dict[str, Material]
    material: Material
    slicer_overrides: dict = field(default_factory=dict)

    def clearance(self, kind: str = "slide") -> float:
        """Diametral clearance for a fit class, adjusted for the selected material."""
        return self.fit[kind] + self.material.clearance_extra


def load_settings(overrides: dict | None = None) -> Settings:
    """Load printer.toml; `overrides` is a model's PRINT dict (material, process, filament, slicer, build_volume)."""
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
    return Settings(
        printer_name=pr["name"],
        machine_profile=o.get("machine", pr["machine_profile"]),
        process_profile=o.get("process", pr["process_profile"]),
        nozzle=pr["nozzle"],
        layer_height=pr["layer_height"],
        build_volume=tuple(o.get("build_volume", pr["build_volume"])),
        design={**raw["design"], **o.get("design", {})},
        fit={**raw["fit"], **o.get("fit", {})},
        materials=mats,
        material=material,
        slicer_overrides=o.get("slicer", {}),
    )
