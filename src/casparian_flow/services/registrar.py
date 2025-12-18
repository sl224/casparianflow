# src/casparian_flow/services/registrar.py
import logging
import sys
import importlib.util
from pathlib import Path
from sqlalchemy.orm import Session
from casparian_flow.db.models import PluginConfig, TopicConfig
from casparian_flow.sdk import PluginMetadata

logger = logging.getLogger(__name__)

def register_plugins_from_source(plugin_dir: Path, session: Session):
    """
    Registers plugins based on MANIFEST.subscriptions.
    Does NOT create RoutingRules (those are configured separately in DB/UI).
    """
    if not plugin_dir.exists(): return

    logger.info(f"Registering plugins from {plugin_dir}")
    if str(plugin_dir) not in sys.path:
        sys.path.insert(0, str(plugin_dir))

    for f in plugin_dir.glob("*.py"):
        if f.name.startswith("_"): continue
        plugin_name = f.stem
        
        try:
            spec = importlib.util.spec_from_file_location(plugin_name, f)
            if not spec or not spec.loader: continue
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            
            if hasattr(mod, "MANIFEST") and isinstance(mod.MANIFEST, PluginMetadata):
                meta: PluginMetadata = mod.MANIFEST
                logger.info(f"Found MANIFEST in {plugin_name}")
                
                # A. Plugin Config (Subscriptions)
                # Join all input topics
                subs_csv = ",".join(sorted(meta.subscriptions))
                
                p_conf = session.get(PluginConfig, plugin_name)
                if not p_conf:
                    session.add(PluginConfig(plugin_name=plugin_name, subscription_tags=subs_csv))
                else:
                    p_conf.subscription_tags = subs_csv
                
                # B. Topic Configs (Output Sinks)
                # Use 'sinks' dict to map output_topic -> URI
                for topic, uri in meta.sinks.items():
                    t_conf = session.query(TopicConfig).filter_by(plugin_name=plugin_name, topic_name=topic).first()
                    if not t_conf:
                        session.add(TopicConfig(
                            plugin_name=plugin_name,
                            topic_name=topic,
                            uri=uri,
                            mode="append"
                        ))
                
                # Also ensure default topics in 'subscriptions' have output sinks? 
                # No, inputs != outputs. Outputs are usually defined by what 'publish' calls are made.
                # If 'sinks' is empty, we don't pre-register outputs. Context defaults to Parquet.
            
        except Exception as e:
            logger.error(f"Failed to inspect/register {f.name}: {e}")
            
    session.commit()