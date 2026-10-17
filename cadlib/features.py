"""Print-oriented feature helpers. Dimensions in mm.

For catalogued fasteners (screws, nuts, heat-set inserts, threads) prefer bd_warehouse:
    from bd_warehouse.fastener import ClearanceHole, InsertHole, HeatSetNut, SocketHeadCapScrew, HexNut
    from bd_warehouse.thread import IsoThread
"""

from __future__ import annotations

import math

from build123d import (
    BuildSketch,
    Circle,
    Edge,
    Locations,
    Mode,
    Part,
    Polygon,
    Rectangle,
    RegularPolygon,
    Sketch,
    chamfer,
)

# Printed clearance hole diameters for metric screws (screw passes freely, vertical hole, 0.4 nozzle).
SCREW_CLEARANCE = {"M2": 2.4, "M2.5": 2.9, "M3": 3.4, "M4": 4.5, "M5": 5.5, "M6": 6.6, "M8": 8.8}
# Socket head cap screw head diameter + clearance, for counterbores.
SHCS_HEAD = {"M2": 4.4, "M2.5": 5.0, "M3": 6.0, "M4": 7.6, "M5": 9.0, "M6": 10.6, "M8": 13.6}
# Hex nut width across flats (ISO 4032), add clearance for traps.
NUT_AF = {"M2": 4.0, "M2.5": 5.0, "M3": 5.5, "M4": 7.0, "M5": 8.0, "M6": 10.0, "M8": 13.0}
NUT_THICK = {"M2": 1.6, "M2.5": 2.0, "M3": 2.4, "M4": 3.2, "M5": 4.7, "M6": 5.2, "M8": 6.8}
# Self-tapping into plastic: pilot hole diameters for thread-forming screws.
PILOT_HOLE = {"M2": 1.7, "M2.5": 2.2, "M3": 2.6, "M4": 3.5, "M5": 4.4}


def hex_nut_trap(size: str, clearance: float = 0.2) -> Sketch:
    """Hexagon sketch sized for a captive nut (flats + clearance)."""
    af = NUT_AF[size] + clearance
    with BuildSketch() as sk:
        RegularPolygon(af / 2, 6, major_radius=False)
    return sk.sketch


def teardrop(diameter: float, angle: float = 45, flat_top: bool = False) -> Sketch:
    """Teardrop profile for horizontal holes; point toward +Y (sketch up), so place +Y as world +Z.

    Avoids a flat overhanging ceiling on holes printed sideways. flat_top truncates the tip at the circle's top.
    """
    r = diameter / 2
    a = math.radians(angle)
    tip = r / math.sin(a)
    tx, ty = r * math.cos(a), r * math.sin(a)
    with BuildSketch() as sk:
        Circle(r)
        Polygon((-tx, ty), (0, tip), (tx, ty), (0, 0), align=None)
        if flat_top:
            with Locations((0, r + tip)):
                Rectangle(diameter * 2, tip * 2, mode=Mode.SUBTRACT)
    return sk.sketch


def bottom_edges(part: Part, tol: float = 1e-3) -> list[Edge]:
    """Edges lying on the bed plane (lowest Z)."""
    z0 = part.bounding_box().min.Z
    return [e for e in part.edges() if abs(e.bounding_box().max.Z - z0) < tol]


def elephant_foot_chamfer(part: Part, size: float = 0.4) -> Part:
    """Chamfer bed-contact edges so first-layer squish doesn't flare the outline."""
    return chamfer(bottom_edges(part), size)


def fit_diameter(nominal: float, clearance: float, hole: bool = True) -> float:
    """Diameter for a printed hole (grown) or pin (shrunk) given total diametral clearance."""
    return nominal + clearance if hole else nominal - clearance
