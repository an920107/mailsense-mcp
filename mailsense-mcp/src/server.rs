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
                            name: "mailsense_search_emails".to_string(),
                            description: "Search emails using semantic similarity and keyword matching, with optional intent filtering. Returns detailed analysis including summaries, deadlines, and password recipes.".to_string(),
                            input_schema: json!({
                                "type": "object",
                                "properties": {
                                    "query": {
                                        "type": "string",
                                        "description": "The search query (natural language or keywords)"
                                    },
                                    "intent": {
                                        "type": "string",
                                        "description": "Optional filter by intent (ActionRequired, FYI, Update, Spam)"
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
                            name: "mailsense_list_attachments".to_string(),
                            description: "List all attachments for a specific email by its Message-ID.".to_string(),
                            input_schema: json!({
                                "type": "object",
                                "properties": {
                                    "message_id": {
                                        "type": "string",
                                        "description": "The Message-ID of the email"
                                    }
                                },
                                "required": ["message_id"]
                            }),
                        },
                        Tool {
                            name: "mailsense_read_attachment".to_string(),
                            description: "Read the content of a specific attachment. Returns text for documents or base64 for images.".to_string(),
                            input_schema: json!({
                                "type": "object",
                                "properties": {
                                    "message_id": {
                                        "type": "string",
                                        "description": "The Message-ID of the email"
                                    },
                                    "filename": {
                                        "type": "string",
                                        "description": "The filename of the attachment to read"
                                    }
                                },
                                "required": ["message_id", "filename"]
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
            "mailsense_search_emails" => {
                let query = params.arguments["query"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;
                let limit = params.arguments["limit"].as_u64().unwrap_or(5) as u32;

                let intent = params
                    .arguments
                    .get("intent")
                    .and_then(|i| i.as_str())
                    .and_then(|s| match s {
                        "ActionRequired" => {
                            Some(mailsense_core::domain::EmailIntent::ActionRequired)
                        }
                        "FYI" => Some(mailsense_core::domain::EmailIntent::FYI),
                        "Update" => Some(mailsense_core::domain::EmailIntent::Update),
                        "Spam" => Some(mailsense_core::domain::EmailIntent::Spam),
                        _ => None,
                    });

                let embedding = self.llm.generate_query_embedding(query).await?;
                let results = self
                    .storage
                    .hybrid_search(query, Some(embedding), intent, limit)
                    .await?;

                let mut text = format!("Found {} results:\n\n", results.len());
                for res in results {
                    let mut analysis_text = String::new();
                    if let Some(analysis) = &res.analysis {
                        analysis_text
                            .push_str(&format!("  [Intent]: {}\n", analysis.intent.as_str()));
                        analysis_text.push_str(&format!("  [Summary]: {}\n", analysis.summary));
                        if !analysis.extracted_deadlines.is_empty() {
                            analysis_text.push_str(&format!(
                                "  [Deadlines]: {}\n",
                                analysis.extracted_deadlines.join(", ")
                            ));
                        }
                    }

                    text.push_str(&format!(
                        "--- \nMessage-ID: {}\nSystem-ID: {}\nFrom: {}\nSubject: {}\nDate: {}\nAnalysis:\n{}\nPreview: {}\n\n",
                        res.message_id,
                        res.id.map(|u| u.to_string()).unwrap_or_else(|| "N/A".to_string()),
                        res.from,
                        res.subject,
                        res.date,
                        if analysis_text.is_empty() { "  (None)\n".to_string() } else { analysis_text },
                        res.body.chars().take(200).collect::<String>()
                    ));
                }

                Ok(CallToolResult {
                    content: vec![ToolContent::Text { text }],
                    is_error: false,
                })
            }
            "mailsense_list_attachments" => {
                let message_id = params.arguments["message_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'message_id' argument"))?;

                let attachments = self
                    .storage
                    .get_attachments_by_message_id(message_id)
                    .await?;

                let mut text = format!("Found {} attachments:\n\n", attachments.len());
                for (i, att) in attachments.iter().enumerate() {
                    let status = if att.is_decrypted {
                        "Decrypted"
                    } else if att.is_encrypted {
                        "Encrypted (Failed)"
                    } else {
                        "No Encryption"
                    };
                    text.push_str(&format!(
                        "{}. {} (MIME: {}) - Status: {}\n",
                        i + 1,
                        att.filename,
                        att.mime_type,
                        status
                    ));
                }

                Ok(CallToolResult {
                    content: vec![ToolContent::Text { text }],
                    is_error: false,
                })
            }
            "mailsense_read_attachment" => {
                let message_id = params.arguments["message_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'message_id' argument"))?;
                let filename = params.arguments["filename"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'filename' argument"))?;

                let attachments = self
                    .storage
                    .get_attachments_by_message_id(message_id)
                    .await?;
                let attachment = attachments.into_iter().find(|a| a.filename == filename);

                if let Some(att) = attachment {
                    let content = if att.mime_type.starts_with("text/") {
                        ToolContent::Text {
                            text: String::from_utf8_lossy(&att.data).to_string(),
                        }
                    } else {
                        // For images or PDFs, return as base64 (future enhancement: specialized data types if needed)
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&att.data);
                        ToolContent::Text {
                            text: format!("(Binary data encoded in Base64)\n{}", b64),
                        }
                    };

                    Ok(CallToolResult {
                        content: vec![content],
                        is_error: false,
                    })
                } else {
                    Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: format!("Attachment '{}' not found.", filename),
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
