"""Read-only version audit of the selected contributor interpreter. No audio/model imports."""
from importlib import metadata
from pathlib import Path
import sys


def check():
    if sys.version_info[:2] != (3, 12):
        raise RuntimeError("Python 3.12 is required")
    from packaging.requirements import Requirement
    path = Path(__file__).with_name("requirements-contributor-lock.txt")
    for line in path.read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        requirement = Requirement(line)
        if requirement.marker and not requirement.marker.evaluate():
            continue
        version = metadata.version(requirement.name)
        if not requirement.specifier.contains(version):
            raise RuntimeError(f"{requirement.name}: expected {requirement.specifier}, installed {version}")


if __name__ == "__main__":
    try:
        check()
        print("ok")
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
