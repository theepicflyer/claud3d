import pytest

from cadlib import bambu, config

HAS_BAMBU = bambu.bambu_command() is not None
requires_bambu = pytest.mark.skipif(not HAS_BAMBU, reason="needs Bambu Studio and its profiles")


@pytest.fixture(scope="session")
def stock_settings():
    """printer.toml without the local override; without Bambu Studio, a slicer-less 256 mm cube."""
    if HAS_BAMBU:
        return config.load_settings(local=False)
    local = config.ROOT / ".tmp-test" / "printer.none.toml"
    local.parent.mkdir(parents=True, exist_ok=True)
    local.write_text('[printer]\nslicer = "none"\nbuild_volume = [256, 256, 256]\n')
    with pytest.MonkeyPatch.context() as mp:
        mp.setattr(config, "PRINTER_LOCAL_TOML", local)
        return config.load_settings()
