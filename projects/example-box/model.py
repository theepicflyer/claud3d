"""Parametric storage box with a friction-fit lid."""

import math

from build123d import Axis, Box, Cylinder, GeomType, Pos, RectangleRounded, Rot, chamfer, extrude, fillet

from cadlib.features import elephant_foot_chamfer

PARAMS = dict(
    inner_x=60.0,     # R1 usable cavity
    inner_y=40.0,
    inner_z=30.0,
    wall=2.0,         # R2
    floor=2.0,
    corner_r=4.0,     # outer vertical corner radius
    lid_t=2.0,
    lip_h=3.0,        # R3 short enough to pull off easily
    lip_wall=1.6,
    fit="snug",       # R3 lid fit class from printer.toml [fit]
    lead_in=0.4,      # chamfer on lip tip to guide the lid in
    scallop_w=14.0,   # R4 finger scallop in the +X wall's rim (chord width)
    scallop_d=2.2,    # R4 must stay above the lip's lead-in so only the lid shows through it
    scallop_round=0.8,
)

PRINT = dict(material="pla")

SECTIONS = {"assembly": ("x", 0.0)}


def rounded_block(x, y, z, r):
    return extrude(RectangleRounded(x, y, r), z)


def build(p, s):
    ox, oy = p.inner_x + 2 * p.wall, p.inner_y + 2 * p.wall
    height = p.floor + p.inner_z
    inner_r = max(p.corner_r - p.wall, 0.5)

    box = rounded_block(ox, oy, height, p.corner_r)
    box -= Pos(0, 0, p.floor) * rounded_block(p.inner_x, p.inner_y, p.inner_z + 1, inner_r)
    if p.scallop_d > 0:
        r = (p.scallop_w ** 2 / 4 + p.scallop_d ** 2) / (2 * p.scallop_d)
        cutter = Pos(ox / 2 - p.wall / 2, 0, height + r - p.scallop_d) * Rot(0, 90, 0) * Cylinder(r, p.wall + 2)
        before = set(box.edges())
        box -= cutter
        new_edges = [e for e in box.edges() if e not in before and e.geom_type != GeomType.LINE]
        if p.scallop_round > 0:
            box = fillet(new_edges, p.scallop_round)
    box = elephant_foot_chamfer(box)

    gap = s.clearance(p.fit) / 2
    lx, ly = p.inner_x - 2 * gap, p.inner_y - 2 * gap
    lip_r = max(inner_r - gap, 0.3)
    lid = rounded_block(ox, oy, p.lid_t, p.corner_r)
    lip = rounded_block(lx, ly, p.lip_h, lip_r)
    lip -= rounded_block(lx - 2 * p.lip_wall, ly - 2 * p.lip_wall, p.lip_h, max(lip_r - p.lip_wall, 0.3))
    lip = Pos(0, 0, p.lid_t) * lip
    tip = lip.faces().sort_by(Axis.Z)[-1]
    lip = chamfer(tip.outer_wire().edges(), p.lead_in)
    lid = elephant_foot_chamfer(lid + lip)

    return {"box": box, "lid": lid}


def assembly(p, s, parts):
    height = p.floor + p.inner_z
    lid = Pos(0, 0, height + p.lid_t) * (Rot(180, 0, 0) * parts["lid"])
    return {"box": parts["box"], "lid": lid}


def cavity(box):
    """Measured cavity (x, y, depth). X/Y come from a slice at mid-depth, below the finger scallop."""
    bb = box.bounding_box()
    floor_top = (box & Pos(bb.center().X, bb.center().Y, 0) * Box(0.5, 0.5, 1000)).bounding_box().max.Z
    slab = Pos(bb.center().X, bb.center().Y, (floor_top + bb.max.Z) / 2) * Box(bb.size.X, bb.size.Y, 0.5)
    inside = max((slab - box).solids(), key=lambda x: x.volume).bounding_box()
    return inside.size.X, inside.size.Y, bb.max.Z - floor_top


def check(p, s, parts):
    cx, cy, cz = cavity(parts["box"])
    yield cx >= p.inner_x - 0.01 and cy >= p.inner_y - 0.01, f"R1 cavity {cx:.2f} x {cy:.2f} mm (need {p.inner_x} x {p.inner_y})"
    yield cz >= p.inner_z - 0.01, f"R1 cavity depth {cz:.2f} mm (need {p.inner_z})"
    lip_sec = parts["lid"] & Pos(0, 0, p.lid_t + 1) * Box(1000, 1000, 0.1)
    lb = lip_sec.bounding_box()
    gx, gy = (cx - lb.size.X) / 2, (cy - lb.size.Y) / 2
    want = s.clearance(p.fit) / 2
    yield abs(gx - want) < 0.01 and abs(gy - want) < 0.01, \
        f"R3 lip gap per side {gx:.3f} / {gy:.3f} mm (target {want:.3f} for '{p.fit}' + {s.material.key})"
    ox = p.inner_x + 2 * p.wall
    rim = parts["box"] & Pos(ox / 2 - p.wall / 2, 0, 0) * Box(p.wall / 2, 0.5, 1000)
    dip = p.floor + p.inner_z - rim.bounding_box().max.Z
    yield abs(dip - p.scallop_d) < 0.05, f"R4 finger scallop {dip:.2f} mm deep at the +X rim (want {p.scallop_d})"
    # Looking into the scallop you should see the lid's lip, not the inside of the box: the lip's full-width
    # outer face (above its lead-in chamfer) must reach below the scallop's lowest point.
    asm = assembly(p, s, parts)
    lx = p.inner_x - s.clearance(p.fit)
    face = asm["lid"] & Pos(lx / 2 - 0.02, 0, 0) * Box(0.02, 0.5, 1000)
    lip_bottom = face.bounding_box().min.Z
    notch_bottom = p.floor + p.inner_z - dip
    yield notch_bottom >= lip_bottom + 0.2, \
        f"R4 scallop bottom z={notch_bottom:.2f} sits {notch_bottom - lip_bottom:.2f} mm above where the lip's face ends (z={lip_bottom:.2f})"


def features(p, s, parts):
    ox, oy = p.inner_x + 2 * p.wall, p.inner_y + 2 * p.wall
    height = p.floor + p.inner_z
    gap = s.clearance(p.fit) / 2
    ly = p.inner_y - 2 * gap
    c45 = math.cos(math.pi / 4)
    yield dict(name="floor", part="box", at=(0, 0, p.floor), normal=(0, 0, 1), params=["floor", "inner_x", "inner_y", "inner_z"],
               text="Bottom of the cavity. The cavity above it is inner_x x inner_y x inner_z.")
    yield dict(name="wall", part="box", at=(0, -oy / 2, height / 2), normal=(0, -1, 0), params=["wall"],
               text="Side walls, the same thickness all round.")
    yield dict(name="corner radius", part="box", normal=(c45, -c45, 0), params=["corner_r"],
               at=(ox / 2 - p.corner_r * (1 - c45), -oy / 2 + p.corner_r * (1 - c45), height / 2),
               text="Rounded vertical corners. The lid's outline uses the same radius so the two line up.")
    yield dict(name="rim", part="box", at=(-ox / 2 + p.wall / 2, 0, height), normal=(0, 0, 1), params=["wall"],
               text="Top edge of the walls. The lid plate rests on it.")
    yield dict(name="finger scallop", part="box", at=(ox / 2, 0, height - p.scallop_d), normal=(1, 0, 0),
               params=["scallop_w", "scallop_d", "scallop_round"], view="assembly",
               text="Shallow notch in the +X rim that exposes the lid's edge so a fingertip can lift it.")
    yield dict(name="elephant-foot chamfer", part="box", at=(0, -oy / 2 + 0.2, 0.2), normal=(0, -1, -1),
               text="0.4 mm chamfer on the edges touching the bed, so first-layer squish doesn't flare the outline.")
    yield dict(name="lid plate", part="lid", at=(0, oy / 2 - p.wall / 2, p.lid_t), normal=(0, 0, 1), params=["lid_t"],
               text="Printed face down on the bed; this face is the underside when the lid is on.")
    yield dict(name="lip", part="lid", at=(0, -ly / 2, p.lid_t + p.lip_h / 2), normal=(0, -1, 0),
               params=["lip_h", "lip_wall", "fit"],
               text=f"Friction-fit lip that drops into the box. {gap:.2f} mm gap per side ('{p.fit}' fit for {s.material.key}).")
    yield dict(name="lead-in chamfer", part="lid", at=(0, -ly / 2 + p.lead_in / 2, p.lid_t + p.lip_h - p.lead_in / 2),
               normal=(0, -1, 1), params=["lead_in"],
               text="Chamfer on the lip's tip that guides the lid into the opening.")
