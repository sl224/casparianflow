//! casparian_plugins - List Available Parsers/Plugins
//!
//! Returns information about registered parsers and plugins.

use super::McpTool;
use crate::approvals::ApprovalManager;
use crate::jobs::JobManager;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct PluginsTool;

#[derive(Debug, Deserialize)]
struct PluginsArgs {
    #[serde(default)]
    include_dev: bool,
}

#[derive(Debug, Serialize)]
struct PluginInfo {
    id: String,
    version: String,
    runtime: String,
    outputs: Vec<String>,
    topics: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PluginsResult {
    plugins: Vec<PluginInfo>,
}

#[async_trait::async_trait]
impl McpTool for PluginsTool {
    fn name(&self) -> &'static str {
        "casparian_plugins"
    }

    fn description(&self) -> &'static str {
        "List available parsers/plugins"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "include_dev": {
                    "type": "boolean",
                    "default": false,
                    "description": "Include path-based dev plugins"
                }
            }
        })
    }

    async fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _jobs: &Arc<Mutex<JobManager>>,
        _approvals: &Arc<Mutex<ApprovalManager>>,
        _config: &McpServerConfig,
    ) -> Result<Value> {
        let _args: PluginsArgs = serde_json::from_value(args).unwrap_or(PluginsArgs {
            include_dev: false,
        });

        // TODO: Query actual plugin registry from casparian storage
        // For now, return a placeholder with the native EVTX parser

        let plugins = vec![
            PluginInfo {
                id: "evtx_native".to_string(),
                version: "0.1.0".to_string(),
                runtime: "native".to_string(),
                outputs: vec!["evtx_events".to_string(), "evtx_eventdata_kv".to_string()],
                topics: vec!["evtx".to_string()],
            },
        ];

        let result = PluginsResult { plugins };
        Ok(serde_json::to_value(result)?)
    }
}
