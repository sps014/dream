"""Replace generated assets without truncating files that OneDrive is hydrating."""
from pathlib import Path
import os
import tempfile
import time


def write_asset(path, data):
    staging = Path(__file__).resolve().parents[3] / 'target/gravewake-assets/staging'
    staging.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=staging, delete=False) as output:
            temporary = Path(output.name)
            output.write(data)
        for attempt in range(6):
            try:
                os.replace(temporary, path)
                break
            except PermissionError:
                if attempt == 5:
                    raise
                time.sleep(.1 * (attempt + 1))
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
