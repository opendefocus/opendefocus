"""Tests for the settings data classes."""

from circle_of_confusion import CameraData, Math, Settings, WorldUnit


def test_defaults() -> None:
    """Defaults match the Rust defaults."""
    settings = Settings()
    assert settings.size == 5.0
    assert settings.max_size == 10.0
    assert settings.math == Math.OneDividedByZ
    assert settings.camera_data is None
    camera = CameraData()
    assert camera.focal_length == 50.0
    assert camera.world_unit == WorldUnit.M
    assert camera.resolution.width == 1920


def test_attributes_round_trip() -> None:
    """Attributes can be set and read back."""
    settings = Settings(camera_data=CameraData(focal_length=51), focal_plane=20)
    settings.math = Math.Real
    assert settings.camera_data is not None
    assert settings.camera_data.focal_length == 51
    assert settings.focal_plane == 20
    assert settings.math == Math.Real
