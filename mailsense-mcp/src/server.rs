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
                            description: "Search emails using semantic similarity and keyword matching, with optional intent filtering. Returns detailed analysis including summaries and deadlines.".to_string(),
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
                            description: "List all attachments for a specific email by its Message-ID. Returns attachment IDs for reading specific content.".to_string(),
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
                            description: "Read the content of a specific attachment using its Attachment ID. Returns text for documents or base64 for images/PDFs.".to_string(),
                            input_schema: json!({
                                "type": "object",
                                "properties": {
                                    "attachment_id": {
                                        "type": "string",
                                        "description": "The stable ID of the attachment (UUID)"
                                    }
                                },
                                "required": ["attachment_id"]
                            }),
                        },
                        Tool {
                            name: "mailsense_get_email".to_string(),
                            description: "Retrieve the full content, metadata, and LLM analysis of a specific email by its Message-ID.".to_string(),
                            input_schema: json!({
                                "type": "object",
                                "properties": {
                                    "message_id": {
                                        "type": "string",
                                        "description": "The Message-ID of the email to retrieve"
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
                let params_res: Result<CallToolParams, _> =
                    serde_json::from_value(req.params.unwrap_or_default());

                match params_res {
                    Ok(params) => {
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
                    Err(e) => Some(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(crate::protocol::JsonRpcError {
                            code: -32602,
                            message: format!("Invalid params: {}", e),
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

                let intent = if let Some(i) = params.arguments.get("intent") {
                    let s = i
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("'intent' must be a string"))?;
                    match s {
                        "ActionRequired" => {
                            Some(mailsense_core::domain::EmailIntent::ActionRequired)
                        }
                        "FYI" => Some(mailsense_core::domain::EmailIntent::FYI),
                        "Update" => Some(mailsense_core::domain::EmailIntent::Update),
                        "Spam" => Some(mailsense_core::domain::EmailIntent::Spam),
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Unknown intent '{}'. Allowed values: ActionRequired, FYI, Update, Spam",
                                s
                            ));
                        }
                    }
                } else {
                    None
                };

                let embedding = self.llm.generate_query_embedding(query).await?;
                let results = self
                    .storage
                    .hybrid_search(query, Some(embedding), intent, limit)
                    .await?;

                let json_results = json!({
                    "count": results.len(),
                    "emails": results.iter().map(|res| {
                        json!({
                            "message_id": res.message_id,
                            "system_id": res.id,
                            "from": res.from,
                            "subject": res.subject,
                            "date": res.date,
                            "analysis": res.analysis.as_ref().map(|a| {
                                json!({
                                    "intent": a.intent.as_str(),
                                    "summary": a.summary,
                                    "deadlines": a.extracted_deadlines
                                })
                            }),
                            "preview": res.body.chars().take(300).collect::<String>()
                        })
                    }).collect::<Vec<_>>()
                });

                Ok(CallToolResult {
                    content: vec![ToolContent::Json { json: json_results }],
                    is_error: false,
                })
            }
            "mailsense_list_attachments" => {
                let message_id = params.arguments["message_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'message_id' argument"))?;

                let attachments = self
                    .storage
                    .get_attachment_metadata_by_message_id(message_id)
                    .await?;

                let json_results = json!({
                    "count": attachments.len(),
                    "attachments": attachments.iter().map(|att| {
                        json!({
                            "attachment_id": att.id,
                            "filename": att.filename,
                            "mime_type": att.mime_type,
                            "status": if att.is_decrypted {
                                "Decrypted"
                            } else if att.is_encrypted {
                                "Encrypted (Decryption Failed)"
                            } else {
                                "No Encryption"
                            }
                        })
                    }).collect::<Vec<_>>()
                });

                Ok(CallToolResult {
                    content: vec![ToolContent::Json { json: json_results }],
                    is_error: false,
                })
            }
            "mailsense_read_attachment" => {
                let attachment_id_str = params.arguments["attachment_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'attachment_id' argument"))?;
                let attachment_id = uuid::Uuid::parse_str(attachment_id_str)
                    .map_err(|e| anyhow::anyhow!("Invalid 'attachment_id' UUID: {}", e))?;

                let attachment_opt = self.storage.get_attachment_by_id(attachment_id).await?;

                if let Some(att) = attachment_opt {
                    let json_res = if att.mime_type.starts_with("text/") {
                        json!({
                            "filename": att.filename,
                            "mime_type": att.mime_type,
                            "content": String::from_utf8_lossy(&att.data).to_string(),
                            "encoding": "text"
                        })
                    } else {
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&att.data);
                        json!({
                            "filename": att.filename,
                            "mime_type": att.mime_type,
                            "content": b64,
                            "encoding": "base64"
                        })
                    };

                    Ok(CallToolResult {
                        content: vec![ToolContent::Json { json: json_res }],
                        is_error: false,
                    })
                } else {
                    Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: format!("Attachment with ID '{}' not found.", attachment_id),
                        }],
                        is_error: true,
                    })
                }
            }
            "mailsense_get_email" => {
                let message_id = params.arguments["message_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'message_id' argument"))?;

                let email_opt = self.storage.get_email_by_id(message_id).await?;

                if let Some(res) = email_opt {
                    let json_res = json!({
                        "message_id": res.message_id,
                        "system_id": res.id,
                        "from": res.from,
                        "subject": res.subject,
                        "date": res.date,
                        "body": res.body,
                        "analysis": res.analysis.as_ref().map(|a| {
                            json!({
                                "intent": a.intent.as_str(),
                                "summary": a.summary,
                                "deadlines": a.extracted_deadlines
                            })
                        })
                    });

                    Ok(CallToolResult {
                        content: vec![ToolContent::Json { json: json_res }],
                        is_error: false,
                    })
                } else {
                    Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: format!("Email with Message-ID '{}' not found.", message_id),
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
