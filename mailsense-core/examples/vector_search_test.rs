use mailsense_core::config::Config;
use mailsense_core::domain::{EmailMessage, LlmProvider, StorageProvider};
use mailsense_core::llm::GeminiClient;
use mailsense_core::storage::PgStorage;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 載入配置
    dotenvy::dotenv().ok();
    let config = Config::load().expect("Failed to load .env");

    let gemini_cfg = config.gemini.as_ref().expect("GEMINI_API_KEY missing");
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL missing");

    // 2. 初始化組件
    let client = GeminiClient::new(
        gemini_cfg.api_key.clone(),
        gemini_cfg.model.clone(),
        gemini_cfg.embedding_model.clone(),
        Some(gemini_cfg.base_url.clone()),
    );
    let storage = PgStorage::connect(&database_url).await?;

    println!("🚀 Testing Vector Search & Threading Pipeline...");

    // 3. 準備模擬數據 (Thread: Project Alpha)
    let email1 = EmailMessage {
        message_id: format!("msg-1-{}", Uuid::new_v4()),
        thread_id: None,
        in_reply_to: None,
        references: vec![],
        subject: "Project Alpha Kickoff".to_string(),
        from: "manager@example.com".to_string(),
        body: "Let's start the new project focused on Rust and MCP integration.".to_string(),
        date: "2026-05-01T10:00:00Z".to_string(),
    };

    let email2 = EmailMessage {
        message_id: format!("msg-2-{}", Uuid::new_v4()),
        thread_id: None,
        in_reply_to: Some(email1.message_id.clone()),
        references: vec![email1.message_id.clone()],
        subject: "Re: Project Alpha Kickoff".to_string(),
        from: "dev@example.com".to_string(),
        body: "I will setup the basic workspace using Cargo Workspaces.".to_string(),
        date: "2026-05-01T11:00:00Z".to_string(),
    };

    let emails = vec![email1, email2];

    // 4. 生成 Embedding 並儲存
    println!("\n📥 Phase 1: Embedding & Storage...");
    for email in &emails {
        print!("Processing: {}... ", email.subject);
        let embedding_text = email.to_embedding_text();
        let embedding = client.generate_embedding(&embedding_text).await?;

        // 簡單的 Thread ID 邏輯：根郵件的 ID
        let thread_id = email
            .in_reply_to
            .clone()
            .unwrap_or(email.message_id.clone());

        storage
            .store_email_document(email, &thread_id, Some(embedding))
            .await?;
        println!("✅ Stored.");
    }

    // 5. 測試混合搜索
    println!("\n🔍 Phase 2: Hybrid Search Testing...");

    let test_queries = vec![
        "Cargo Workspaces in Rust", // 語義相關 (Semantic)
        "Kickoff",                  // 關鍵字匹配 (Keyword)
        "integration",              // 混合 (Hybrid)
    ];

    for query in test_queries {
        println!("\n--- Query: '{}' ---", query);
        let query_embedding = client.generate_embedding(query).await?;
        let results = storage
            .hybrid_search(query, Some(query_embedding), 5)
            .await?;

        if results.is_empty() {
            println!("❌ No results found.");
        } else {
            for (i, res) in results.iter().enumerate() {
                let tid = res.thread_id.as_deref().unwrap_or("Unknown");
                println!(
                    "{}. [{}] From: {} (Thread: {})",
                    i + 1,
                    res.subject,
                    res.from,
                    if tid.len() > 8 { &tid[..8] } else { tid }
                );
                println!(
                    "   Preview: {}",
                    if res.body.len() > 50 {
                        format!("{}...", &res.body[..50])
                    } else {
                        res.body.clone()
                    }
                );
            }
        }
    }

    Ok(())
}
