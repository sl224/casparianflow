# src/casparian_flow/engine/worker_client.py
import sys
import zmq
import logging
import json
import time
import argparse
import importlib.util
from pathlib import Path
from typing import Dict, Any, Optional
import pyarrow as pa
from sqlalchemy import create_engine
from sqlalchemy.orm import Session

# Project Imports
from casparian_flow.protocol import OpCode, unpack_header, msg_hello, msg_ready, msg_data, msg_err
from casparian_flow.config import settings
from casparian_flow.db import access as sql_io
from casparian_flow.services.registrar import register_plugins_from_source
from casparian_flow.sdk import FileEvent  # Import FileEvent

logging.basicConfig(level=logging.INFO, format="%(asctime)s [WORKER] %(message)s")
logger = logging.getLogger(__name__)

class ProxyContext:
    def __init__(self, worker: 'GeneralistWorker'):
        self.worker = worker

    def register_topic(self, topic: str, default_uri: str = None) -> int:
        return 0

    def publish(self, handle: int, data: Any):
        if self.worker.current_job_id is None:
             raise RuntimeError("Attempted to publish data without an active job context.")
        
        data_bytes = self.worker._serialize_arrow(data)
        self.worker.socket.send_multipart(msg_data(self.worker.current_job_id, data_bytes))

class GeneralistWorker:
    def __init__(self, sentinel_addr: str, plugin_dir: Path, db_engine: Any):
        self.sentinel_addr = sentinel_addr
        self.plugin_dir = plugin_dir
        self.db_engine = db_engine
        self.plugins = {}
        
        self.ctx = zmq.Context()
        self.socket = self.ctx.socket(zmq.DEALER)
        self.socket.setsockopt(zmq.LINGER, 0)
        
        self.identity = f"w-{time.time_ns()}".encode()
        self.socket.setsockopt(zmq.IDENTITY, self.identity)
        
        self.running = False
        self.current_job_id: Optional[int] = None
        self.proxy_context = ProxyContext(self)

    def start(self):
        self.running = True
        logger.info(f"Scanning plugins in {self.plugin_dir}...")
        self._load_plugins()
        
        if not self.plugins:
            logger.warning("No valid plugins found. Exiting.")
            return

        with Session(self.db_engine) as session:
            register_plugins_from_source(self.plugin_dir, session)
            logger.info("Auto-registered plugin configurations in Database.")

        logger.info(f"Dialing Sentinel at {self.sentinel_addr}...")
        self.socket.connect(self.sentinel_addr)
        self.socket.send_multipart(msg_hello(list(self.plugins.keys())))
        
        poller = zmq.Poller()
        poller.register(self.socket, zmq.POLLIN)
        
        logger.info("Entering Event Loop...")
        self.socket.send_multipart(msg_ready())
        
        try:
            while self.running:
                socks = dict(poller.poll(timeout=100))
                if self.socket in socks:
                    self._handle_message()
        except Exception as e:
            logger.critical(f"Worker crashed: {e}", exc_info=True)
        finally:
            self.stop()

    def stop(self):
        self.running = False
        try:
            self.socket.close()
            self.ctx.term()
        except Exception: pass
        logger.info("Worker stopped.")

    def _handle_message(self):
        frames = self.socket.recv_multipart(flags=zmq.NOBLOCK)
        if not frames: return
        
        header = frames[0]
        op, job_id, _, _, _ = unpack_header(header)
        
        if op == OpCode.EXEC:
            payload = json.loads(frames[1].decode())
            plugin_name = payload["plugin"]
            file_path = payload["path"]
            
            logger.info(f"Received Job {job_id} -> {plugin_name}")
            self._execute_job(job_id, plugin_name, file_path)
            
            if self.running:
                self.socket.send_multipart(msg_ready())

    def _execute_job(self, job_id: int, plugin_name: str, file_path: str):
        self.current_job_id = job_id
        try:
            handler = self.plugins.get(plugin_name)
            if not handler:
                raise ValueError(f"Plugin {plugin_name} not loaded.")
            
            # Create File Event
            event = FileEvent(path=file_path, file_id=0) # ID would come from EXEC metadata ideally
            
            # Try new 'consume' API first, fallback to 'execute'
            if hasattr(handler, "consume") and callable(handler.consume):
                try:
                    result = handler.consume(event)
                except NotImplementedError:
                    # Fallback if consume() calls super().consume() which raises NotImplemented
                    result = handler.execute(file_path)
            else:
                result = handler.execute(file_path)
            
            # Handle return/yield results (if any)
            if result is not None and not (hasattr(result, "__iter__") and hasattr(result, "__next__")):
                if not hasattr(result, "send"): result = [result]

            if result:
                for batch in result:
                    if batch is not None:
                        data_bytes = self._serialize_arrow(batch)
                        self.socket.send_multipart(msg_data(job_id, data_bytes))
            
        except Exception as e:
            logger.error(f"Job {job_id} Failed: {e}", exc_info=True)
            self.socket.send_multipart(msg_err(job_id, str(e)))
        finally:
            self.current_job_id = None

    def _serialize_arrow(self, obj) -> bytes:
        if hasattr(obj, "to_arrow"): obj = obj.to_arrow()
        
        if isinstance(obj, pa.Table):
            sink = pa.BufferOutputStream()
            with pa.ipc.new_stream(sink, obj.schema) as writer:
                writer.write_table(obj)
            return sink.getvalue().to_pybytes()
            
        import pandas as pd
        if isinstance(obj, pd.DataFrame):
            try:
                table = pa.Table.from_pandas(obj)
                return self._serialize_arrow(table)
            except Exception as e:
                logger.warning(f"Pandas cast failed: {e}")
                return b""
        return b""

    def _load_plugins(self):
        if not self.plugin_dir.exists(): return
        sys.path.insert(0, str(self.plugin_dir.resolve()))
        for f in self.plugin_dir.glob("*.py"):
            if f.name.startswith("_"): continue
            try:
                spec = importlib.util.spec_from_file_location(f.stem, f)
                mod = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(mod)
                if hasattr(mod, "Handler"):
                    instance = mod.Handler()
                    if hasattr(instance, "configure"):
                        instance.configure(self.proxy_context, {})
                    self.plugins[f.stem] = instance
                    logger.info(f"Loaded: {f.stem}")
            except Exception as e:
                logger.error(f"Failed to load {f.name}: {e}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Casparian Generalist Worker")
    parser.add_argument("--connect", default="tcp://127.0.0.1:5555", help="Sentinel Address")
    parser.add_argument("--plugins", default="plugins", help="Path to plugins directory")
    args = parser.parse_args()

    engine = sql_io.get_engine(settings.database)
    worker = GeneralistWorker(args.connect, Path(args.plugins), engine)
    
    try:
        worker.start()
    except KeyboardInterrupt:
        worker.stop()