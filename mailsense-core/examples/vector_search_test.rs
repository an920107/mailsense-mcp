use mailsense_core::domain::{Attachment, EmailMessage, LlmProvider, StorageProvider};
use mailsense_core::llm::GeminiClient;
use mailsense_core::storage::PgStorage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 載入配置 (Addressing PR 3193688170: Use explicit env vars for examples)
    dotenvy::dotenv().ok();

    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY missing");
    let model = std::env::var("GEMINI_MODEL").expect("GEMINI_MODEL missing");
    let embedding_model =
        std::env::var("GEMINI_EMBEDDING_MODEL").expect("GEMINI_EMBEDDING_MODEL missing");
    let base_url = std::env::var("GEMINI_BASE_URL")
        .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL missing");

    // 2. 初始化組件
    let client = GeminiClient::new(api_key, model, embedding_model, Some(base_url));
    let storage = PgStorage::connect(&database_url).await?;

    println!("🚀 Testing Vector Search & Threading Pipeline...");

    // 3. 準備模擬數據 (Thread: Project Alpha)
    let email1 = EmailMessage {
        message_id: "example-msg-1-kickoff".to_string(),
        thread_id: None,
        in_reply_to: None,
        references: vec![],
        subject: "Project Alpha Kickoff".to_string(),
        from: "manager@example.com".to_string(),
        body: "Let's start the new project focused on Rust and MCP integration.".to_string(),
        date: "2026-05-01T10:00:00Z".to_string(),
        attachments: vec![],
    };

    let email2 = EmailMessage {
        message_id: "example-msg-2-reply".to_string(),
        thread_id: None,
        in_reply_to: Some(email1.message_id.clone()),
        references: vec![email1.message_id.clone()],
        subject: "Re: Project Alpha Kickoff".to_string(),
        from: "dev@example.com".to_string(),
        body: "I will setup the basic workspace using Cargo Workspaces. See the design attached."
            .to_string(),
        date: "2026-05-01T11:00:00Z".to_string(),
        attachments: vec![Attachment {
            filename: "architecture.png".to_string(),
            mime_type: "image/png".to_string(),
            // A valid 1x1 transparent PNG
            data: vec![
                137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0,
                1, 8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 8, 215, 99, 96, 0,
                2, 0, 0, 5, 0, 1, 226, 38, 5, 155, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
            ],
        }],
    };

    let mut emails = vec![email1];
    // Add email2 with attachment
    emails.push(email2);

    // 4. 生成 Embedding 並儲存
    println!("\n📥 Phase 1: Embedding & Storage...");
    for email in &emails {
        print!(
            "Processing: {} (Attachments: {})... ",
            email.subject,
            email.attachments.len()
        );

        // 🚀 Idempotency Guard: 檢查是否已經處理過，避免重複呼叫昂貴的 LLM (Comment 3192264175)
        if storage.is_email_processed(&email.message_id).await? {
            println!("⏩ Skipped (Already processed).");
            continue;
        }

        let embedding = client.generate_embedding(email).await?;

        // 簡單的 Thread ID 邏輯：根郵件的 ID
        let thread_id = email
            .in_reply_to
            .clone()
            .unwrap_or(email.message_id.clone());

        storage
            .store_email_document(email, &thread_id, Some(embedding))
            .await?;

        // 標記為已處理
        storage.mark_email_processed(&email.message_id).await?;

        println!("✅ Stored & Marked.");
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
        let query_embedding = client.generate_query_embedding(query).await?;
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
