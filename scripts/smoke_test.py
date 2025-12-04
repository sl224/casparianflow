import os
import time
import shutil
import logging
from pathlib import Path
from sqlalchemy import create_engine
from casparian_flow.config import settings
from casparian_flow.db.base_session import SessionLocal
from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import PluginConfig, ProcessingJob, FileLocation, FileVersion
from casparian_flow.services.scout import Scout

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("smoke_test")

def run_smoke_test():
    # 1. Setup Environment
    test_dir = Path("smoke_test_data")
    if test_dir.exists():
        shutil.rmtree(test_dir)
    test_dir.mkdir()
    
    # Create a dummy file
    dummy_file = test_dir / "test_data.csv"
    dummy_file.write_text("col1,col2\n1,2\n3,4")
    
    # 2. Setup DB
    # Force SQLite for test
    db_url = f"sqlite:///{settings.database.db_location}"
    engine = create_engine(db_url)
    initialize_database(engine, reset_tables=True)
    
    db = SessionLocal(bind=engine)
    
    # 3. Configure System
    # Create SourceRoot
    root_id = get_or_create_sourceroot(engine, str(test_dir.absolute()))
    logger.info(f"Created SourceRoot with ID: {root_id}")
    
    # Create PluginConfig and TopicConfig
    from casparian_flow.db.models import TopicConfig
    
    plugin_config = PluginConfig(plugin_name="test_plugin")
    db.add(plugin_config)
    db.flush()
    
    topic_config = TopicConfig(
        plugin_name="test_plugin",
        topic_name="test",
        uri="parquet://./output",
        mode="append"
    )
    db.add(topic_config)
    db.commit()
    
    # 4. Run Scout (First Scan)
    scout = Scout(db)
    from casparian_flow.db.models import SourceRoot
    root = db.query(SourceRoot).get(root_id)
    scout.scan_source(root)
    
    # 5. Verify Initial State
    locations = db.query(FileLocation).all()
    versions = db.query(FileVersion).all()
    jobs = db.query(ProcessingJob).all()
    
    logger.info(f"After first scan: {len(locations)} locations, {len(versions)} versions, {len(jobs)} jobs")
    
    if len(locations) == 1 and len(versions) == 1 and len(jobs) == 1:
        logger.info("✅ Initial scan successful!")
        version1 = versions[0]
        job1 = jobs[0]
        logger.info(f"   Location: {locations[0].rel_path}")
        logger.info(f"   Version 1: Hash={version1.content_hash[:8]}..., Size={version1.size_bytes}")
        logger.info(f"   Job 1: Version={job1.file_version_id}, Plugin={job1.plugin_name}, Status={job1.status}")
    else:
        logger.error(f"❌ Expected 1 location, 1 version, 1 job. Got {len(locations)}, {len(versions)}, {len(jobs)}")
        exit(1)
    
    # 6. Modify File (Version Test)
    logger.info("\n🔄 Modifying file to test versioning...")
    time.sleep(0.1)  # Ensure different timestamp
    dummy_file.write_text("col1,col2\n5,6\n7,8\n9,10")  # Different content
    
    # 7. Run Scout Again
    scout.scan_source(root)
    
    # 8. Verify Version Tracking
    locations = db.query(FileLocation).all()
    versions = db.query(FileVersion).order_by(FileVersion.detected_at).all()
    jobs = db.query(ProcessingJob).order_by(ProcessingJob.id).all()
    
    logger.info(f"\nAfter file modification: {len(locations)} locations, {len(versions)} versions, {len(jobs)} jobs")
    
    if len(locations) == 1 and len(versions) == 2 and len(jobs) == 2:
        logger.info("✅ Version tracking successful!")
        logger.info(f"   Same location, but 2 versions detected")
        logger.info(f"   Version 1: Hash={versions[0].content_hash[:8]}...")
        logger.info(f"   Version 2: Hash={versions[1].content_hash[:8]}...")
        logger.info(f"   Job 1 → Version {jobs[0].file_version_id} (original)")
        logger.info(f"   Job 2 → Version {jobs[1].file_version_id} (modified)")
        
        # Verify jobs link to correct versions
        if jobs[0].file_version_id == versions[0].id and jobs[1].file_version_id == versions[1].id:
            logger.info("✅ Jobs correctly linked to their respective versions!")
        else:
            logger.error("❌ Job-to-version linkage incorrect!")
            exit(1)
    else:
        logger.error(f"❌ Expected 1 location, 2 versions, 2 jobs. Got {len(locations)}, {len(versions)}, {len(jobs)}")
        exit(1)
    
    logger.info("\n🎉 All smoke tests passed! Version tracking is working correctly.")

if __name__ == "__main__":
    run_smoke_test()
