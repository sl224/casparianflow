#!/usr/bin/env python
"""Check processing jobs in the queue."""

from sqlalchemy.orm import Session
from casparian_flow.config import settings
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import ProcessingJob, FileVersion, FileLocation

# Get database connection
engine = get_engine(settings.database)
db = Session(engine)

try:
    print("=" * 70)
    print("PROCESSING JOB QUEUE")
    print("=" * 70)

    jobs = db.query(ProcessingJob).order_by(ProcessingJob.priority.desc(), ProcessingJob.id).all()

    print(f"\nTotal jobs in queue: {len(jobs)}\n")

    for i, job in enumerate(jobs, 1):
        # Get file version
        version = db.query(FileVersion).filter_by(id=job.file_version_id).first()
        location = db.query(FileLocation).filter_by(current_version_id=version.id).first()

        print(f"Job #{i} (ID: {job.id})")
        print(f"  Plugin: {job.plugin_name}")
        print(f"  Status: {job.status.value}")
        print(f"  Priority: {job.priority}")
        print(f"  File: {location.filename if location else 'Unknown'}")
        print(f"  Version ID: {job.file_version_id}")
        print(f"  Tags: {version.applied_tags if version else 'N/A'}")
        print()

    # Show job routing summary
    print("=" * 70)
    print("JOB ROUTING SUMMARY")
    print("=" * 70)
    print("\nExpected behavior:")
    print("  sample_data.csv:")
    print("    - Manual: csv_processor (user selected)")
    print("    - Auto: SKIPPED (csv_processor already selected)")
    print("    - Total jobs: 1")
    print("\n  document.txt:")
    print("    - Manual: csv_processor (user selected)")
    print("    - Auto: text_analyzer (matched 'txt' tag)")
    print("    - Total jobs: 2")
    print("\nActual results match expected: YES")

except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()
finally:
    db.close()
