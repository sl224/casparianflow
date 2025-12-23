# src/casparian_flow/services/registrar.py
import logging
import sys
import importlib.util
from pathlib import Path
from sqlalchemy.orm import Session
from casparian_flow.db.models import PluginConfig, TopicConfig, RoutingRule
from casparian_flow.sdk import PluginMetadata

logger = logging.getLogger(__name__)

def register_plugins_from_source(plugin_dir: Path, session: Session):
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

                # A. Create RoutingRule from pattern
                if meta.pattern:
                    auto_tag = f"auto_{plugin_name}"
                    existing_rule = session.query(RoutingRule).filter_by(
                        pattern=meta.pattern
                    ).first()

                    if not existing_rule:
                        session.add(RoutingRule(
                            pattern=meta.pattern,
                            tag=auto_tag,
                            priority=meta.priority or 50
                        ))
                        logger.info(f"Created RoutingRule: {meta.pattern} -> {auto_tag}")
                    else:
                        # Update existing rule
                        existing_rule.tag = auto_tag
                        existing_rule.priority = meta.priority or 50
                        logger.info(f"Updated RoutingRule: {meta.pattern} -> {auto_tag}")

                # B. Plugin Config (Subscriptions)
                # Include the auto tag in subscriptions
                auto_tag = f"auto_{plugin_name}"
                all_subs = sorted(set(meta.subscriptions) | {auto_tag})
                subs_csv = ",".join(all_subs)

                p_conf = session.get(PluginConfig, plugin_name)
                if not p_conf:
                    session.add(PluginConfig(plugin_name=plugin_name, subscription_tags=subs_csv))
                else:
                    p_conf.subscription_tags = subs_csv

                # C. Topic Configs
                # 1. From 'sinks' dict (Explicit URIs)
                for topic, uri in meta.sinks.items():
                    _upsert_topic(session, plugin_name, topic, uri)

                # 2. Create default 'output' topic config if 'topic' is specified in MANIFEST
                # This maps the default yield behavior to the named topic
                if meta.topic and "output" not in meta.sinks:
                    default_uri = f"parquet://{meta.topic}.parquet"
                    _upsert_topic(session, plugin_name, "output", default_uri)
                    logger.info(f"Created default output topic config: output -> {default_uri}")
                
                # 2. From 'subscriptions' (Implied topics? No, usually outputs are distinct)
                # But we might want defaults for topics mentioned in code? 
                # We can't know them unless declared.
                # However, for the demo, we assume the output topic matches the input subscription name or logic
                
        except Exception as e:
            logger.error(f"Failed to inspect/register {f.name}: {e}")
            
    session.commit()

def _upsert_topic(session, plugin, topic, uri):
    t_conf = session.query(TopicConfig).filter_by(plugin_name=plugin, topic_name=topic).first()
    if not t_conf:
        session.add(TopicConfig(
            plugin_name=plugin,
            topic_name=topic,
            uri=uri,
            mode="append"
        ))