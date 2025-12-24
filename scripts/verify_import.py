#!/usr/bin/env python
"""Verify imported files and check managed directory."""

from pathlib import Path
from sqlalchemy.orm import Session
from casparian_flow.config import settings
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import SourceRoot, FileLocation

# Get database connection
engine = get_engine(settings.database)
db = Session(engine)

try:
    print("Checking managed directory...")

    # 1. Verify managed directory exists
    managed_dir = Path("data/managed").resolve()
    print(f"\nManaged directory: {managed_dir}")
    print(f"Exists: {managed_dir.exists()}")

    if managed_dir.exists():
        files = list(managed_dir.iterdir())
        print(f"Files in managed dir: {len(files)}")
        for f in files:
            print(f"  - {f.name} ({f.stat().st_size} bytes)")

    # 2. Verify managed SourceRoot in database
    managed_root = db.query(SourceRoot).filter_by(type="managed").first()
    if managed_root:
        print(f"\nManaged SourceRoot:")
        print(f"  ID: {managed_root.id}")
        print(f"  Path: {managed_root.path}")
        print(f"  Type: {managed_root.type}")
        print(f"  Active: {managed_root.active}")

        # Count files in managed source
        file_count = db.query(FileLocation).filter_by(source_root_id=managed_root.id).count()
        print(f"  Files tracked: {file_count}")

    # 3. List all FileLocations
    print(f"\nAll FileLocations in database:")
    locations = db.query(FileLocation).all()
    for loc in locations:
        print(f"  ID {loc.id}: {loc.filename} (SourceRoot: {loc.source_root_id})")

    # 4. Verify file contents match
    print(f"\nVerifying file contents...")
    for loc in locations:
        if loc.source_root_id == managed_root.id:
            file_path = managed_dir / loc.rel_path
            if file_path.exists():
                print(f"  [{loc.filename}] File exists: YES")
                content = file_path.read_text()
                print(f"    Preview: {content[:50]}...")
            else:
                print(f"  [{loc.filename}] File exists: NO (ERROR!)")

except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()
finally:
    db.close()
