use lopdf::Document;
use std::io::Cursor;
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
            // Use load_from כדי 確保能夠處理 Stream
            let mut cursor = Cursor::new(pdf_bytes);
            if let Ok(mut doc) = Document::load_from(&mut cursor) {
                if !doc.is_encrypted() {
                    return Some(pdf_bytes.to_vec());
                }

                // Try to authenticate with the current password
                if doc.authenticate_password(password).is_ok() {
                    // 1. Decrypt all encrypted objects using the password
                    if doc.decrypt(password).is_ok() {
                        // 2. CRITICAL: Remove all encryption metadata
                        doc.trailer.remove(b"Encrypt");
                        doc.trailer.remove(b"ID"); // Re-generating ID is safer

                        // 3. Flatten the document: Decompress and reset version for compatibility
                        doc.version = "1.7".to_string();
                        doc.decompress();

                        let mut output = Vec::new();
                        // 4. Use save_to (standard) instead of save_modern to maximize reader compatibility
                        if doc.save_to(&mut output).is_ok() {
                            tracing::info!("Successfully decrypted AES-256 PDF.");
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
