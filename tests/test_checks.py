import pytest
from build123d import Axis, Box, Cone, Pos

from cadlib import checks
from cadlib.config import load_settings
from cadlib.render import tessellate


@pytest.fixture(scope="module")
def s():
    return load_settings()


def statuses(shape, s):
    rep = checks.check_part("t", tessellate(Pos(0, 0, -shape.bounding_box().min.Z) * shape), s)
    return {f.check: f.status for f in rep.findings}, rep


def test_plain_block_passes(s):
    st, rep = statuses(Box(20, 20, 10), s)
    assert rep.status == checks.PASS, rep.findings
    assert rep.stats["bbox"] == [20, 20, 10]


def test_nozzle_regions_from_profile(s):
    left, right = s.nozzles
    assert left.size == (256, 256, 261) and left.drive == "direct drive"
    assert right.size == (235.5, 256, 256) and right.x == (20.5, 256) and right.drive == "bowden"
    assert s.shared.size == (235.5, 256, 256)


@pytest.mark.parametrize("dims, status, phrase", [
    ((200, 200, 200), checks.PASS, "every nozzle"),
    ((250, 200, 10), checks.WARN, "rotated 90 deg"),
    ((250, 250, 10), checks.WARN, "left (main)"),
    ((20, 20, 258), checks.WARN, "left (main)"),
    ((300, 20, 10), checks.FAIL, "exceeds"),
])
def test_build_volume_per_nozzle(s, dims, status, phrase):
    _, rep = statuses(Box(*dims), s)
    f = next(f for f in rep.findings if f.check == "build volume")
    assert f.status == status and phrase in f.detail, f.detail


def test_overhang_detected(s):
    # Inverted cone: sides slope 63 deg from vertical.
    st, rep = statuses(Cone(2, 10, 4), s)
    assert st["overhangs"] == checks.WARN
    assert len(rep.overhang_faces) > 0


def test_floating_ceiling_warns(s):
    # The upper plate hovers over a gap, so its underside has no wall below any edge.
    shelf = Box(20, 20, 2) + Pos(0, 0, 5) * Box(20, 20, 2)
    st, rep = statuses(shelf, s)
    assert "floating" in next(f.detail for f in rep.findings if f.check == "bridges")


def test_long_bridge_warns(s):
    arch = Pos(0, 0, 10) * Box(60, 10, 20) - Pos(0, 0, 7.5) * Box(40, 12, 15)
    st, _ = statuses(arch, s)
    assert st["bridges"] == checks.WARN


def test_thin_wall_fails(s):
    fin = Box(30, 0.5, 10)
    st, _ = statuses(fin, s)
    assert st["wall thickness"] == checks.FAIL


def test_disconnected_bodies_warn(s):
    two = Box(10, 10, 10) + Pos(30, 0, 0) * Box(10, 10, 10)
    st, _ = statuses(two, s)
    assert st["bodies"] == checks.WARN


def test_interference(s):
    a = Box(10, 10, 10)
    found = checks.check_interference({"a": a, "b": Pos(5, 0, 0) * Box(10, 10, 10)})
    assert found[0].status == checks.FAIL
    found = checks.check_interference({"a": a, "b": Pos(10.5, 0, 0) * Box(10, 10, 10)})
    assert found[0].status == checks.PASS and "0.500" in found[0].detail


def test_short_bridge_passes(s):
    arch = Pos(0, 0, 10) * Box(30, 10, 20) - Pos(0, 0, 7.5) * Box(10, 12, 15)
    st, _ = statuses(arch, s)
    assert st["bridges"] == checks.PASS


def test_cantilever_warns(s):
    t = Pos(0, 0, 5) * Box(10, 10, 10) + Pos(0, 0, 12.5) * Box(30, 10, 5)
    st, rep = statuses(t, s)
    assert st["bridges"] == checks.WARN
    assert "cantilever" in next(f.detail for f in rep.findings if f.check == "bridges")


def test_45_degree_chamfer_passes(s):
    from build123d import chamfer
    b = Box(30, 30, 10)
    st, _ = statuses(chamfer(b.edges().group_by(Axis.Z)[0], 3), s)
    assert st["overhangs"] == checks.PASS
