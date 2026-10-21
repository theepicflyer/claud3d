"""curved-i-beam: an I-beam bent into a 25° wedge.

The two flanges are the flat end plates, on radii `sweep` degrees apart. The web joins them, and its inner and outer
edges follow concentric arcs. The I is `inner_arc` tall at the inner edge (measured along the arc, flange face to
flange face), so the inner radius is inner_arc / angle; the flanges are `flange_len` long, which sets the outer radius.
Fillets join the web to the flanges.

Modelled with the arc centre on the Z axis and one flange's outer face on the plane y=0, then turned so that flange
lies on the bed. In the print frame the arc centre is the Y axis, the web stands vertical, and the other flange leans
over at the sweep angle.
"""

import math

from build123d import Axis, Box, GeomType, Plane, Pos, Rectangle, RectangleRounded, Rot, fillet, revolve

PARAMS = dict(
    inner_arc=15.0,    # R1 I height at the inner edge, along the arc
    flange_len=43.0,   # R2 flange length, inner arc to outer arc
    sweep=25.0,        # R3 angle between the two flanges
    flange_w=10.0,     # R4 width of each flange
    flange_t=1.0,      # R5 flange thickness
    web_t=1.0,         # R5 web thickness
    corner_r=1.0,      # rounds the flanges' four corners, so the bed footprint has no sharp corners
    root_r=2.0,        # R6 fillets where the web meets the flanges
)

PRINT = dict(material="pla")

SECTIONS = {"beam": ("y", -5.0)}


def radii(p):
    ri = p.inner_arc / math.radians(p.sweep)
    return ri, ri + p.flange_len


def _to_print(shape):
    return Rot(X=90) * shape


def _pt(p, r, deg, z=None):
    """Arc-frame point (radius, angle from the bed flange, height across the flange) in the print frame."""
    z = p.flange_w / 2 if z is None else z
    a = math.radians(deg)
    return (r * math.cos(a), -z, r * math.sin(a))


def _ring(p, ri, ro, h, corner_r, start, arc):
    """Revolved band of rectangular (optionally rounded) cross-section, mid-height at flange_w / 2."""
    sec = Pos((ri + ro) / 2, p.flange_w / 2) * (RectangleRounded(ro - ri, h, corner_r) if corner_r else Rectangle(ro - ri, h))
    return Rot(Z=start) * revolve(Plane.XZ * sec, Axis.Z, arc)


def build(p, s):
    """Overshoot the flanges and web radially, then trim everything to one band so the flange ends and the web's
    edges share the same inner and outer arcs. The band's rounded section rounds the flange corners. The root fillets
    go on the straight edges of the web's two faces, which are where it meets the flanges."""
    ri, ro = radii(p)
    t, over = p.flange_t, 1.0
    plate = Pos((ro - ri) / 2, t / 2, p.flange_w / 2) * Box(ro - ri + 2 * over, t, p.flange_w)
    flange_a = Pos(ri, 0, 0) * plate
    flange_b = Rot(Z=p.sweep) * Pos(ri, -t, 0) * plate
    web = _ring(p, ri - over, ro + over, p.web_t, 0, 0, p.sweep)
    band = _ring(p, ri, ro, p.flange_w, p.corner_r, -10, p.sweep + 20)
    beam = (flange_a + flange_b + web) & band
    web_faces = beam.faces().filter_by(Axis.Z).filter_by(lambda f: abs(f.center().Z - p.flange_w / 2) < p.web_t)
    roots = [e for f in web_faces for e in f.edges() if e.geom_type == GeomType.LINE]
    return {"beam": _to_print(fillet(roots, p.root_r))}


def features(p, s, parts):
    ri, ro = radii(p)
    rm, t = (ri + ro) / 2, p.flange_t
    a_top = math.radians(p.sweep)
    yield dict(name="bottom flange", part="beam", at=_pt(p, rm, 0, p.flange_w), normal=(0, -1, 0.4),
               params=["flange_w", "flange_t", "corner_r"],
               text=f"Flat end plate that sits on the bed, {ro - ri:.1f} x {p.flange_w:g} mm, {t:g} mm thick, corners rounded {p.corner_r:g} mm.")
    yield dict(name="top flange", part="beam", at=_pt(p, rm, p.sweep, p.flange_w), normal=(-math.sin(a_top), -0.4, math.cos(a_top)),
               params=["sweep", "flange_w", "flange_t"],
               text=f"The other end plate, {p.sweep:g}° from the bottom one. Its underside overhangs the web by "
                    f"{(p.flange_w - p.web_t) / 2:g} mm each side at {90 - p.sweep:g}° from vertical.")
    yield dict(name="web", part="beam", at=_pt(p, rm, p.sweep / 2, (p.flange_w + p.web_t) / 2), normal=(0, -1, 0),
               params=["web_t"], text=f"Flat plate joining the flanges, {p.web_t:g} mm thick.")
    for deg, which in ((0, "bottom"), (p.sweep, "top")):
        off = p.web_t / 2 + p.root_r * 0.3
        sgn = 1 if deg == 0 else -1
        a = math.radians(deg)
        n = (-math.sin(a) * sgn, 0, math.cos(a) * sgn)
        base = _pt(p, rm, deg, p.flange_w / 2 + off)
        at = tuple(b + n_i * (t + p.root_r * 0.3) for b, n_i in zip(base, n))
        yield dict(name=f"{which} root fillets", part="beam", at=at, normal=(n[0] - 0.3, -1, n[2]), params=["root_r"],
                   text=f"{p.root_r:g} mm fillets on both sides where the web meets the {which} flange, so the joint "
                        "doesn't split along a layer line.")
    yield dict(name="inner edge", part="beam", at=_pt(p, ri, p.sweep / 2), normal=(-1, -0.3, -0.3), params=["inner_arc"],
               text=f"Short curved edge: the I is {p.inner_arc:g} mm tall here along the arc (radius {ri:.1f} mm).")
    yield dict(name="outer edge", part="beam", at=_pt(p, ro, p.sweep / 2), normal=(1, -0.3, 0.5), params=["flange_len"],
               text=f"Long curved edge: the I is {ro * math.radians(p.sweep):.1f} mm tall here along the arc (radius {ro:.1f} mm), "
                    "and the flange ends follow the same arc. "
                    f"Each flange is {ro - ri:.1f} mm long.")


def check(p, s, parts):
    beam = parts["beam"]
    sides = [f for f in beam.faces() if f.geom_type == GeomType.PLANE and abs(f.normal_at().Y) < 1e-6]
    n0, n1 = (f.normal_at() for f in sorted(sides, key=lambda f: f.area)[-2:])
    sweep = 180 - math.degrees(math.acos(max(-1.0, min(1.0, n0.dot(n1)))))
    yield abs(sweep - p.sweep) < 0.05, f"R3 flanges are {sweep:.2f}° apart (want {p.sweep:g})"

    a = math.radians(sweep / 2)
    web = beam & Plane(origin=(0, 0, 0), z_dir=(-math.sin(a), 0, math.cos(a)))
    rs = [math.hypot(v.X, v.Z) for v in web.vertices()]
    ri, ro = min(rs), max(rs)
    yield abs(ri * math.radians(sweep) - p.inner_arc) < 0.1, \
        f"R1 inner height {ri * math.radians(sweep):.2f} mm along the arc (radius {ri:.2f}, want {p.inner_arc:g})"
    yield abs(ro - ri - p.flange_len) < 0.05, \
        f"R2 flanges {ro - ri:.2f} mm long (want {p.flange_len:g}); outer height {ro * math.radians(sweep):.2f} mm along the arc"

    w = beam.bounding_box().size.Y
    yield abs(w - p.flange_w) < 0.01, f"R4 flange width {w:.2f} mm (want {p.flange_w:g})"

    web_t = web.bounding_box().size.Y
    wing = beam & Plane.YZ.offset((ri + ro) / 2) & Plane.XZ.offset(1.0)
    flange_t = min(e.bounding_box().max.Z for e in wing.edges())
    ok = abs(web_t - p.web_t) < 0.01 and abs(flange_t - p.flange_t) < 0.01
    yield ok, f"R5 web {web_t:.2f} mm, bottom flange {flange_t:.2f} mm thick (want {p.web_t:g} / {p.flange_t:g})"

    fillets = [f for f in beam.faces() if f.geom_type == GeomType.CYLINDER and abs(f.radius - p.root_r) < 0.01]
    yield len(fillets) == 4, f"R6 {len(fillets)} root fillets of {p.root_r:g} mm between web and flanges (want 4)"
