use axum::Router;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities,
        ServerConfig,
    },
    schemars,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use std::sync::{Arc, RwLock};
use tokio_util::sync::CancellationToken;

use crate::{ChartsHashMaps, ChartsHost, lookup_charts};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetChartsRequest {
    #[schemars(description = "Airport identifier code (ICAO or FAA)")]
    airport: String,
}

#[derive(Clone)]
pub struct Server {
    hashmaps: Arc<RwLock<ChartsHashMaps>>,
}

#[tool_router]
impl Server {
    pub const fn from_hashmap(hashmaps: Arc<RwLock<ChartsHashMaps>>) -> Self {
        Self { hashmaps }
    }

    #[tool(description = "Retrieve airport reference chart metadata for a given airport")]
    fn get_charts(
        &self,
        Parameters(GetChartsRequest { airport }): Parameters<GetChartsRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(
            lookup_charts(&airport.to_uppercase(), ChartsHost::Faa, &self.hashmaps).map_or_else(
                || CallToolResult::error(vec![ContentBlock::text("Not found")]),
                |dto_vec| {
                    let contents = dto_vec
                        .into_iter()
                        .map(|dto| ContentBlock::text(format!("{dto:?}")))
                        .collect();
                    CallToolResult::success(contents)
                },
            ),
        )
    }
}

#[tool_handler]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_server_info(Implementation::new("chartsapi-rs", "0.1.1"))
            .with_instructions(
                "Use this Server to query FAA airport charts for a given airport.".to_string(),
            )
    }
}

fn allowed_hosts() -> Vec<String> {
    match std::env::var("MCP_ALLOWED_HOSTS") {
        Ok(value) => value
            .split(',')
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .map(str::to_owned)
            .collect(),
        Err(_) => vec![
            "localhost".to_owned(),
            "127.0.0.1".to_owned(),
            "::1".to_owned(),
        ],
    }
}

pub fn get_router(hashmaps: Arc<RwLock<ChartsHashMaps>>) -> (CancellationToken, Router) {
    let ct = CancellationToken::new();
    let service = StreamableHttpService::new(
        move || Ok(Server::from_hashmap(hashmaps.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_cancellation_token(ct.child_token())
            .with_allowed_hosts(allowed_hosts()),
    );
    (ct, Router::new().nest_service("/mcp", service))
}
