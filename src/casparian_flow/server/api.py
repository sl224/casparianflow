import logging
import json
from typing import List, Dict, Any, Optional
from fastapi import FastAPI, HTTPException, Depends, Query
from pydantic import BaseModel
from sqlalchemy.orm import Session, joinedload
from sqlalchemy import select, and_
from sqlalchemy.dialects.sqlite import insert as sqlite_upsert

from casparian_flow.config import settings
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import (
    FileLocation,
    FileTag,
    PluginSubscription,
    PluginConfig,
    ProcessingJob,
    StatusEnum
)
from casparian_flow.engine.queue import JobQueue
from casparian_flow.engine.sentinel import Sentinel # Use ZMQ to signal? 
# To signal sentinel, we need a ZMQ Client (Dealer) to send RELOAD message.
import zmq
from casparian_flow.protocol import pack_header, OpCode

logger = logging.getLogger(__name__)
app = FastAPI(title="Casparian Flow API", version="0.4.0")

# Dependency
def get_db():
    engine = get_engine(settings.database)
    with Session(engine) as session:
        yield session

def get_job_queue():
    engine = get_engine(settings.database)
    return JobQueue(engine)

# To signal Sentinel (assuming standard port)
def signal_sentinel_reload():
    # Helper to send RELOAD signal
    try:
        ctx = zmq.Context()
        socket = ctx.socket(zmq.DEALER)
        socket.connect("tcp://127.0.0.1:5555") # TODO: Configurable
        # RELOAD op has no payload
        msg = [pack_header(OpCode.RELOAD, 0, 0)]
        socket.send_multipart(msg)
        socket.close()
        ctx.term()
    except Exception as e:
        logger.error(f"Failed to signal Sentinel: {e}")

# --- Models ---

class JobSubmitRequest(BaseModel):
    file_id: int
    plugin_name: str
    priority: int = 10
    sinks: Optional[Dict[str, Any]] = None # Config overrides

class SubscriptionRequest(BaseModel):
    plugin_name: str
    topic: str
    is_active: bool = True

class FileResponse(BaseModel):
    id: int
    filename: str
    rel_path: str
    tags: List[str]

    class Config:
        from_attributes = True

# --- Endpoints ---

@app.post("/jobs", status_code=201)
def submit_job(req: JobSubmitRequest, queue: JobQueue = Depends(get_job_queue)):
    """
    Ad-Hoc Run: Trigger a specific plugin on a specific file.
    Supports 'sinks' overrides for custom output destinations.
    """
    try:
        queue.push_job(
            file_id=req.file_id,
            plugin_name=req.plugin_name,
            priority=req.priority,
            overrides=req.sinks
        )
        return {"status": "queued", "file_id": req.file_id, "plugin": req.plugin_name}
    except Exception as e:
        logger.error(f"Failed to submit job: {e}")
        raise HTTPException(status_code=500, detail=str(e))

@app.put("/config/wiring")
def update_subscription(req: SubscriptionRequest, db: Session = Depends(get_db)):
    """
    Dynamic Wiring: Map a Topic to a Plugin.
    Signals Sentinel to hot-reload configuration.
    """
    # Verify plugin exists
    plugin = db.get(PluginConfig, req.plugin_name)
    if not plugin:
        raise HTTPException(status_code=404, detail=f"Plugin {req.plugin_name} not found")

    # Upsert Subscription
    # Handling SQLite / generic upsert is tricky without dialect specific code.
    # Simple logic: Try get, then update/insert.
    sub = db.query(PluginSubscription).filter_by(
        plugin_name=req.plugin_name, topic_name=req.topic
    ).first()

    if sub:
        sub.is_active = req.is_active
    else:
        sub = PluginSubscription(
            plugin_name=req.plugin_name,
            topic_name=req.topic,
            is_active=req.is_active
        )
        db.add(sub)
    
    db.commit()

    # Hot Reload
    signal_sentinel_reload()
    
    return {"status": "updated", "subscription": f"{req.topic} -> {req.plugin_name}"}

@app.get("/files", response_model=List[FileResponse])
def browse_files(
    tag: Optional[str] = None, 
    limit: int = 100, 
    db: Session = Depends(get_db)
):
    """
    File Browser: List files, optionally filtered by Tag.
    """
    query = select(FileLocation).options(joinedload(FileLocation.source_root)).limit(limit)

    if tag:
        # Join with FileTag to filter
        query = query.join(FileTag).where(FileTag.tag == tag)
    
    # We need to fetch tags for the response. 
    # Ideally use a relationship, but FileTag relationship on FileLocation isn't defined yet!
    # Let's add the relationship to models.py? Or manual fetch.
    # Manual fetch for MVP efficiency or Relationship.
    # Modification to models.py to add relationship `tags` to FileLocation 
    # would make this endpoint cleaner with `joinedload`.
    
    # Executing query
    results = db.execute(query).scalars().all()
    
    # Populate tags manually if relationship missing
    response = []
    for f in results:
        # manual query for tags
        tags = db.query(FileTag.tag).filter(FileTag.file_id == f.id).all()
        tag_list = [t[0] for t in tags]
        
        response.append(FileResponse(
            id=f.id,
            filename=f.filename,
            rel_path=f.rel_path,
            tags=tag_list
        ))
        
    return response

@app.post("/files/{file_id}/tags")
def add_tag(file_id: int, tag: str, db: Session = Depends(get_db)):
    """
    Tagging: Add a manual tag to a file.
    """
    file_loc = db.get(FileLocation, file_id)
    if not file_loc:
        raise HTTPException(status_code=404, detail="File not found")
        
    # Check if exists
    existing = db.query(FileTag).filter_by(file_id=file_id, tag=tag).first()
    if not existing:
        db.add(FileTag(file_id=file_id, tag=tag))
        db.commit()
        
    return {"status": "tagged", "file_id": file_id, "tag": tag}
