use mailsense_core::config::Config;
use mailsense_core::domain::{EmailMessage, LlmProvider};
use mailsense_core::llm::GeminiClient;
use mailsense_core::password::PasswordPoolBuilder;
use mailsense_core::pdf::decrypt_pdf_with_timeout;
use std::env;
use std::fs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 載入配置
    let config =
        Config::load().expect("Failed to load .env. Please set USER_ID_NUMBER and USER_BIRTHDAY.");
    let personal = config
        .personal
        .as_ref()
        .expect("Personal info missing or invalid in .env");

    // 檢查是否有傳入 PDF 檔案路徑
    let args: Vec<String> = env::args().collect();
    let pdf_path = args.get(1);

    println!("🚀 Testing Full Decryption Pipeline...");

    // 安全地遮罩 ID 與生日 (Comment 3192264175, 3192264138)
    let masked_id = if personal.id_number.len() >= 4 {
        format!(
            "****{}",
            &personal.id_number[personal.id_number.len() - 4..]
        )
    } else {
        "****".to_string()
    };
    let masked_bday = if personal.birthday.len() >= 4 {
        format!("****{}", &personal.birthday[personal.birthday.len() - 4..])
    } else {
        "****".to_string()
    };

    println!("Local ID (Masked): {}", masked_id);
    println!("Local Bday (Masked): {}", masked_bday);

    // 2. 初始化 LLM
    let gemini_cfg = config.gemini.as_ref().expect("GEMINI_API_KEY missing");
    let client = GeminiClient::new(
        gemini_cfg.api_key.clone(),
        gemini_cfg.model.clone(),
        gemini_cfg.embedding_model.clone(),
        Some(gemini_cfg.base_url.clone()),
        5 * 1024 * 1024,
        3,
    );

    // 3. 準備模擬郵件 (您可以根據實際 PDF 的密碼規則修改這段 Body)
    let email = EmailMessage {
        id: None,
        message_id: "test-id-pdf-123".to_string(),
        thread_id: None,
        in_reply_to: None,
        references: vec![],
        subject: "Your E-Statement is ready".to_string(),
        from: "bank@example.com".to_string(),
        body: r#"
            Dear Customer,
            Your monthly statement is attached as an encrypted PDF.
            The password is the last 4 digits of your ID number followed by your birthday MMDD.
        "#
        .to_string(),
        date: "2026-05-04T10:00:00Z".to_string(),
        attachments: vec![],
        analysis: None,
    };

    println!("\n📧 Phase 1: LLM Recipe Deduction...");
    let analysis = client.analyze_email(&email).await?;
    println!("Deduced Intent: {:?}", analysis.intent); // Fix typo (Comment 3192264184)
    println!("Password Recipes from LLM: {:?}", analysis.password_recipes);

    // 4. 根據 LLM 配方組裝密碼池
    println!("\n🔑 Phase 2: Local Password Assembly...");
    let builder = PasswordPoolBuilder::new(personal);
    let pool = builder.build(&email, analysis.password_recipes.as_ref());
    println!("Generated {} password candidates.", pool.len());

    // 5. 如果有提供檔案，則執行真實解密
    if let Some(path) = pdf_path {
        println!("\n📄 Phase 3: Real PDF Decryption (Path: {})...", path);
        let pdf_bytes = fs::read(path)?;

        match decrypt_pdf_with_timeout(&pdf_bytes, &pool).await? {
            Some(decrypted) => {
                let out_path = "decrypted_output.pdf";
                fs::write(out_path, decrypted)?;
                println!("✅ Success! PDF decrypted and saved to: {}", out_path);
            }
            None => {
                println!("❌ Failure: Could not decrypt the PDF with the generated password pool.");
            }
        }
    } else {
        println!("\n💡 Tip: Provide a PDF path as an argument to test real decryption.");
        println!(
            "Example: cargo run -p mailsense-core --example pdf_decryption_test -- my_locked_file.pdf"
        );

        // 僅驗證密碼池組裝邏輯 (Comment 3192264159)
        if personal.id_number.len() >= 4 && personal.birthday.len() >= 8 {
            let expected = format!(
                "{}{}",
                &personal.id_number[personal.id_number.len() - 4..],
                &personal.birthday[4..8]
            );
            if pool.contains(&expected) {
                println!(
                    "\n✅ Assembly logic verified: '****{}' is in the pool.",
                    &expected[expected.len() - 4..]
                );
            }
        }
    }

    Ok(())
}
