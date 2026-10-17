import pytest

from cadlib import bambu
from cadlib.project import resolve_project, run_build


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
