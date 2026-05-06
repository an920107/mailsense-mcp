use crate::protocol::{
    CallToolParams, CallToolResult, InitializeParams, InitializeResult, JsonRpcRequest,
    JsonRpcResponse, ListToolsResult, ServerCapabilities, ServerInfo, Tool, ToolContent,
};
use anyhow::Result;
use mailsense_core::domain::{LlmProvider, StorageProvider};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info};

pub struct McpServer {
    pub name: String,
    pub version: String,
    storage: Arc<dyn StorageProvider>,
    llm: Arc<dyn LlmProvider>,
}

impl McpServer {
    pub fn new(
        name: &str,
        version: &str,
        storage: Arc<dyn StorageProvider>,
        llm: Arc<dyn LlmProvider>,
    ) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            storage,
            llm,
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
                let params: InitializeParams =
                    serde_json::from_value(req.params.unwrap_or_default()).ok()?;
                info!(
                    "Client connected: {} (v{})",
                    params.client_info.name, params.client_info.version
                );

                let result = InitializeResult {
                    protocol_version: params.protocol_version,
                    capabilities: ServerCapabilities {
                        tools: Some(json!({ "listChanged": false })),
                        ..Default::default()
                    },
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
            "tools/list" => {
                let result = ListToolsResult {
                    tools: vec![
                        Tool {
                            name: "mailsense_hybrid_search".to_string(),
                            description: "Search emails using a combination of vector similarity and keyword matching.".to_string(),
                            input_schema: json!({
                                "type": "object",
                                "properties": {
                                    "query": {
                                        "type": "string",
                                        "description": "The search query (natural language or keywords)"
                                    },
                                    "limit": {
                                        "type": "integer",
                                        "description": "Maximum number of results to return",
                                        "default": 5
                                    }
                                },
                                "required": ["query"]
                            }),
                        },
                        Tool {
                            name: "mailsense_analyze_email".to_string(),
                            description: "Analyze a specific email by its Message-ID to extract intent, summary, deadlines, and password recipes.".to_string(),
                            input_schema: json!({
                                "type": "object",
                                "properties": {
                                    "message_id": {
                                        "type": "string",
                                        "description": "The Message-ID of the email to analyze"
                                    }
                                },
                                "required": ["message_id"]
                            }),
                        },
                    ],
                };

                Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(serde_json::to_value(result).unwrap()),
                    error: None,
                })
            }
            "tools/call" => {
                let params: CallToolParams =
                    serde_json::from_value(req.params.unwrap_or_default()).ok()?;
                let result = self.handle_tool_call(params).await;

                match result {
                    Ok(res) => Some(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: Some(serde_json::to_value(res).unwrap()),
                        error: None,
                    }),
                    Err(e) => Some(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(crate::protocol::JsonRpcError {
                            code: -32603,
                            message: e.to_string(),
                            data: None,
                        }),
                    }),
                }
            }
            _ => {
                debug!("Received unknown method: {}", req.method);
                None
            }
        }
    }

    async fn handle_tool_call(&self, params: CallToolParams) -> Result<CallToolResult> {
        match params.name.as_str() {
            "mailsense_hybrid_search" => {
                let query = params.arguments["query"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;
                let limit = params.arguments["limit"].as_u64().unwrap_or(5) as u32;

                let embedding = self.llm.generate_query_embedding(query).await?;
                let results = self
                    .storage
                    .hybrid_search(query, Some(embedding), limit)
                    .await?;

                let mut text = format!("Found {} results:\n\n", results.len());
                for res in results {
                    text.push_str(&format!(
                        "--- \nID: {}\nFrom: {}\nSubject: {}\nDate: {}\nSummary Preview: {}\n\n",
                        res.message_id,
                        res.from,
                        res.subject,
                        res.date,
                        res.body.chars().take(200).collect::<String>()
                    ));
                }

                Ok(CallToolResult {
                    content: vec![ToolContent::Text { text }],
                    is_error: false,
                })
            }
            "mailsense_analyze_email" => {
                let message_id = params.arguments["message_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'message_id' argument"))?;

                let email_opt = self.storage.get_email_by_id(message_id).await?;
                if let Some(email) = email_opt {
                    let analysis = self.llm.analyze_email(&email).await?;
                    let mut text = format!("Analysis for Email: {}\n", email.subject);
                    text.push_str(&format!("Intent: {}\n", analysis.intent.as_str()));
                    text.push_str(&format!("Summary: {}\n", analysis.summary));
                    text.push_str(&format!(
                        "Deadlines: {}\n",
                        analysis.extracted_deadlines.join(", ")
                    ));
                    if let Some(recipes) = analysis.password_recipes {
                        text.push_str(&format!("Password Recipes found: {}\n", recipes.len()));
                    }

                    Ok(CallToolResult {
                        content: vec![ToolContent::Text { text }],
                        is_error: false,
                    })
                } else {
                    Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: "Email not found.".to_string(),
                        }],
                        is_error: true,
                    })
                }
            }
            _ => Err(anyhow::anyhow!("Unknown tool: {}", params.name)),
        }
    }
}

impl ServerCapabilities {}
