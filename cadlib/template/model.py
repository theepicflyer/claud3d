"""{{name}}: one-line description."""

from build123d import Box

from cadlib import features as F  # noqa: F401

# Every dimension that a requirement or a mating part depends on lives here. Override with `cad build {{name}} -p key=value`.
PARAMS = dict(
    size_x=40.0,
    size_y=30.0,
    size_z=10.0,
)

# Material key from printer.toml [materials]; optional "process", "filament", "slicer" (dict of Bambu process overrides).
PRINT = dict(material="pla")

# Optional section planes for renders: {"part name" or "assembly": ("x"|"y"|"z", offset)}.
SECTIONS = {}


def build(p, s):
    """Return {name: Part} in print orientation. `s` is cadlib.config.Settings (s.clearance("slide"), s.nozzle...)."""
    part = Box(p.size_x, p.size_y, p.size_z)
    return {"part": part}


# def assembly(p, s, parts):
#     """Optional: parts moved into assembled positions, for the assembly render and interference check."""
#     return {"part": parts["part"]}


def check(p, s, parts):
    """Yield (ok, message) per requirement, measured from the geometry. Prefix messages with the requirement id."""
    bb = parts["part"].bounding_box()
    yield abs(bb.size.X - p.size_x) < 0.01, f"R1 width {bb.size.X:.2f} mm (want {p.size_x})"
