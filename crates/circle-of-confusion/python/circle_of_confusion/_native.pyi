import enum

class WorldUnit(enum.Enum):
    M = ...
    Mm = ...
    Cm = ...
    Dm = ...
    In = ...
    Ft = ...

class Math(enum.Enum):
    OneDividedByZ = ...
    Real = ...

class Filmback:
    width: float
    height: float
    def __init__(self, width: float = 24.576, height: float = 18.672) -> None: ...

class Resolution:
    width: int
    height: int
    def __init__(self, width: int = 1920, height: int = 1080) -> None: ...

class CameraData:
    focal_length: float
    f_stop: float
    filmback: Filmback
    near_field: float
    far_field: float
    world_unit: WorldUnit
    resolution: Resolution
    def __init__(
        self,
        focal_length: float = 50.0,
        f_stop: float = 16.0,
        filmback: Filmback | None = None,
        near_field: float = 0.1,
        far_field: float = 10000.0,
        world_unit: WorldUnit = ...,
        resolution: Resolution | None = None,
    ) -> None: ...

class Settings:
    size: float
    max_size: float
    math: Math
    focal_plane: float
    protect: float
    pixel_aspect: float
    camera_data: CameraData | None
    def __init__(
        self,
        size: float = 5.0,
        max_size: float = 10.0,
        math: Math = ...,
        focal_plane: float = 0.0,
        protect: float = 0.0,
        pixel_aspect: float = 1.0,
        camera_data: CameraData | None = None,
    ) -> None: ...

class Calculator:
    def __init__(self, settings: Settings) -> None: ...
    def calculate(self, value: float) -> float: ...
    def calculate_many(self, values: list[float]) -> list[float]: ...
