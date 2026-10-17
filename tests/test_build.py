import json

import pytest
from build123d import Box

from cadlib import bambu
from cadlib.config import ROOT, load_settings
from cadlib.project import place_on_plate, resolve_project, run_build
from cadlib.render import tessellate


def test_profiles_resolve():
    m = bambu.resolve_profile("machine", "Bambu Lab X2D 0.4 nozzle")
    assert m["printable_height"] and "inherits" not in m


@pytest.mark.parametrize("do_slice", [False, pytest.param(True, marks=pytest.mark.skipif(not bambu.bambu_command(), reason="no Bambu Studio"))])
def test_example_box_builds(do_slice):
    res = run_build(resolve_project("example-box"), do_render=True, do_slice=do_slice, log=lambda *_: None)
    assert res.status == "PASS", [(r.name, r.findings) for r in res.reports] + res.spec_findings
    assert all(f.exists() for f in res.files + res.images)
    if do_slice:
        assert res.slice.ok and res.slice.print_time_s > 0
    view = json.loads((res.project / "out" / "view.json").read_text())
    assert set(view["parts"]) == {"box", "lid"}
    lid = view["assembly"]["lid"]["matrix"]
    assert lid[2][2] == pytest.approx(-1) and lid[2][3] == pytest.approx(34)  # flipped onto the box


def test_plate_layout_uses_shared_region_when_possible():
    s = load_settings()
    placed = place_on_plate({"a": tessellate(Box(100, 50, 10)), "b": tessellate(Box(100, 80, 10))}, s)
    for m in placed.values():
        lo, hi = m.bounds
        assert lo[0] >= s.shared.x[0] and hi[0] <= s.shared.x[1] and lo[2] == 0


def test_plate_layout_oversized_part_uses_main_nozzle_and_too_many_fail():
    s = load_settings()
    lo, hi = place_on_plate({"big": tessellate(Box(250, 250, 10))}, s)["big"].bounds
    assert lo[0] >= 0 and hi[0] <= 256 and lo[0] < s.shared.x[0]
    assert place_on_plate({f"p{i}": tessellate(Box(200, 100, 10)) for i in range(3)}, s) is None


@pytest.mark.skipif(not bambu.bambu_command(), reason="no Bambu Studio")
def test_full_width_part_slices_on_left_nozzle():
    # The flatpak sandbox only sees $HOME, so pytest's /tmp paths won't do.
    tmp_path = ROOT / ".tmp-test" / "wide"
    tmp_path.mkdir(parents=True, exist_ok=True)
    s = load_settings()
    m = place_on_plate({"wide": tessellate(Box(250, 20, 5))}, s)["wide"]
    m.export(tmp_path / "wide.stl")
    r = bambu.slice_files([tmp_path / "wide.stl"], tmp_path / "out" / "wide.gcode.3mf", s.machine_profile,
                          s.process_profile, s.material.filament_profile, tmp_path / "profiles")
    assert r.ok, r.message
    assert r.filament_map == [1]
