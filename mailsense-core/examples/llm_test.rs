use mailsense_core::domain::{EmailMessage, LlmProvider};
use mailsense_core::llm::GeminiClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 載入 .env 檔案與配置
    // 我們使用手動加載來避免因為缺少 IMAP/DB 配置而失敗，這解決了 Copilot 的 Review 意見。
    dotenvy::dotenv().ok();

    let api_key = std::env::var("GEMINI_API_KEY")
        .expect("GEMINI_API_KEY must be set in .env for this example.");
    let model = std::env::var("GEMINI_MODEL").expect("GEMINI_MODEL must be set");
    let embedding_model =
        std::env::var("GEMINI_EMBEDDING_MODEL").expect("GEMINI_EMBEDDING_MODEL must be set");
    let base_url = std::env::var("GEMINI_BASE_URL").ok();

    println!("🚀 Testing Gemini LLM Integration...");
    println!("Model: {}", model);

    // 2. 初始化 Gemini 客戶端
    let client = GeminiClient::new(api_key, model, embedding_model, base_url);

    // 3. 準備一封測試郵件
    let email = EmailMessage {
        message_id: "test-id-123".to_string(),
        thread_id: None,
        in_reply_to: None,
        references: vec![],
        subject: "Urgent: System Maintenance for Project MailSense".to_string(),
        from: "devops@example.com".to_string(),
        body: r#"
            Hi Team,

            We have a scheduled maintenance window this Friday, May 8th, at 10:00 PM UTC.
            Please ensure all background workers are paused before that.

            Thanks,
            DevOps Team
        "#
        .to_string(),
        date: "2026-05-04T10:00:00Z".to_string(),
        attachments: vec![],
        analysis: None,
    };

    println!("\n📧 Analyzing Email...");
    println!("-------------------");
    println!("Subject: {}", email.subject);

    // 4. 呼叫 LLM
    match client.analyze_email(&email).await {
        Ok(analysis) => {
            println!("\n✅ Success! Analysis Result:");
            println!("-------------------");
            println!("Intent: {:?}", analysis.intent);
            println!("Tags: {:?}", analysis.tags);
            println!("Summary: {}", analysis.summary);
            println!("Deadlines: {:?}", analysis.extracted_deadlines);
        }
        Err(e) => {
            eprintln!("\n❌ LLM Analysis Failed!");
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}
