# src/casparian_flow/services/registrar.py
import re
from pathlib import Path
from sqlalchemy.orm import Session
from casparian_flow.db.models import RoutingRule, PluginConfig, TopicConfig

def register_plugins_from_source(plugin_dir: Path, session: Session):
    """
    Scans source code for magic comments and registers configuration in DB.
    Pattern:
      # PATTERN: *.csv
      # TOPIC: output_table
    """
    for f in plugin_dir.glob("*.py"):
        if f.name.startswith("_"): continue
        
        content = f.read_text("utf-8")
        plugin_name = f.stem
        
        pattern_m = re.search(r'#\s*PATTERN:\s*(.+)', content)
        topic_m = re.search(r'#\s*TOPIC:\s*(.+)', content)
        
        if pattern_m:
            pattern = pattern_m.group(1).strip()
            tag = f"auto_{plugin_name}"
            
            # Upsert Routing Rule
            existing = session.query(RoutingRule).filter_by(pattern=pattern).first()
            if not existing:
                session.add(RoutingRule(pattern=pattern, tag=tag, priority=10))
                
            # Upsert Plugin Config
            p_conf = session.get(PluginConfig, plugin_name)
            if not p_conf:
                session.add(PluginConfig(plugin_name=plugin_name, subscription_tags=tag))
            else:
                if tag not in p_conf.subscription_tags:
                    p_conf.subscription_tags += f",{tag}"
        
        if topic_m:
            topic = topic_m.group(1).strip()
            # Upsert Topic Config (Default to Parquet)
            t_conf = session.query(TopicConfig).filter_by(plugin_name=plugin_name, topic_name="output").first()
            if not t_conf:
                session.add(TopicConfig(
                    plugin_name=plugin_name,
                    topic_name="output",
                    uri=f"parquet://{topic}",
                    mode="append"
                ))
    
    session.commit()