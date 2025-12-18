import pytest
from pathlib import Path
from casparian_flow.sdk import PluginMetadata
from casparian_flow.services.registrar import register_plugins_from_source
from casparian_flow.db.models import RoutingRule, PluginConfig

@pytest.fixture
def plugin_dir(tmp_path):
    d = tmp_path / "plugins"
    d.mkdir()
    
    # Create a DOD plugin
    p = d / "dod_plugin.py"
    p.write_text("""
from casparian_flow.sdk import BasePlugin, PluginMetadata

MANIFEST = PluginMetadata(
    pattern="finance/*.csv",
    topic="finance_data",
    version="2.0"
)

class Handler(BasePlugin):
    def execute(self, path): pass
""")
    return d

def test_registration_reads_manifest(test_db_session, plugin_dir):
    register_plugins_from_source(plugin_dir, test_db_session)
    
    # Check Rule
    rule = test_db_session.query(RoutingRule).filter_by(pattern="finance/*.csv").first()
    assert rule is not None
    assert rule.tag == "auto_dod_plugin"
    
    # Check Plugin Config
    p_conf = test_db_session.get(PluginConfig, "dod_plugin")
    assert p_conf is not None
    assert "auto_dod_plugin" in p_conf.subscription_tags