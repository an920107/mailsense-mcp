use crate::protocol::{InitializeParams, InitializeResult, JsonRpcRequest, JsonRpcResponse, ServerCapabilities, ServerInfo};
use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info, debug};

pub struct McpServer {
    pub name: String,
    pub version: String,
}

impl McpServer {
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting MCP server: {} (v{})", self.name, self.version);
        
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin).lines();
        let mut stdout = tokio::io::stdout();

        while let Some(line) = reader.next_line().await? {
            debug!("Received message: {}", line);
            
            match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(req) => {
                    if let Some(resp) = self.handle_request(req).await {
                        let resp_json = serde_json::to_string(&resp)? + "\n";
                        stdout.write_all(resp_json.as_bytes()).await?;
                        stdout.flush().await?;
                    }
                }
                Err(e) => {
                    error!("Failed to parse JSON-RPC request: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn handle_request(&self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        match req.method.as_str() {
            "initialize" => {
                let params: InitializeParams = serde_json::from_value(req.params.unwrap_or_default()).ok()?;
                info!("Client connected: {} (v{})", params.client_info.name, params.client_info.version);
                
                let result = InitializeResult {
                    protocol_version: params.protocol_version,
                    capabilities: ServerCapabilities::default(),
                    server_info: ServerInfo {
                        name: self.name.clone(),
                        version: self.version.clone(),
                    },
                };

                Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(serde_json::to_value(result).unwrap()),
                    error: None,
                })
            }
            _ => {
                debug!("Received unknown method: {}", req.method);
                None
            }
        }
    }
}
