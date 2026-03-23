"""Shared pytest fixtures for analysis tests."""

from pathlib import Path

import pytest


@pytest.fixture
def project_root() -> Path:
    """Return the project root directory."""
    return Path(__file__).resolve().parent.parent.parent


@pytest.fixture
def content_dir(project_root: Path) -> Path:
    """Return the content/ directory path."""
    return project_root / "content"
