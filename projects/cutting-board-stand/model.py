"""Drying stand for two cutting boards and a baking sheet, standing in the counter nook right of the sink.

Two side panels with a sweeping top edge, tall at the back and low at the front, with three slanted slots cut
into them, joined by low rounded rails. It sits on stick-on silicone bumpers.

Modelled in the counter frame, which is also the print orientation: X along the back wall from the sink's right
edge, Y out from the foot of the wall curve, Z up. Items lean back toward the wall (-Y) by `lean` degrees.
"""

import math

import numpy as np
from build123d import (Align, Axis, Box, Cylinder, Face, Plane, Pos, Rectangle, Rot, Vector, Wire,
                       extrude, fillet)

PARAMS = dict(
    # Items (user's measurements). Slots from the back: big board, baking sheet, small board.
    big_w=355.0, big_h=355.0, big_t=20.0,
    sheet_w=280.0, sheet_h=370.0, sheet_t=25.0,   # stands on its 280 mm edge; t is the rim height
    small_w=235.0, small_h=235.0, small_t=20.0,
    # Counter nook (R3, R4)
    zone_x=300.0, zone_y=210.0, strip_y=80.0, wall_r=13.0,
    # Layout
    lean=8.0,          # degrees back from vertical; 8 keeps the big board inside the 80 mm strip
    wall_gap=3.0,      # big board's top edge stays this far in front of the wall curve
    air_gap=30.0,      # R5 face-to-face air between neighbouring items
    x_center=150.0,    # stand centre along the wall
    big_right=297.0,   # big board's right edge (assembly only); its left 58 mm hangs over the strip
    # Side panels
    panel_span=185.0,  # centre to centre; inside the small board's width with 25 mm to spare each side
    panel_w=12.0,
    edge_r=4.5,        # rounding on every exposed panel edge
    back_y=5.0,        # rear end of the panels
    nose=14.0,         # front finger's length, in front of the small board's slot
    top_back=135.0,    # finger heights follow a sweep from this at the back end
    top_front=42.0,    # height at the front end
    top_bulge=15.0,    # how far the sweep bows above a straight line
    # Slots
    floor_z=22.0,      # height of each item's back-bottom corner (slot floor) above the panel bottom
    slot_tilt=3.0,     # slot back wall stands this much more upright than the item, so the item touches only its top
    front_clear=5.0,   # gap in front of the item at the slot floor; the front wall is vertical (R2)
    floor_r=4.0,       # inside corners of the slot floor (smaller where the item's corner is close)
    rest_min=18.0,     # middle fingers are at least this wide at the top; matches the small board's rest
    # Rails
    rail_w=10.0, rail_h=12.0,
    joint_r=3.0,       # rounds the inside corners where rails meet the panels
)

PRINT = dict(material="petg", slicer={"curr_bed_type": "Textured PEI Plate"})

SECTIONS = {"stand": ("x", 57.5), "assembly": ("x", 57.5)}

ITEMS = ("big", "sheet", "small")
WOOD = 0.75e-3       # g/mm^3, beech-ish
SHEET_G = 1000.0     # baking sheet mass, generous for steel
PETG = 1.27e-3
STAND_FILL = 0.3     # effective solid fraction of the printed stand


def _uv(p):
    a = math.radians(p.lean)
    return np.array([-math.sin(a), math.cos(a)]), np.array([math.cos(a), math.sin(a)])


def _dir(deg):
    """Unit (y, z) direction leaning back from vertical by deg."""
    a = math.radians(deg)
    return np.array([-math.sin(a), math.cos(a)])


def item_dims(p, k):
    return getattr(p, f"{k}_w"), getattr(p, f"{k}_h"), getattr(p, f"{k}_t")


def slot_corners(p):
    """Back-bottom corner (y, z) of each item, back to front."""
    a = math.radians(p.lean)
    y = p.wall_gap + p.big_h * math.sin(a)
    out = {}
    for k in ITEMS:
        out[k] = np.array([y, p.floor_z])
        y += (item_dims(p, k)[2] + p.air_gap) / math.cos(a)
    return out


def panel_xs(p):
    return (p.x_center - p.panel_span / 2, p.x_center + p.panel_span / 2)


def extent(p):
    u, n = _uv(p)
    B = slot_corners(p)["small"]
    front = B + (p.small_t + p.front_clear) * n
    return p.back_y, front[0] + p.nose


def top_arc(p):
    """Centre and radius of the sweep the finger heights follow, from (back_y, top_back) to (front, top_front)."""
    y0, y1 = extent(p)
    e0, e1 = np.array([y0, p.top_back]), np.array([y1, p.top_front])
    L = np.linalg.norm(e1 - e0)
    d = (e1 - e0) / L
    m = np.array([-d[1], d[0]])
    R = (L * L / 4 + p.top_bulge ** 2) / (2 * p.top_bulge)
    return (e0 + e1) / 2 - (R - p.top_bulge) * m, R


def top_z(p, y):
    A, R = top_arc(p)
    return A[1] + math.sqrt(R * R - (y - A[0]) ** 2)


def _wall_at(P, w, z):
    return P + (z - P[1]) / w[1] * w


def _solve_slot(p, k, back_y, r):
    """Place slot k's back wall so a cap corner of radius r on the finger behind it just touches the item."""
    u, n = _uv(p)
    B = slot_corners(p)[k]
    w = _dir(p.lean - p.slot_tilt)
    nw = np.array([w[1], -w[0]])           # wall normal, pointing forward into the slot
    H = top_z(p, (back_y + B[0]) / 2)
    lo, hi = 0.0, 40.0
    for _ in range(60):
        o = (lo + hi) / 2
        Cc = _wall_at(B - o * n - r * nw, w, H - r)
        if np.dot(n, Cc - B) + r > 0:
            lo = o
        else:
            hi = o
    P = B - o * n
    return dict(P=P, w=w, o=o, H=H, r=r, rest=Cc + r * n, back_y=back_y)


def layout(p):
    """Slots back to front, each with the finger behind it. A finger's front edge is the slot's back wall; its back
    edge is the previous slot's front wall (or the panel's back end). Its cap is two corner radii of almost half
    its top width, so it reads as one round end. A front wall leans back just enough to keep the finger in front
    of it at least rest_min wide at the top; otherwise it is vertical."""
    u, n = _uv(p)
    out = {}
    back_y = extent(p)[0]
    prev = None
    for k in ITEMS:
        lean = 0.0
        for _ in range(2):
            r = 6.0
            for _ in range(6):
                sl = _solve_slot(p, k, back_y, r)
                top_back = back_y - (sl["H"] - prev["C0"][1]) * math.tan(lean) if prev else back_y
                width = _wall_at(sl["P"], sl["w"], sl["H"])[0] - top_back
                r = 0.49 * width
            if prev is None or width >= p.rest_min - 0.2:
                break
            lean = math.atan((p.rest_min - width) / (sl["H"] - prev["C0"][1]) + math.tan(lean))
        if prev:
            prev["front_lean"] = math.degrees(lean)
        sl["C0"] = slot_corners(p)[k] + (item_dims(p, k)[2] + p.front_clear) * n
        sl["width"], sl["top_back"], sl["front_lean"] = width, top_back, 0.0
        out[k] = prev = sl
        back_y = sl["C0"][0]
    return out


def _vertex_near(face, pt):
    return min(face.vertices(), key=lambda v: (v.X - pt[0]) ** 2 + (v.Y - pt[1]) ** 2)


def panel_profile(p):
    """Side outline, drawn in XY as (y, z) and placed on Plane.YZ by the caller: flat bottom, four leaning fingers
    with round caps whose heights follow the sweep, and the three slots between them."""
    y0, y1 = extent(p)
    lay = layout(p)
    pts = [(y0, 0), (y1, 0)]
    nose_back = lay["small"]["C0"][0]
    Hn = top_z(p, (nose_back + y1) / 2)
    pts += [(y1, Hn), (nose_back, Hn)]
    corners = [((y1, Hn), 0.49 * (y1 - nose_back)), ((nose_back, Hn), 0.49 * (y1 - nose_back))]
    for k in reversed(ITEMS):
        sl = lay[k]
        P, C0, H = sl["P"], sl["C0"], sl["H"]
        W = _wall_at(P, sl["w"], H)
        pts += [tuple(C0), tuple(P), tuple(W), (sl["top_back"], H)]
        corners += [(W, sl["r"]), ((sl["top_back"], H), sl["r"]),
                    (P, min(p.floor_r, 0.9 * sl["o"] / 1.05)), (C0, min(p.floor_r, 0.85 * p.front_clear))]
    face = Face(Wire.make_polygon([Vector(float(a), float(b)) for a, b in pts], close=True))
    for pt, r in corners:
        face = face.fillet_2d(r, [_vertex_near(face, pt)])
    return face


def build(p, s):
    face = panel_profile(p)
    panel = extrude(Plane.YZ * face, p.panel_w, dir=(1, 0, 0))
    side = [e for e in panel.edges() if e.bounding_box().size.X < 1e-6 and e.bounding_box().max.Z > 1e-3]
    panel = fillet(side, p.edge_r)
    xa, xb = panel_xs(p)
    stand = Pos(xa - p.panel_w / 2, 0, 0) * panel + Pos(xb - p.panel_w / 2, 0, 0) * panel

    # Base frame: both panels' footprints plus three rails, drawn in plan with every corner rounded (outer corners
    # match the panels' rounded ends; inner corners keep the footprint from lifting), then rounded along its top.
    # The end rails are flush with the panel ends, so the back and front are one flat face.
    y0, y1 = extent(p)
    ym = rail_ys(p)[1]
    h = p.panel_w / 2
    rect = lambda xl, xh, yl, yh: Pos((xl + xh) / 2, (yl + yh) / 2) * Rectangle(xh - xl, yh - yl)
    plan = (rect(xa - h, xb + h, y0, y1) - rect(xa + h, xb - h, y0 + p.rail_w, ym - p.rail_w / 2)
            - rect(xa + h, xb - h, ym + p.rail_w / 2, y1 - p.rail_w)).faces()[0]
    plan = plan.fillet_2d(p.edge_r, [v for v in plan.vertices() if abs(abs(v.X - p.x_center) - p.panel_span / 2 - h) < 1e-6])
    plan = plan.fillet_2d(p.joint_r, [v for v in plan.vertices() if abs(abs(v.X - p.x_center) - p.panel_span / 2 + h) < 1e-6])
    frame = extrude(plan, p.rail_h)
    frame = fillet(frame.edges().group_by(Axis.Z)[-1], p.edge_r - 0.1)
    stand += frame
    return {"stand": stand}


def rail_ys(p):
    """Back, middle and front rails."""
    y0, y1 = extent(p)
    lay = layout(p)
    # End rails are flush with the panel ends
    return [y0 + p.rail_w / 2, (lay["big"]["C0"][0] + lay["sheet"]["P"][0]) / 2, y1 - p.rail_w / 2]


def item_solid(p, k, slot=None):
    """Item k standing in its own slot, or in `slot` (the small board also fits the sheet's slot)."""
    w, h, t = item_dims(p, k)
    B = slot_corners(p)[slot or k]
    u, n = _uv(p)
    x0 = p.big_right - w if k == "big" else p.x_center - w / 2
    # 0.05 mm off the floor and back wall so the interference check sees contact, not overlap
    B = B + 0.05 * n + 0.05 * u
    if k == "sheet":
        body = Box(w, t, h, align=Align.MIN) - Pos(1, 1, 1) * Box(w - 2, t, h - 2, align=Align.MIN)
    else:
        body = Box(w, t, h, align=Align.MIN)
        body = body.fillet(15, body.edges().filter_by(Axis.Y))
    return Pos(x0, B[0], B[1]) * Rot(p.lean, 0, 0) * body


def kitchen(p):
    """Counter top around the nook plus a short stub of the back wall with its curve, for context only."""
    top = Pos(0, 0, -10) * Box(p.zone_x, p.zone_y + 40, 10, align=Align.MIN)
    strip = Pos(-150, 0, -10) * Box(150, p.strip_y, 10, align=Align.MIN)
    wall = Pos(-150, -p.wall_r - 15, -10) * Box(p.zone_x + 150, 15, 50, align=Align.MIN)
    cove = Pos(-150, -p.wall_r, 0) * Box(p.zone_x + 150, p.wall_r, p.wall_r, align=Align.MIN)
    cove -= Pos(p.zone_x / 2 - 75, 0, p.wall_r) * Rot(0, 90, 0) * Cylinder(p.wall_r, p.zone_x + 152)
    return top + strip + wall + cove


def assembly(p, s, parts):
    out = {"stand": parts["stand"]}
    out.update({f"{k}": item_solid(p, k) for k in ITEMS})
    out["counter"] = kitchen(p)
    return out


# ---------------------------------------------------------------- checks

def _hull(pts):
    pts = sorted(set(map(tuple, pts)))
    def half(seq):
        h = []
        for q in seq:
            while len(h) >= 2 and (h[-1][0] - h[-2][0]) * (q[1] - h[-2][1]) - (h[-1][1] - h[-2][1]) * (q[0] - h[-2][0]) <= 0:
                h.pop()
            h.append(q)
        return h
    lo, hi = half(pts), half(reversed(pts))
    return lo[:-1] + hi[:-1]


def _inside_margin(hull, q):
    """Signed distance from q to the hull boundary; positive inside (hull is counter-clockwise)."""
    d = math.inf
    for i in range(len(hull)):
        a, b = np.array(hull[i]), np.array(hull[(i + 1) % len(hull)])
        e = b - a
        cross = e[0] * (q[1] - a[1]) - e[1] * (q[0] - a[0])
        d = min(d, cross / np.linalg.norm(e))
    return d


def _projected_area(shape, direction):
    if shape is None or not shape.solids():
        return 0.0
    d = np.array(direction)
    total = 0.0
    for f in shape.faces():
        nrm = f.normal_at()
        total += max(0.0, float(np.dot([nrm.X, nrm.Y, nrm.Z], d))) * f.area
    return total


def _overlap(a, b):
    c = a & b
    return sum(x.volume for x in c.solids()) if c is not None else 0.0


def check(p, s, parts):
    asm = assembly(p, s, parts)
    stand = asm["stand"]
    u, n = _uv(p)
    n3, u3 = np.array([0, n[0], n[1]]), np.array([0, u[0], u[1]])

    # R1: each item sits on both panels (contact, no overlap)
    left = stand & Box(p.x_center, 1000, 1000, align=(Align.MIN, Align.CENTER, Align.MIN))
    right = stand - left
    cases = [(k, asm[k]) for k in ITEMS] + [("small (in sheet slot)", item_solid(p, "small", "sheet"))]
    for k, it in cases:
        dl, dr = it.distance_to(left), it.distance_to(right)
        ov = _overlap(it, stand)
        yield dl < 0.2 and dr < 0.2 and ov < 1e-3, \
            f"R1 {k} rests on both panels (gaps {dl:.2f} / {dr:.2f} mm, overlap {ov:.3f} mm^3)"

    # R2: lifting each item straight up, or along its own lean, never hits the stand
    for k, it in cases:
        worst = 0.0
        for d in (0.5, 1, 2, 3, 5, 8, 12, 20, 40, 80, 150):
            for vec in ((0, 0, d), tuple(d * u3)):
                worst = max(worst, _overlap(Pos(*vec) * it, stand))
        yield worst < 1e-3, f"R2 {k} lifts out vertically and along its lean (max overlap {worst:.3f} mm^3)"

    # R3 / R4: footprint inside the nook; nothing behind the wall curve
    for name in ("stand", "sheet", "small"):
        bb = asm[name].bounding_box()
        ok = bb.min.X >= 0 and bb.max.X <= p.zone_x and bb.min.Y >= 0 and bb.max.Y <= p.zone_y
        yield ok, f"R3 {name} spans x {bb.min.X:.1f}..{bb.max.X:.1f}, y {bb.min.Y:.1f}..{bb.max.Y:.1f} (zone 0..{p.zone_x:.0f} x 0..{p.zone_y:.0f})"
    bb = asm["big"].bounding_box()
    ok = bb.max.X <= p.zone_x and bb.min.Y >= 0 and bb.max.Y <= p.strip_y
    yield ok, f"R3 big board spans x {bb.min.X:.1f}..{bb.max.X:.1f}, y {bb.min.Y:.1f}..{bb.max.Y:.1f} (left overhang must stay in y 0..{p.strip_y:.0f})"
    ymin = min(asm[k].bounding_box().min.Y for k in ("stand",) + ITEMS)
    yield ymin >= 0, f"R4 nearest approach to the wall curve y={ymin:.1f} mm"

    # R5: air between items, and stand area within 5 mm of each face
    for a, b in zip(ITEMS, ITEMS[1:]):
        gap = asm[a].distance_to(asm[b])
        yield gap >= 25, f"R5 {a} to {b} face gap {gap:.1f} mm (need >= 25)"
    for k in ITEMS:
        w, h, t = item_dims(p, k)
        B = slot_corners(p)[k]
        x0 = p.big_right - w if k == "big" else p.x_center - w / 2
        for side, off, sign in (("back", -5.0, -1), ("front", t, 1)):
            slab = Pos(x0, B[0], B[1]) * Rot(p.lean, 0, 0) * Pos(0, off, 0) * Box(w, 5, h, align=Align.MIN)
            area = _projected_area(stand & slab, -sign * n3)
            frac = area / (w * h)
            yield frac <= 0.05, f"R5 {k} {side} face: {area:.0f} mm^2 of stand within 5 mm ({100 * frac:.1f}%, max 5%)"

    # R6: combined centre of mass stays inside the footprint (bumpers go under the panel ends)
    foot = stand & Box(1000, 1000, 0.5, align=(Align.CENTER, Align.CENTER, Align.MIN))
    pts = [(v.X, v.Y) for v in foot.vertices()]
    hull = _hull(pts)
    if _inside_margin(hull, np.mean(pts, axis=0)) < 0:
        hull = hull[::-1]
    masses = {"stand": (stand.volume * PETG * STAND_FILL, stand.center())}
    for k in ITEMS:
        it = asm[k]
        masses[k] = (SHEET_G if k == "sheet" else it.volume * WOOD, it.center())
    worst = (math.inf, "")
    for mask in range(8):
        load = ["stand"] + [k for i, k in enumerate(ITEMS) if mask >> i & 1]
        m = sum(masses[x][0] for x in load)
        cx = sum(masses[x][0] * masses[x][1].X for x in load) / m
        cy = sum(masses[x][0] * masses[x][1].Y for x in load) / m
        d = _inside_margin(hull, (cx, cy))
        if d < worst[0]:
            worst = (d, "+".join(load))
    yield worst[0] >= 20, f"R6 worst centre-of-mass margin {worst[0]:.1f} mm inside the footprint ({worst[1]}; need >= 20)"

    # R7: flat underside for the bumpers; no level up-facing patch wide enough to pool water
    bottoms = [f for f in stand.faces() if f.normal_at().Z < -0.99]
    low = max(f.center().Z for f in bottoms)
    yield low < 0.01, f"R7 every downward flat face is on the bed (highest at z={low:.2f}), so the bumpers sit flush"
    flats = [2 * f.area / sum(e.length for e in f.edges()) for f in stand.faces()
             if f.geom_type.name == "PLANE" and f.normal_at().Z > 0.99]
    widest = max(flats, default=0.0)
    yield widest <= 3.0, f"R7 widest level up-facing patch {widest:.2f} mm across (max 3, so water runs off)"


# ---------------------------------------------------------------- callouts

def features(p, s, parts):
    u, n = _uv(p)
    xa, xb = panel_xs(p)
    y0, y1 = extent(p)
    lay = layout(p)
    names = {"big": "big board", "sheet": "baking sheet", "small": "small board"}
    yield dict(name="side panel", part="stand", at=(xa - p.panel_w / 2, (y0 + y1) / 2, p.floor_z / 2),
               normal=(-1, 0, 0), params=["panel_span", "panel_w", "edge_r"],
               text="One of the two side panels. The items pass through its slots; only the slot edges touch them.")
    yield dict(name="finger heights", part="stand", at=(xa, y0 + 15, top_z(p, y0 + 15)), normal=(0, -0.3, 1),
               params=["top_back", "top_front", "top_bulge"],
               text="The fingers' heights follow one curve from the tall back down to the low front.")
    yield dict(name="front finger", part="stand", at=(xa, y1, top_z(p, y1) * 0.6), normal=(0, 1, 0.3),
               params=["nose"], text="Front end of the panel, in front of the small board's slot.")
    for k in ITEMS:
        sl = lay[k]
        yield dict(name=f"{names[k]} rest", part="stand", at=(xb, sl["rest"][0], sl["rest"][1]), normal=(0, 0.5, 1),
                   params=["slot_tilt", "lean"],
                   text=f"Round cap of the finger behind the {names[k]}. The item's back face leans on it; below it the finger "
                        f"stands {p.slot_tilt:.0f} degrees more upright than the item, so the gap opens downward.")
        fl = (sl["P"] + sl["C0"]) / 2
        yield dict(name=f"{names[k]} slot floor", part="stand", at=(xa, fl[0], fl[1]), normal=(0, 0.3, 1),
                   params=["floor_z", "floor_r"], text=f"The {names[k]}'s bottom edge sits here, tilted to match the lean.")
        yield dict(name=f"{names[k]} slot front", part="stand", at=(xa, sl["C0"][0], sl["C0"][1] + 15), normal=(0, -1, 0.3),
                   params=["front_clear"],
                   text=f"Vertical front wall of the slot, {p.front_clear:.0f} mm clear of the item at the floor, so the item lifts straight up.")
    yield dict(name="cross rail", part="stand", at=(p.x_center, rail_ys(p)[1], p.rail_h), normal=(0, 0.3, 1),
               params=["rail_w", "rail_h", "joint_r"], text="Round-topped rail tying the two panels together, with rounded inside corners where it meets them. The back and front rails are flush with the panel ends.")
