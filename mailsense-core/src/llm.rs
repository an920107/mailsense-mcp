use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use crate::domain::{EmailMessage, EmailAnalysis, LlmProvider};
use crate::prompt::SYSTEM_INSTRUCTIONS;
use std::time::Duration;

pub struct GeminiClient {
    api_key: String,
    model: String,
    client: Client,
    base_url: String,
}

impl GeminiClient {
    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| "gemini-2.0-flash".to_string()),
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            base_url: base_url.unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string()),
        }
    }
}

#[async_trait]
impl LlmProvider for GeminiClient {
    async fn analyze_email(&self, email: &EmailMessage) -> anyhow::Result<EmailAnalysis> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, self.model
        );

        // Security: Mitigate prompt injection by clearly separating instructions from data
        // and including the message timestamp for relative date resolution.
        let prompt = format!(
            "{}\n\n[UNTRUSTED EMAIL DATA START]\nDate: {}\nSubject: {}\nFrom: {}\nBody:\n{}\n[UNTRUSTED EMAIL DATA END]",
            SYSTEM_INSTRUCTIONS, email.date, email.subject, email.from, email.body
        );

        // Gemini JSON Schema definition for Structured Outputs
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
                    "items": { "type": "STRING" },
                    "description": "ISO 8601 formatted date-time strings if available, or just dates."
                }
            },
            "required": ["intent", "tags", "summary", "extracted_deadlines"]
        });

        let body = json!({
            "contents": [{
                "parts": [{ "text": prompt }]
            }],
            "generationConfig": {
                "response_mime_type": "application/json",
                "response_schema": schema
            }
        });

        let response = self.client.post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Gemini API error: {}", error_text));
        }

        let resp_json: serde_json::Value = response.json().await?;
        
        // Gemini response structure: candidates[0].content.parts[0].text
        let text = resp_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid response structure from Gemini: {:?}", resp_json))?;

        let analysis: EmailAnalysis = serde_json::from_str(text)?;
        Ok(analysis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Server, Matcher};
    use crate::domain::EmailIntent;

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
                            "extracted_deadlines": ["2026-05-10T10:00:00Z"]
                        }"#
                    }]
                }
            }]
        });

        let mock = server.mock("POST", Matcher::Regex("/v1beta/models/.*:generateContent".to_string()))
            .match_header("x-goog-api-key", "test-key")
            // We use Matcher::PartialJson to validate the structure without worrying about the exact dynamic prompt text
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

        let client = GeminiClient::new("test-key".to_string(), None, Some(url));

        let email = EmailMessage {
            subject: "Overdue Invoice".to_string(),
            from: "billing@example.com".to_string(),
            body: "Your invoice is past due. Please pay by May 10th.".to_string(),
            date: "2026-05-04".to_string(),
        };

        let result = client.analyze_email(&email).await.unwrap();

        assert_eq!(result.intent, EmailIntent::ActionRequired);
        assert_eq!(result.tags, vec!["Urgent", "Invoice"]);
        assert_eq!(result.summary, "Please pay the overdue invoice.");
        assert_eq!(result.extracted_deadlines.len(), 1);
        
        mock.assert_async().await;
    }
}
