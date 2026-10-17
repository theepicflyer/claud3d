"""Drying stand for two cutting boards and a baking sheet, standing in the counter nook right of the sink.

Modelled in the counter frame, which is also the print orientation: X along the back wall from the sink's right
edge, Y out from the foot of the wall curve, Z up. Items lean back toward the wall (-Y) by `lean` degrees.
"""

import math

import numpy as np
from build123d import (Align, Axis, Box, Cylinder, Plane, Polygon, Pos, Rot, chamfer, extrude)

from cadlib.render import tessellate

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
    # Combs (the two side frames)
    comb_span=185.0,   # centre to centre; inside the small board's width with 25 mm to spare each side
    comb_w=10.0,
    back_y=5.0,        # rear end of the base
    front_pad=2.0,     # base runs this far past the front lip
    beam_h=14.0,
    ledge_z=22.0,      # height of the items' back-bottom corners above the counter
    prong_root=14.0,   # prong depth at the ledge, tapering to prong_tip
    prong_tip=8.0,
    prong_big=130.0, prong_sheet=130.0, prong_small=100.0,
    lip_t=6.0, lip_h=6.0,
    lip_clear=1.5,     # R2 > lip_h * tan(lean) so a vertical lift clears the lip
    ridge=1.0,         # flat left on top of chamfered ridges; contact strip on prongs and ledges
    # Base rails and feet (R7)
    rail_w=10.0,
    air_h=6.0,         # arch height between feet
    bridge=12.0,       # flat top of each arch
    pad=8.0,           # foot length along the beam
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


def item_dims(p, k):
    return getattr(p, f"{k}_w"), getattr(p, f"{k}_h"), getattr(p, f"{k}_t")


def slot_corners(p):
    """Back-bottom corner (y, z) of each item, back to front."""
    a = math.radians(p.lean)
    y = p.wall_gap + p.big_h * math.sin(a)
    out = {}
    for k in ITEMS:
        out[k] = np.array([y, p.ledge_z])
        y += (item_dims(p, k)[2] + p.air_gap) / math.cos(a)
    return out


def prong_len(p, k):
    return getattr(p, f"prong_{k}")


def comb_xs(p):
    return (p.x_center - p.comb_span / 2, p.x_center + p.comb_span / 2)


def arch_openings(a, b, p):
    """Openings (start, end) along a beam from a to b, leaving feet of length `pad` at both ends."""
    span = p.bridge + 2 * p.air_h
    length = b - a
    n = max(1, math.ceil((length - p.pad) / (span + p.pad)))
    w = (length - (n + 1) * p.pad) / n
    return [(a + p.pad + i * (w + p.pad), a + p.pad + i * (w + p.pad) + w) for i in range(n)]


def arch_cut(o0, o1, h):
    """Trapezoid with 45 degree flanks, slightly below z=0 so the cut is clean."""
    return [(o0 - 1, -1), (o1 + 1, -1), (o1 - h, h), (o0 + h, h)]


def base_extent(p):
    u, n = _uv(p)
    B = slot_corners(p)["small"]
    front = B + (p.small_t + p.lip_clear + p.lip_t) * n
    return p.back_y, front[0] + p.front_pad


def _prism(pts, w):
    return extrude(Plane.YZ * Polygon(*[tuple(q) for q in pts], align=None), w, dir=(1, 0, 0))


def _ridge(solid, dirs, c):
    """Chamfer both side edges of the faces whose outward normal (y, z) is in `dirs`, leaving a narrow ridge."""
    edges = []
    for f in solid.faces():
        nn = f.normal_at()
        if any(abs(nn.Y - d[0]) < 1e-3 and abs(nn.Z - d[1]) < 1e-3 for d in dirs):
            edges += [e for e in f.edges() if e.bounding_box().size.X < 1e-6]
    return chamfer(edges, c)


def comb(p):
    """One side comb, x from 0 to comb_w. Built from simple prisms, each ridged on its exposed upper faces
    before they are fused, because chamfering the fused outline fails."""
    u, n = _uv(p)
    up = (0.0, 1.0)
    c = (p.comb_w - p.ridge) / 2
    w = p.comb_w
    y0, y1 = base_extent(p)
    e = p.beam_h - 0.5
    pieces, covered = [], []
    for k, B in slot_corners(p).items():
        t = item_dims(p, k)[2]
        back = B - p.prong_root * n
        C0 = B + (t + p.lip_clear) * n
        C1 = C0 + p.lip_t * n
        L = prong_len(p, k)
        tip = B + L * u - p.prong_tip * n
        covered.append((back[0], C1[0]))
        pieces += [
            _prism([(back[0], e), back, B, (B[0], e)], w),
            _ridge(_prism([(B[0], e), B, C0, (C0[0], e)], w), [tuple(u)], c),
            _prism([(C0[0], e), C0, C1, (C1[0], e)], w),
            _ridge(_prism([back - 0.5 * u, B - 0.5 * u, B + L * u, tip], w), [tuple(n), tuple(u)], c),
            _ridge(_prism([C0 - 0.5 * u, C1 - 0.5 * u, C1 + p.lip_h * u, C0 + p.lip_h * u], w), [tuple(n), tuple(u)], c),
        ]
    edges = [y0] + [v for pair in covered for v in pair] + [y1]
    for i, (ya, yb) in enumerate(zip(edges, edges[1:])):
        seg = _prism([(ya, 0), (yb, 0), (yb, p.beam_h), (ya, p.beam_h)], w)
        pieces.append(seg if i % 2 else _ridge(seg, [up], c))
    part = pieces[0]
    for q in pieces[1:]:
        part += q
    for o0, o1 in arch_openings(y0, y1, p):
        part -= _prism(arch_cut(o0, o1, p.air_h), w)
    return part.clean()


def build(p, s):
    comb0 = comb(p)
    xa, xb = comb_xs(p)
    stand = Pos(xa - p.comb_w / 2, 0, 0) * comb0 + Pos(xb - p.comb_w / 2, 0, 0) * comb0

    y0, y1 = base_extent(p)
    rails = [y0 + p.rail_w / 2, (y0 + y1) / 2, y1 - p.rail_w / 2]
    rail_prof = Polygon((xa, 0), (xb, 0), (xb, p.beam_h), (xa, p.beam_h), align=None)
    for o0, o1 in arch_openings(xa, xb, p):
        rail_prof -= Polygon(*arch_cut(o0, o1, p.air_h), align=None)
    rail0 = extrude(Plane.XZ * rail_prof, p.rail_w / 2, both=True)
    top = rail0.edges().filter_by(Axis.X).group_by(Axis.Z)[-1]
    rail0 = chamfer(top, (p.rail_w - p.ridge) / 2)
    for ry in rails:
        stand += Pos(0, ry, 0) * rail0
    # Elephant-foot chamfer only where a wall meets the bed; on an arch flank it would make a steep overhang.
    feet = {e for fc in stand.faces() if abs(fc.normal_at().Z) < 1e-3 for e in fc.edges()
            if e.bounding_box().max.Z < 1e-3}
    stand = chamfer(list(feet), 0.4)
    return {"stand": stand}


def item_solid(p, k):
    w, h, t = item_dims(p, k)
    B = slot_corners(p)[k]
    u, n = _uv(p)
    x0 = p.big_right - w if k == "big" else p.x_center - w / 2
    # 0.05 mm off the ledge and prong so the interference check sees contact, not overlap
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
    xa, xb = comb_xs(p)

    # R1: each item sits on both combs (contact, no overlap)
    for k in ITEMS:
        it = asm[k]
        left = stand & Pos(0, 0, 0) * Box(p.x_center, 1000, 1000, align=(Align.MIN, Align.CENTER, Align.MIN))
        right = stand - left
        dl, dr = it.distance_to(left), it.distance_to(right)
        ov = _overlap(it, stand)
        yield dl < 0.2 and dr < 0.2 and ov < 1e-3, \
            f"R1 {k} rests on both combs (gaps {dl:.2f} / {dr:.2f} mm, overlap {ov:.3f} mm^3)"

    # R2: lifting each item straight up, or along its own lean, never hits the stand
    for k in ITEMS:
        it = asm[k]
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
    order = list(ITEMS)
    for a, b in zip(order, order[1:]):
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

    # R6: combined centre of mass stays inside the feet's footprint
    foot = stand & Box(1000, 1000, 0.5, align=(Align.CENTER, Align.CENTER, Align.MIN))
    pts = [(v.X, v.Y) for v in foot.vertices()]
    hull = _hull(pts)
    if _inside_margin(hull, np.mean(pts, axis=0)) < 0:
        hull = hull[::-1]
    masses = {"stand": (stand.volume * PETG * STAND_FILL, stand.center())}
    for k in ITEMS:
        it = asm[k]
        g = SHEET_G if k == "sheet" else it.volume * WOOD
        masses[k] = (g, it.center())
    worst = (math.inf, "")
    for mask in range(8):
        load = ["stand"] + [k for i, k in enumerate(ITEMS) if mask >> i & 1]
        m = sum(masses[x][0] for x in load)
        cx = sum(masses[x][0] * masses[x][1].X for x in load) / m
        cy = sum(masses[x][0] * masses[x][1].Y for x in load) / m
        d = _inside_margin(hull, (cx, cy))
        if d < worst[0]:
            worst = (d, "+".join(load))
    yield worst[0] >= 20, f"R6 worst centre-of-mass margin {worst[0]:.1f} mm inside the feet ({worst[1]}; need >= 20)"

    # R7: counter contact, arch clearance, no flat up-facing patches wide enough to pool water
    contact = sum(f.area for f in stand.faces() if f.normal_at().Z < -0.99 and f.center().Z < 0.01)
    bb = stand.bounding_box()
    yield contact <= 0.10 * bb.size.X * bb.size.Y, \
        f"R7 counter contact {contact:.0f} mm^2 ({100 * contact / (bb.size.X * bb.size.Y):.1f}% of the stand's plan, max 10%)"
    mesh = tessellate(stand)
    y0, y1 = base_extent(p)
    lines = [((x, yy) for yy in np.arange(y0 + 0.5, y1, 0.5)) for x in (xa, xb)]
    lines += [((xx, ry) for xx in np.arange(xa + 0.5, xb, 0.5)) for ry in (y0 + p.rail_w / 2, (y0 + y1) / 2, y1 - p.rail_w / 2)]
    worst_crown = math.inf
    for line in lines:
        pts = np.array([(a, b, -1.0) for a, b in line])
        loc, idx, _ = mesh.ray.intersects_location(pts, np.tile([0, 0, 1.0], (len(pts), 1)))
        first = np.full(len(pts), np.inf)
        np.minimum.at(first, idx, loc[:, 2])
        run = []
        for z in list(first) + [0.0]:
            if z > 0.5:
                run.append(z)
            elif run:
                worst_crown = min(worst_crown, max(run))
                run = []
    yield worst_crown >= 5, f"R7 lowest arch crown {worst_crown:.1f} mm above the counter (need >= 5)"
    flats = [2 * f.area / sum(e.length for e in f.edges()) for f in stand.faces() if f.normal_at().Z > 0.99]
    widest = max(flats, default=0.0)
    yield widest <= 2.0, f"R7 widest level up-facing patch {widest:.2f} mm across (max 2, so water runs off)"


# ---------------------------------------------------------------- callouts

def features(p, s, parts):
    u, n = _uv(p)
    xa, xb = comb_xs(p)
    y0, y1 = base_extent(p)
    corners = slot_corners(p)
    names = {"big": "big board", "sheet": "baking sheet", "small": "small board"}
    yield dict(name="side comb", part="stand", at=(xa, (y0 + y1) / 2, p.beam_h / 2), normal=(-1, 0, 0),
               params=["comb_span", "comb_w", "beam_h"],
               text="One of the two side frames. Every slot's ledge, lip and prong is part of a comb.")
    for k, B in corners.items():
        t = item_dims(p, k)[2]
        L = prong_len(p, k)
        mid = B - p.prong_tip / 2 * n + L / 2 * u
        yield dict(name=f"{names[k]} prong", part="stand", at=(xb, mid[0], mid[1]), normal=(0, -1, 0.3),
                   params=[f"prong_{k}", "prong_root", "prong_tip", "lean", "ridge"],
                   text=f"Leans {p.lean:.0f} degrees; the {names[k]}'s back face rests on its front ridge, {p.ridge:.0f} mm wide.")
        led = B + t / 2 * n
        yield dict(name=f"{names[k]} ledge", part="stand", at=(xa, led[0], led[1]), normal=(0, 0.3, 1),
                   params=["ledge_z", f"{k}_t", "lip_clear"],
                   text=f"Ridge the {names[k]}'s bottom edge sits on, tilted to match the lean.")
        lip = B + (t + p.lip_clear + p.lip_t / 2) * n + p.lip_h / 2 * u
        yield dict(name=f"{names[k]} lip", part="stand", at=(xa, lip[0], lip[1]), normal=(0, 1, 0.3),
                   params=["lip_h", "lip_t", "lip_clear"],
                   text=f"Stops the {names[k]}'s bottom sliding forward. Low enough that a straight lift clears it.")
    yield dict(name="cross rail", part="stand", at=(p.x_center, (y0 + y1) / 2, p.beam_h), normal=(0, 0.3, 1),
               params=["rail_w", "beam_h"], text="Ties the two combs together. Three of them: back, middle and front.")
    o0, o1 = arch_openings(y0, y1, p)[0]
    yield dict(name="arch and feet", part="stand", at=(xa, (o0 + o1) / 2, p.air_h), normal=(-1, 0, -0.3),
               params=["air_h", "bridge", "pad"],
               text=f"The frame stands on {p.pad:.0f} mm feet with {p.air_h:.0f} mm arches between them, so the counter underneath dries.")
    yield dict(name="ridge chamfer", part="stand", at=(xa, y0 + 20, p.beam_h), normal=(0, 0, 1), params=["ridge"],
               text="Every up-facing edge is chamfered to a narrow ridge so water runs off instead of sitting.")
