import json

import pytest
from build123d import Box

from conftest import requires_bambu

from cadlib import bambu, config
from cadlib.config import ROOT, load_settings
from cadlib.project import place_on_plate, resolve_project, run_build
from cadlib.render import tessellate


@requires_bambu
def test_profiles_resolve():
    m = bambu.resolve_profile("machine", "Bambu Lab X2D 0.4 nozzle")
    assert m["printable_height"] and "inherits" not in m


@requires_bambu
@pytest.mark.parametrize("do_slice", [False, True])
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
    assert {f["name"] for f in view["features"]} >= {"lip", "finger scallop", "wall"}
    assert view["params"]["lip_h"] == 3.0 and view["parts"]["box"]["topo"] == "box.topo.json"


@requires_bambu
def test_plate_layout_uses_shared_region_when_possible():
    s = load_settings(local=False)
    placed = place_on_plate({"a": tessellate(Box(100, 50, 10)), "b": tessellate(Box(100, 80, 10))}, s)
    for m in placed.values():
        lo, hi = m.bounds
        assert lo[0] >= s.shared.x[0] and hi[0] <= s.shared.x[1] and lo[2] == 0


@requires_bambu
def test_plate_layout_oversized_part_uses_main_nozzle_and_too_many_fail():
    s = load_settings(local=False)
    lo, hi = place_on_plate({"big": tessellate(Box(250, 250, 10))}, s)["big"].bounds
    assert lo[0] >= 0 and hi[0] <= 256 and lo[0] < s.shared.x[0]
    assert place_on_plate({f"p{i}": tessellate(Box(200, 100, 10)) for i in range(3)}, s) is None


@requires_bambu
def test_full_width_part_slices_on_left_nozzle():
    # The flatpak sandbox only sees $HOME, so pytest's /tmp paths won't do.
    tmp_path = ROOT / ".tmp-test" / "wide"
    tmp_path.mkdir(parents=True, exist_ok=True)
    s = load_settings(local=False)
    m = place_on_plate({"wide": tessellate(Box(250, 20, 5))}, s)["wide"]
    m.export(tmp_path / "wide.stl")
    r = bambu.slice_files([tmp_path / "wide.stl"], tmp_path / "out" / "wide.gcode.3mf", s.machine_profile,
                          s.process_profile, s.material.filament_profile, tmp_path / "profiles")
    assert r.ok, r.message
    assert r.filament_map == [1]


def test_local_toml_merges_over_printer_toml(monkeypatch):
    local = ROOT / ".tmp-test" / "printer.local.toml"
    local.parent.mkdir(parents=True, exist_ok=True)
    local.write_text('[printer]\nslicer = "none"\nbuild_volume = [180, 180, 180]\n[fit]\nslide = 0.4\n')
    monkeypatch.setattr(config, "PRINTER_LOCAL_TOML", local)
    s = load_settings()
    assert s.slicer == "none" and s.build_volume == (180, 180, 180)
    assert s.fit["slide"] == 0.4 and s.fit["press"] == 0.10
    assert load_settings({"material": "petg"}).clearance("slide") == pytest.approx(0.45)


def test_slicer_none_builds_without_slicing(monkeypatch):
    local = ROOT / ".tmp-test" / "printer.none.toml"
    local.parent.mkdir(parents=True, exist_ok=True)
    local.write_text('[printer]\nslicer = "none"\nbuild_volume = [256, 256, 256]\n')
    monkeypatch.setattr(config, "PRINTER_LOCAL_TOML", local)
    res = run_build(resolve_project("example-box"), do_render=False, do_slice=True, log=lambda *_: None)
    assert res.status == "PASS" and res.slice is None
    assert "skipped" in (res.project / "out" / "report.md").read_text()


def test_private_projects_resolve(monkeypatch, tmp_path):
    priv = tmp_path / "private"
    (priv / "secret-part").mkdir(parents=True)
    (priv / "secret-part" / "model.py").write_text("")
    monkeypatch.setattr("cadlib.project.PROJECT_DIRS", (ROOT / "projects", priv))
    assert resolve_project("secret-part") == (priv / "secret-part").resolve()
    assert resolve_project("example-box") == (ROOT / "projects" / "example-box").resolve()
