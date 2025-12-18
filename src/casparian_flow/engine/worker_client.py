# src/casparian_flow/engine/worker_client.py
import sys
import zmq
import logging
import json
import time
import argparse
import importlib.util
from pathlib import Path
from typing import Dict, Any
import pyarrow as pa
from sqlalchemy import create_engine
from sqlalchemy.orm import Session

# Project Imports
from casparian_flow.protocol import OpCode, unpack_header, msg_hello, msg_ready, msg_data, msg_err
from casparian_flow.config import settings
from casparian_flow.db import access as sql_io
from casparian_flow.services.registrar import register_plugins_from_source

logging.basicConfig(level=logging.INFO, format="%(asctime)s [WORKER] %(message)s")
logger = logging.getLogger(__name__)

class GeneralistWorker:
    def __init__(self, sentinel_addr: str, plugin_dir: Path, db_engine: Any):
        self.sentinel_addr = sentinel_addr
        self.plugin_dir = plugin_dir
        self.db_engine = db_engine
        self.plugins = {}  # name -> instance
        
        self.ctx = zmq.Context()
        self.socket = self.ctx.socket(zmq.DEALER)
        # Identity helps Sentinel track us across reconnects
        self.identity = f"w-{time.time_ns()}".encode()
        self.socket.setsockopt(zmq.IDENTITY, self.identity)

    def start(self):
        # 1. Discovery & Registration (Chunk 7)
        logger.info(f"Scanning plugins in {self.plugin_dir}...")
        self._load_plugins()
        
        if not self.plugins:
            logger.warning("No valid plugins found. Exiting.")
            return

        # Auto-register RoutingRules and TopicConfigs in DB
        with Session(self.db_engine) as session:
            register_plugins_from_source(self.plugin_dir, session)
            logger.info("Auto-registered plugin configurations in Database.")

        # 2. Network Handshake
        logger.info(f"Dialing Sentinel at {self.sentinel_addr}...")
        self.socket.connect(self.sentinel_addr)
        
        caps = list(self.plugins.keys())
        self.socket.send_multipart(msg_hello(caps))
        logger.info(f"Sent HELLO. Capabilities: {caps}")
        
        # 3. Execution Loop
        logger.info("Entering Event Loop...")
        while True:
            self._loop_tick()

    def _loop_tick(self):
        # Announce we are ready for work
        self.socket.send_multipart(msg_ready())
        
        # Block until we get a command
        frames = self.socket.recv_multipart()
        if not frames: return
        
        header = frames[0]
        op, job_id, _, _, _ = unpack_header(header)
        
        if op == OpCode.EXEC:
            payload = json.loads(frames[1].decode())
            plugin_name = payload["plugin"]
            file_path = payload["path"]
            
            logger.info(f"Received Job {job_id} -> {plugin_name} on {Path(file_path).name}")
            self._execute_job(job_id, plugin_name, file_path)
        elif op == OpCode.HEARTBEAT:
            pass # Socket handles keeping connection alive usually, or we can reply

    def _execute_job(self, job_id: int, plugin_name: str, file_path: str):
        try:
            handler = self.plugins.get(plugin_name)
            if not handler:
                raise ValueError(f"Plugin {plugin_name} not loaded.")
            
            # Run the user's plugin code
            # We support both yield (generator) and return
            result = handler.execute(file_path)
            
            # Normalize to iterator
            if not hasattr(result, "__iter__") and not hasattr(result, "__next__"):
                result = [result] if result is not None else []

            # Stream results back to Sentinel
            for batch in result:
                if batch is not None:
                    data_bytes = self._serialize_arrow(batch)
                    self.socket.send_multipart(msg_data(job_id, data_bytes))
            
            # Loop sends READY next, which Sentinel interprets as Job Done.
            
        except Exception as e:
            logger.error(f"Job {job_id} Failed: {e}", exc_info=True)
            self.socket.send_multipart(msg_err(job_id, str(e)))

    def _serialize_arrow(self, obj) -> bytes:
        """Convert DataFrame/Table to Arrow IPC bytes."""
        if hasattr(obj, "to_arrow"): # Polars/Pandas 3?
            obj = obj.to_arrow()
        
        if isinstance(obj, pa.Table):
            sink = pa.BufferOutputStream()
            with pa.ipc.new_stream(sink, obj.schema) as writer:
                writer.write_table(obj)
            return sink.getvalue().to_pybytes()
            
        # Pandas fallback
        import pandas as pd
        if isinstance(obj, pd.DataFrame):
            table = pa.Table.from_pandas(obj)
            return self._serialize_arrow(table)
            
        return b""

    def _load_plugins(self):
        if not self.plugin_dir.exists():
            return
            
        # Add plugin dir to path so plugins can import siblings
        sys.path.insert(0, str(self.plugin_dir.resolve()))

        for f in self.plugin_dir.glob("*.py"):
            if f.name.startswith("_"): continue
            
            try:
                spec = importlib.util.spec_from_file_location(f.stem, f)
                mod = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(mod)
                
                if hasattr(mod, "Handler"):
                    self.plugins[f.stem] = mod.Handler()
                    logger.info(f"Loaded: {f.stem}")
            except Exception as e:
                logger.error(f"Failed to load {f.name}: {e}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Casparian Generalist Worker")
    parser.add_argument("--connect", default="tcp://127.0.0.1:5555", help="Sentinel Address")
    parser.add_argument("--plugins", default="plugins", help="Path to plugins directory")
    args = parser.parse_args()

    # Create DB Engine using global settings
    engine = sql_io.get_engine(settings.database)
    
    worker = GeneralistWorker(
        sentinel_addr=args.connect, 
        plugin_dir=Path(args.plugins),
        db_engine=engine
    )
    
    try:
        worker.start()
    except KeyboardInterrupt:
        print("Worker stopped.")