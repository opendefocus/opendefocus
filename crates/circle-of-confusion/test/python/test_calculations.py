"""Tests for the native calculator against test/cases.json."""

import json
from pathlib import Path

import pytest
from circle_of_confusion import Calculator, CameraData, Math, Settings

CASES = json.loads((Path(__file__).parent.parent / "cases.json").read_text())


def _case_to_settings(raw: dict) -> Settings:
    settings = Settings()
    camera = raw.get("camera_data")
    if camera:
        settings.camera_data = CameraData(
            **{k: v for k, v in camera.items() if k in {"focal_length", "f_stop"}},
        )
    for key in ("size", "max_size", "focal_plane"):
        if key in raw:
            setattr(settings, key, raw[key])
    if "math" in raw:
        settings.math = Math.Real if raw["math"] == "REAL" else Math.OneDividedByZ
    return settings


@pytest.mark.parametrize("case", CASES)
def test_cases(case: dict) -> None:
    """Every case in cases.json matches the expected value."""
    calculator = Calculator(_case_to_settings(case["settings"]))
    assert calculator.calculate(case["coc"]) == pytest.approx(case["expected"], 0.01)


def test_calculate_many_matches_calculate() -> None:
    """The batch variant equals repeated scalar calls."""
    calculator = Calculator(Settings(math=Math.Real, focal_plane=3.0, size=1.0))
    values = [0.0, 1.0, 3.0, 10.0]
    assert calculator.calculate_many(values) == [
        calculator.calculate(v) for v in values
    ]


def test_invalid_arguments() -> None:
    """Wrong argument types raise TypeError."""
    with pytest.raises(TypeError):
        Calculator("not settings")  # ty: ignore[invalid-argument-type]
    with pytest.raises(TypeError):
        Calculator(Settings()).calculate(None)  # ty: ignore[invalid-argument-type]
