use lopdf::Document;
use std::time::Duration;
use tokio::time::timeout;

pub async fn decrypt_pdf_with_timeout(
    pdf_bytes: &[u8],
    password_pool: &[String],
) -> anyhow::Result<Option<Vec<u8>>> {
    if password_pool.is_empty() {
        return Ok(None);
    }

    // Wrap the brute-force in a 60-second timeout
    let result = timeout(Duration::from_secs(60), async {
        for password in password_pool {
            // Attempt to load and authenticate the PDF
            if let Ok(mut doc) = Document::load_mem(pdf_bytes) {
                if !doc.is_encrypted() {
                    // Already decrypted or not encrypted
                    return Some(pdf_bytes.to_vec());
                }

                if doc.authenticate_password(password).is_ok() {
                    // Success! Strip encryption and return bytes
                    if doc.decrypt(password).is_ok() {
                        let mut output = Vec::new();
                        if doc.save_to(&mut output).is_ok() {
                            return Some(output);
                        }
                    }
                }
            }
        }
        None
    })
    .await;

    match result {
        Ok(decrypted_bytes) => Ok(decrypted_bytes),
        Err(_) => {
            tracing::warn!("PDF decryption timed out after 60 seconds.");
            Ok(None)
        }
    }
}
