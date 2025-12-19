import os


def pytest_configure():
    """Clean the dev build environment before tests run."""
    os.environ.pop("OPENDEFOCUS_USE_DEV_BUILD", None)
