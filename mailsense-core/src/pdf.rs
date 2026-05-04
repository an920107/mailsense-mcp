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
            let mut cursor = Cursor::new(pdf_bytes);

            // 1. Load the document
            if let Ok(mut doc) = Document::load_from(&mut cursor) {
                if !doc.is_encrypted() {
                    return Some(pdf_bytes.to_vec());
                }

                // 2. Authenticate
                // Note: authenticate_password returns Result<bool>.
                // Both true (Owner) and false (User) allow decryption.
                if doc.authenticate_password(password).is_ok() {
                    // 3. Decrypt all objects
                    if doc.decrypt(password).is_ok() {
                        // 4. CRITICAL CLEANUP:
                        // Remove encryption metadata so readers don't look for a password.
                        doc.trailer.remove(b"Encrypt");
                        doc.trailer.remove(b"ID"); // Force re-generation of ID for safety

                        // 5. RESTRUCTURE:
                        // Renumbering is essential for files that used XRef streams (common in qpdf 256).
                        // It rebuilds the internal object map and ensures the catalog is reachable.
                        doc.renumber_objects();

                        // 6. COMPRESS & SAVE:
                        // Using save_modern() produces a modern PDF structure with XRef streams,
                        // which is most compatible with the original qpdf source structure.
                        let mut output = Vec::new();
                        if doc.save_modern(&mut output).is_ok() {
                            tracing::info!("Successfully decrypted and rebuilt PDF structure.");
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
