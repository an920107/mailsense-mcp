use crate::domain::{EmailAnalysis, EmailMessage, LlmProvider};
use serde_json::json;

pub struct GeminiClient {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl GeminiClient {
    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| "gemini-1.5-flash".to_string()),
            base_url: base_url
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string()),
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
                            "type": "OBJECT",
                            "properties": {
                                "type": { "type": "STRING", "enum": ["ID", "Bday", "Literal"] },
                                "operation": { "type": "STRING", "enum": ["Full", "First", "Last"] },
                                "length": {
                                    "type": "INTEGER",
                                    "description": "Required for First/Last operations. The number of characters to extract."
                                },
                                "format": { "type": "STRING", "enum": ["YYYYMMDD", "MMDD", "YYMMDD", "YYMM", "MINGUO"] },
                                "value": { "type": "STRING" }
                            },
                            "required": ["type"]
                        }
                    }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::EmailIntent;
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

        let recipes = result.password_recipes.unwrap();
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].len(), 2);

        mock.assert_async().await;
    }
}
