#!/usr/bin/env python
"""Test the import feature programmatically."""

from sqlalchemy.orm import Session
from casparian_flow.config import settings
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import (
    SourceRoot, FileLocation, FileVersion, FileTag, ProcessingJob
)
from casparian_flow.services.import_service import ImportService

# Get database connection
engine = get_engine(settings.database)
db = Session(engine)

try:
    print("=" * 60)
    print("TESTING FILE IMPORT FEATURE")
    print("=" * 60)

    # 1. Get test SourceRoot
    source_root = db.query(SourceRoot).filter_by(id=1).first()
    if not source_root:
        print("ERROR: SourceRoot ID 1 not found. Run test_setup_import.py first.")
        exit(1)

    print(f"\n1. Source Root: {source_root.path}")

    # 2. Test importing files
    print("\n2. Importing files...")

    import_service = ImportService(db, managed_dir="data/managed")

    # Import 2 files: sample_data.csv and document.txt
    imported = import_service.import_files(
        source_root_id=1,
        rel_paths=["sample_data.csv", "document.txt"],
        manual_tags={"test", "demo"},
        manual_plugins={"csv_processor"}  # Manually select csv_processor
    )

    print(f"   Imported {len(imported)} file(s)")

    # 3. Verify database entries
    print("\n3. Verifying database entries...")

    for file_loc in imported:
        print(f"\n   File: {file_loc.filename} (ID: {file_loc.id})")
        print(f"   - Location: {file_loc.rel_path}")
        print(f"   - Source Root: {file_loc.source_root_id}")

        # Get FileVersion
        version = db.query(FileVersion).filter_by(id=file_loc.current_version_id).first()
        if version:
            print(f"   - Version ID: {version.id}")
            print(f"   - Content Hash: {version.content_hash[:16]}...")
            print(f"   - Applied Tags: {version.applied_tags}")

        # Get manual tags
        manual_tags = db.query(FileTag).filter_by(file_id=file_loc.id).all()
        if manual_tags:
            print(f"   - Manual Tags: {[t.tag for t in manual_tags]}")

        # Get jobs
        jobs = db.query(ProcessingJob).filter_by(file_version_id=version.id).all()
        if jobs:
            print(f"   - Jobs Created: {len(jobs)}")
            for job in jobs:
                print(f"      * Plugin: {job.plugin_name}, Status: {job.status.value}, Priority: {job.priority}")

    # 4. Summary
    print("\n" + "=" * 60)
    print("IMPORT TEST SUMMARY")
    print("=" * 60)

    total_files = db.query(FileLocation).count()
    total_versions = db.query(FileVersion).count()
    total_jobs = db.query(ProcessingJob).count()

    print(f"Total FileLocations: {total_files}")
    print(f"Total FileVersions: {total_versions}")
    print(f"Total ProcessingJobs: {total_jobs}")

    # Check for expected auto-routing
    print("\n5. Checking auto-routing...")
    csv_file = [f for f in imported if f.filename == "sample_data.csv"][0]
    csv_version = db.query(FileVersion).filter_by(id=csv_file.current_version_id).first()
    csv_jobs = db.query(ProcessingJob).filter_by(file_version_id=csv_version.id).all()

    print(f"\n   CSV file should trigger:")
    print(f"   - Manual: csv_processor (user selected)")
    print(f"   - Auto: csv_processor (already selected, should be skipped)")
    print(f"   - Actual jobs: {[j.plugin_name for j in csv_jobs]}")

    txt_file = [f for f in imported if f.filename == "document.txt"][0]
    txt_version = db.query(FileVersion).filter_by(id=txt_file.current_version_id).first()
    txt_jobs = db.query(ProcessingJob).filter_by(file_version_id=txt_version.id).all()

    print(f"\n   TXT file should trigger:")
    print(f"   - Auto: text_analyzer (subscription match)")
    print(f"   - Actual jobs: {[j.plugin_name for j in txt_jobs]}")

    print("\n" + "=" * 60)
    print("TEST COMPLETE!")
    print("=" * 60)

except Exception as e:
    print(f"\nError during test: {e}")
    import traceback
    traceback.print_exc()
finally:
    db.close()
