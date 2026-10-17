"""Exact B-rep features of a part for the review app: snap points, edges, and analytic faces."""

from __future__ import annotations

import math

from build123d import GeomType, Shape
from OCP.BRep import BRep_Tool
from OCP.BRepAdaptor import BRepAdaptor_Surface


def _r(v, nd: int = 4) -> list[float]:
    return [round(float(c), nd) for c in (v.X, v.Y, v.Z)]


def topology(shape: Shape, step: float = 1.0) -> dict:
    """Vertices, edges (lines, circles/arcs, sampled curves) and plane/cylinder faces in the part's frame."""
    seams = set()
    for f in shape.faces():
        for e in f.edges():
            if BRep_Tool.IsClosed_s(e.wrapped, f.wrapped):
                seams.add(hash(e))
    vertices = [_r(v.center()) for v in shape.vertices()]
    edges = []
    for e in shape.edges():
        if hash(e) in seams:
            continue
        n = max(2, min(96, math.ceil(e.length / step) + 1))
        pts = [_r(v) for v in e.positions([i / (n - 1) for i in range(n)])]
        if e.geom_type == GeomType.LINE:
            edges.append({"t": "line", "a": pts[0], "b": pts[-1], "len": round(e.length, 4)})
        elif e.geom_type == GeomType.CIRCLE:
            edges.append({"t": "circle", "c": _r(e.arc_center), "r": round(e.radius, 4), "pts": pts,
                          "full": e.is_closed, "len": round(e.length, 4)})
        else:
            edges.append({"t": "curve", "pts": pts, "len": round(e.length, 4)})
    faces = []
    for f in shape.faces():
        if f.geom_type == GeomType.PLANE:
            n = f.normal_at()
            faces.append({"t": "plane", "n": _r(n, 6), "d": round(n.dot(f.center()), 4), "area": round(f.area, 3)})
        elif f.geom_type == GeomType.CYLINDER:
            cyl = BRepAdaptor_Surface(f.wrapped).Cylinder()
            o, d = cyl.Axis().Location(), cyl.Axis().Direction()
            faces.append({"t": "cyl", "o": [round(o.X(), 4), round(o.Y(), 4), round(o.Z(), 4)],
                          "dir": [round(d.X(), 6), round(d.Y(), 6), round(d.Z(), 6)], "r": round(cyl.Radius(), 4),
                          "area": round(f.area, 3)})
    return {"vertices": vertices, "edges": edges, "faces": faces}
