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
            // Attempt to load the PDF
            if let Ok(mut doc) = Document::load_mem(pdf_bytes) {
                if !doc.is_encrypted() {
                    return Some(pdf_bytes.to_vec());
                }

                // Try to authenticate with the current password
                if doc.authenticate_password(password).is_ok() {
                    // 1. Decrypt all encrypted objects
                    if doc.decrypt(password).is_ok() {
                        // 2. CRITICAL: Remove the Encrypt entry from the trailer
                        // to tell PDF readers this file is no longer encrypted.
                        doc.trailer.remove(b"Encrypt");

                        // 3. Ensure the document is compressed/structured correctly for saving
                        doc.compress();

                        let mut output = Vec::new();
                        if doc.save_to(&mut output).is_ok() {
                            tracing::info!(
                                "Successfully decrypted PDF with a password from the pool."
                            );
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
