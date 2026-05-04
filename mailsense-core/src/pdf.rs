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
            // 1. Load and decrypt in one step
            if let Ok(mut doc) = Document::load_mem_with_password(pdf_bytes, password) {
                // 2. CRITICAL CLEANUP:
                // Remove encryption metadata so readers don't look for a password.
                doc.trailer.remove(b"Encrypt");
                
                // 3. DECOMPRESS
                // Often helpful for compatibility with various readers after decryption.
                doc.decompress();

                // 4. SAVE:
                let mut output = Vec::new();
                if doc.save_to(&mut output).is_ok() {
                    tracing::info!("Successfully decrypted and saved PDF using load_mem_with_password. Size: {}", output.len());
                    return Some(output);
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

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Document, Object, Dictionary};
    use std::io::Cursor;

    #[tokio::test]
    async fn test_pdf_decryption_cycle() {
        // 1. Create a simple PDF
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Font".into()),
            ("Subtype", "Type1".into()),
            ("BaseFont", "Courier".into()),
        ]));
        let resources_id = doc.add_object(Dictionary::from_iter(vec![
            ("Font", Dictionary::from_iter(vec![("F1", font_id.into())]).into()),
        ]));
        let content = b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET";
        let content_id = doc.add_object(lopdf::Stream::new(Dictionary::new(), content.to_vec()));
        let page_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Page".into()),
            ("Parent", pages_id.into()),
            ("Contents", content_id.into()),
            ("Resources", resources_id.into()),
            ("MediaBox", vec![0.into(), 0.into(), 595.into(), 842.into()].into()),
        ]));
        let pages = Dictionary::from_iter(vec![
            ("Type", "Pages".into()),
            ("Kids", vec![page_id.into()].into()),
            ("Count", 1.into()),
        ]);
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let root_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Catalog".into()),
            ("Pages", pages_id.into()),
        ]));
        doc.trailer.set("Root", root_id);

        // 2. Mocking encryption by just checking the unencrypted path first
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        
        let result = decrypt_pdf_with_timeout(&bytes, &["pass".to_string()])
            .await
            .unwrap()
            .expect("Should return a valid PDF even if not encrypted");
        
        // Load the result and verify it's a valid PDF
        let result_doc = Document::load_from(&mut Cursor::new(&result)).unwrap();
        assert_eq!(result_doc.version, "1.5");
        assert!(!result_doc.is_encrypted());
    }
}
