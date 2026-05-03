use mailsense_core::config::Config;
use mailsense_core::domain::{EmailMessage, LlmProvider};
use mailsense_core::llm::GeminiClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 載入 .env 檔案與配置
    let config = Config::load().expect("Failed to load .env file. Please check GEMINI_API_KEY.");
    
    println!("🚀 Testing Gemini LLM Integration...");
    println!("Model: {}", config.gemini.model);
    println!("Base URL: {}", config.gemini.base_url);

    // 2. 初始化 Gemini 客戶端
    let client = GeminiClient::new(
        config.gemini.api_key,
        Some(config.gemini.model),
        Some(config.gemini.base_url),
    );

    // 3. 準備一封測試郵件
    let email = EmailMessage {
        subject: "Urgent: System Maintenance for Project MailSense".to_string(),
        from: "devops@example.com".to_string(),
        body: r#"
            Hi Team,
            
            We have a scheduled maintenance window this Friday, May 8th, at 10:00 PM UTC.
            Please ensure all background workers are paused before that.
            
            Thanks,
            DevOps Team
        "#.to_string(),
        date: "2026-05-04".to_string(),
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
