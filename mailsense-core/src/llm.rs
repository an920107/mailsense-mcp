use crate::domain::{EmailAnalysis, EmailMessage, LlmProvider};
use serde_json::json;

pub struct GeminiClient {
    api_key: String,
    model: String,
    embedding_model: String,
    base_url: String,
    max_attachment_size: usize,
    max_multimodal_parts: usize,
    client: reqwest::Client,
}

impl GeminiClient {
    pub fn new(
        api_key: String,
        model: String,
        embedding_model: String,
        base_url: Option<String>,
        max_attachment_size: usize,
        max_multimodal_parts: usize,
    ) -> Self {
        Self {
            api_key,
            model,
            embedding_model,
            base_url: base_url
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string()),
            max_attachment_size,
            max_multimodal_parts,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for GeminiClient {
    async fn analyze_email(&self, email: &EmailMessage) -> anyhow::Result<EmailAnalysis> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, self.model
        );

        let prompt = crate::prompt::generate_analysis_prompt(email);

        let mut parts = vec![json!({ "text": prompt })];

        // Add attachments if they are supported types (Multi-modal Analysis)
        use base64::prelude::*;
        let mut multimodal_count = 0;
        for attachment in &email.attachments {
            if multimodal_count >= self.max_multimodal_parts {
                tracing::warn!(
                    "Skipping attachment {} for analysis: limit of {} reached",
                    attachment.filename,
                    self.max_multimodal_parts
                );
                continue;
            }

            if attachment.data.len() > self.max_attachment_size {
                tracing::warn!(
                    "Skipping attachment {} for analysis: size {} exceeds limit of {}MB",
                    attachment.filename,
                    attachment.data.len(),
                    self.max_attachment_size / 1024 / 1024
                );
                continue;
            }

            // Skip encrypted PDFs for analysis as they can't be read by LLM
            if attachment.mime_type == "application/pdf"
                && attachment.is_encrypted
                && !attachment.is_decrypted
            {
                tracing::debug!(
                    "Skipping encrypted PDF {} for analysis",
                    attachment.filename
                );
                continue;
            }

            if attachment.mime_type.starts_with("image/")
                || attachment.mime_type == "application/pdf"
            {
                parts.push(json!({
                    "inline_data": {
                        "mime_type": attachment.mime_type,
                        "data": BASE64_STANDARD.encode(&attachment.data)
                    }
                }));
                multimodal_count += 1;
            }
        }

        let schema = json!({
            "type": "OBJECT",
            "properties": {
                "intent": {
                    "type": "STRING",
                    "enum": ["ActionRequired", "FYI", "Update", "Spam"]
                },
                "tags": {
                    "type": "ARRAY",
                    "items": { "type": "STRING" },
                    "minItems": 1,
                    "maxItems": 3
                },
                "summary": { "type": "STRING" },
                "extracted_deadlines": {
                    "type": "ARRAY",
                    "items": { "type": "STRING" }
                },
                "password_recipes": {
                    "type": "ARRAY",
                    "items": {
                        "type": "ARRAY",
                        "items": {
                            "anyOf": [
                                {
                                    "type": "OBJECT",
                                    "properties": {
                                        "type": { "type": "STRING", "enum": ["ID"] },
                                        "operation": { "type": "STRING", "enum": ["Full", "First", "Last"] },
                                        "length": {
                                            "type": "INTEGER",
                                            "description": "The number of characters to extract. Mandatory for First/Last operations."
                                        }
                                    },
                                    "required": ["type", "operation", "length"]
                                },
                                {
                                    "type": "OBJECT",
                                    "properties": {
                                        "type": { "type": "STRING", "enum": ["Bday"] },
                                        "format": { "type": "STRING", "enum": ["YYYYMMDD", "MMDD", "YYMMDD", "YYMM", "MINGUO"] }
                                    },
                                    "required": ["type", "format"]
                                },
                                {
                                    "type": "OBJECT",
                                    "properties": {
                                        "type": { "type": "STRING", "enum": ["Literal"] },
                                        "value": { "type": "STRING" }
                                    },
                                    "required": ["type", "value"]
                                }
                            ]
                        }
                    }
                }
            },
            "required": ["intent", "tags", "summary", "extracted_deadlines"]
        });

        let body = json!({
            "contents": [{
                "parts": parts
            }],
            "generationConfig": {
                "response_mime_type": "application/json",
                "response_schema": schema
            }
        });

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Gemini API error: {}", error_text));
        }

        let resp_json: serde_json::Value = response.json().await?;

        let text = resp_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| {
                anyhow::anyhow!("Invalid response structure from Gemini: {:?}", resp_json)
            })?;

        let analysis: EmailAnalysis = serde_json::from_str(text)?;
        Ok(analysis)
    }

    async fn generate_embedding(&self, email: &EmailMessage) -> anyhow::Result<Vec<f32>> {
        let url = format!(
            "{}/v1beta/models/{}:embedContent",
            self.base_url, self.embedding_model
        );

        let mut parts = vec![json!({ "text": email.to_embedding_text() })];

        // Add attachments if they are supported types
        use base64::prelude::*;
        let mut multimodal_count = 0;
        for attachment in &email.attachments {
            if multimodal_count >= self.max_multimodal_parts {
                tracing::warn!(
                    "Skipping attachment {} for embedding: limit of {} reached",
                    attachment.filename,
                    self.max_multimodal_parts
                );
                continue;
            }

            if attachment.data.len() > self.max_attachment_size {
                tracing::warn!(
                    "Skipping attachment {} for embedding: size {} exceeds limit of {}MB",
                    attachment.filename,
                    attachment.data.len(),
                    self.max_attachment_size / 1024 / 1024
                );
                continue;
            }

            // Skip encrypted PDFs for embedding
            if attachment.mime_type == "application/pdf"
                && attachment.is_encrypted
                && !attachment.is_decrypted
            {
                tracing::debug!(
                    "Skipping encrypted PDF {} for embedding",
                    attachment.filename
                );
                continue;
            }

            // Gemini embedding supports images and PDFs as multi-modal input
            if attachment.mime_type.starts_with("image/")
                || attachment.mime_type == "application/pdf"
            {
                parts.push(json!({
                    "inline_data": {
                        "mime_type": attachment.mime_type,
                        "data": BASE64_STANDARD.encode(&attachment.data)
                    }
                }));
                multimodal_count += 1;
            }
        }

        let body = json!({
            "model": format!("models/{}", self.embedding_model),
            "content": {
                "parts": parts
            },
            "output_dimensionality": 768
        });

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Gemini Embedding API error: {}",
                error_text
            ));
        }

        let resp_json: serde_json::Value = response.json().await?;
        let values = resp_json["embedding"]["values"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid embedding response: {:?}", resp_json))?;

        let embedding: Vec<f32> = values
            .iter()
            .map(|v| {
                v.as_f64()
                    .map(|f| f as f32)
                    .ok_or_else(|| anyhow::anyhow!("Non-numeric value in embedding: {:?}", v))
            })
            .collect::<anyhow::Result<Vec<f32>>>()?;

        Ok(embedding)
    }

    async fn generate_query_embedding(&self, query: &str) -> anyhow::Result<Vec<f32>> {
        let url = format!(
            "{}/v1beta/models/{}:embedContent",
            self.base_url, self.embedding_model
        );

        // Applying Gemini 2 recommendation for search queries
        let task_formatted_query = format!("task: search result | query: {}", query);

        let body = json!({
            "model": format!("models/{}", self.embedding_model),
            "content": {
                "parts": [{ "text": task_formatted_query }]
            },
            "output_dimensionality": 768
        });

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Gemini Query Embedding API error: {}",
                error_text
            ));
        }

        let resp_json: serde_json::Value = response.json().await?;
        let values = resp_json["embedding"]["values"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid embedding response: {:?}", resp_json))?;

        let embedding: Vec<f32> = values
            .iter()
            .map(|v| {
                v.as_f64()
                    .map(|f| f as f32)
                    .ok_or_else(|| anyhow::anyhow!("Non-numeric value in embedding: {:?}", v))
            })
            .collect::<anyhow::Result<Vec<f32>>>()?;

        Ok(embedding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Attachment, EmailIntent};
    use mockito::{Matcher, Server};

    #[tokio::test]
    async fn test_gemini_analysis_parsing() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let mock_body = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "text": r#"{
                            "intent": "ActionRequired",
                            "tags": ["Urgent", "Invoice"],
                            "summary": "Please pay the overdue invoice.",
                            "extracted_deadlines": ["2026-05-10T10:00:00Z"],
                            "password_recipes": [[
                                {"type": "ID", "operation": "Last", "length": 4},
                                {"type": "Bday", "format": "MMDD"}
                            ]]
                        }"#
                    }]
                }
            }]
        });

        let mock = server
            .mock(
                "POST",
                Matcher::Regex("/v1beta/models/.*:generateContent".to_string()),
            )
            .match_header("x-goog-api-key", "test-key")
            .match_body(Matcher::PartialJson(json!({
                "generationConfig": {
                    "response_mime_type": "application/json",
                    "response_schema": {
                        "type": "OBJECT",
                        "required": ["intent", "tags", "summary", "extracted_deadlines"]
                    }
                }
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_body).unwrap())
            .create_async()
            .await;

        let client = GeminiClient::new(
            "test-key".to_string(),
            "gemini-1.5-flash".to_string(),
            "text-embedding-004".to_string(),
            Some(url),
            5 * 1024 * 1024,
            3,
        );

        let email = EmailMessage {
            id: None,
            message_id: "test-id".to_string(),
            thread_id: None,
            in_reply_to: None,
            references: vec![],
            subject: "Overdue Invoice".to_string(),
            from: "billing@example.com".to_string(),
            body: "Your invoice is past due. Please pay by May 10th.".to_string(),
            date: "2026-05-04".to_string(),
            attachments: vec![],
            analysis: None,
        };

        let result = client.analyze_email(&email).await.unwrap();

        assert_eq!(result.intent, EmailIntent::ActionRequired);
        assert_eq!(result.tags, vec!["Urgent", "Invoice"]);
        assert_eq!(result.summary, "Please pay the overdue invoice.");
        assert_eq!(result.extracted_deadlines.len(), 1);

        let recipes = result.password_recipes.unwrap();
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].len(), 2);

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_gemini_embedding_parsing() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let mock_body = json!({
            "embedding": {
                "values": [0.1, 0.2, 0.3]
            }
        });

        let mock = server
            .mock("POST", "/v1beta/models/text-embedding-004:embedContent")
            .match_header("x-goog-api-key", "test-key")
            .match_body(Matcher::PartialJson(json!({
                "model": "models/text-embedding-004",
                "content": {
                    "parts": [{ "text": "title: hello world | text: From: Unknown\nBody: " }]
                }
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_body).unwrap())
            .create_async()
            .await;

        let client = GeminiClient::new(
            "test-key".to_string(),
            "gemini-1.5-flash".to_string(),
            "text-embedding-004".to_string(),
            Some(url),
            5 * 1024 * 1024,
            3,
        );

        let email = EmailMessage {
            id: None,
            message_id: "test-id".to_string(),
            thread_id: None,
            in_reply_to: None,
            references: vec![],
            subject: "hello world".to_string(),
            from: "Unknown".to_string(),
            body: "".to_string(),
            date: "2026-05-04".to_string(),
            attachments: vec![],
            analysis: None,
        };

        let result = client.generate_embedding(&email).await.unwrap();

        assert_eq!(result, vec![0.1, 0.2, 0.3]);

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_gemini_multi_modal_embedding() {
        let mut server = Server::new_async().await;
        let url = server.url();

        let mock_body = json!({
            "embedding": {
                "values": [0.5, 0.6]
            }
        });

        let mock = server
            .mock("POST", "/v1beta/models/text-embedding-004:embedContent")
            .match_header("x-goog-api-key", "test-key")
            .match_body(Matcher::Json(json!({
                "model": "models/text-embedding-004",
                "content": {
                    "parts": [
                        { "text": "title: img test | text: From: me\nBody: look at this" },
                        {
                            "inline_data": {
                                "mime_type": "image/png",
                                "data": "AQID" // [1, 2, 3] in base64
                            }
                        }
                    ]
                },
                "output_dimensionality": 768
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_body).unwrap())
            .create_async()
            .await;

        let client = GeminiClient::new(
            "test-key".to_string(),
            "gemini-1.5-flash".to_string(),
            "text-embedding-004".to_string(),
            Some(url),
            5 * 1024 * 1024,
            3,
        );

        let email = EmailMessage {
            id: None,
            message_id: "test-id".to_string(),
            thread_id: None,
            in_reply_to: None,
            references: vec![],
            subject: "img test".to_string(),
            from: "me".to_string(),
            body: "look at this".to_string(),
            date: "2026-05-04".to_string(),
            attachments: vec![Attachment {
                filename: "test.png".to_string(),
                mime_type: "image/png".to_string(),
                data: vec![1, 2, 3],
                is_encrypted: false,
                is_decrypted: false,
                decryption_error: None,
            }],
            analysis: None,
        };

        let result = client.generate_embedding(&email).await.unwrap();

        assert_eq!(result, vec![0.5, 0.6]);

        mock.assert_async().await;
    }
}
