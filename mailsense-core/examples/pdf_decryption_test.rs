use mailsense_core::config::Config;
use mailsense_core::domain::{EmailMessage, LlmProvider};
use mailsense_core::llm::GeminiClient;
use mailsense_core::password::PasswordPoolBuilder;
use mailsense_core::pdf::decrypt_pdf_with_timeout;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 載入配置 (包含您的身分證與生日)
    let config = Config::load().expect("Failed to load .env. Please set USER_ID_NUMBER and USER_BIRTHDAY.");
    let personal = config.personal.as_ref().expect("Personal info missing in .env");
    
    println!("🚀 Testing Full Decryption Pipeline...");
    println!("Local ID (Masked): ****{}", &personal.id_number[personal.id_number.len()-4..]);
    println!("Local Bday: {}", personal.birthday);

    // 2. 初始化 LLM
    let gemini_cfg = config.gemini.as_ref().expect("GEMINI_API_KEY missing");
    let client = GeminiClient::new(
        gemini_cfg.api_key.clone(),
        Some(gemini_cfg.model.clone()),
        Some(gemini_cfg.base_url.clone()),
    );

    // 3. 準備一封帶有複雜密碼說明的模擬郵件
    let email = EmailMessage {
        subject: "Your E-Statement is ready".to_string(),
        from: "bank@example.com".to_string(),
        body: r#"
            Dear Customer,
            Your monthly statement is attached as an encrypted PDF.
            The password is the last 4 digits of your ID number followed by your birthday MMDD.
        "#.to_string(),
        date: "2026-05-04T10:00:00Z".to_string(),
    };

    println!("\n📧 Phase 1: LLM Recipe Deduction...");
    let analysis = client.analyze_email(&email).await?;
    println!("Duced Intent: {:?}", analysis.intent);
    println!("Password Recipes from LLM: {:?}", analysis.password_recipes);

    // 4. 根據 LLM 配方組裝真實密碼
    println!("\n🔑 Phase 2: Local Password Assembly...");
    let builder = PasswordPoolBuilder::new(personal);
    let pool = builder.build(&email, analysis.password_recipes.as_ref());
    
    println!("Generated Password Pool (Top 5):");
    for (i, pwd) in pool.iter().take(5).enumerate() {
        println!("  {}. {}", i + 1, pwd);
    }

    // 5. 驗證期望的密碼是否在池中
    let expected = format!("{}{}", &personal.id_number[personal.id_number.len()-4..], &personal.birthday[4..]);
    if pool.contains(&expected) {
        println!("\n✅ Success! The correct password '{}' was successfully generated locally.", expected);
    } else {
        println!("\n❌ Failure: The expected password '{}' is missing from the pool.", expected);
    }

    Ok(())
}
