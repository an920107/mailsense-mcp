use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use crate::domain::{EmailMessage, EmailAnalysis, LlmProvider};
use crate::prompt::SYSTEM_INSTRUCTIONS;

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
            client: Client::new(),
            base_url: base_url.unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string()),
        }
    }
}

#[async_trait]
impl LlmProvider for GeminiClient {
    async fn analyze_email(&self, email: &EmailMessage) -> anyhow::Result<EmailAnalysis> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let prompt = format!(
            "{}\n\nEmail Subject: {}\nEmail From: {}\nEmail Body:\n{}",
            SYSTEM_INSTRUCTIONS, email.subject, email.from, email.body
        );

        // Gemini JSON Schema definition for Structured Outputs
        let schema = json!({
            "type": "object",
            "properties": {
                "intent": {
                    "type": "string",
                    "enum": ["ActionRequired", "FYI", "Update", "Spam"]
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "maxItems": 3
                },
                "summary": { "type": "string" },
                "extracted_deadlines": {
                    "type": "array",
                    "items": { "type": "string", "format": "date-time" }
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
    use mockito::Server;
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

        let mock = server.mock("POST", "/v1beta/models/gemini-2.0-flash:generateContent?key=test-key")
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
