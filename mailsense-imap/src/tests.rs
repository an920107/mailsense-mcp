#[cfg(test)]
mod tests {
    #[test]
    fn test_email_parsing() {
        let raw_email = b"From: Alice <alice@example.com>\r\n\
                          To: Bob <bob@example.com>\r\n\
                          Subject: Hello from MailSense\r\n\
                          Date: Sun, 03 May 2026 15:00:00 +0000\r\n\
                          Content-Type: text/plain; charset=utf-8\r\n\r\n\
                          This is a test email body.\r\n";

        let parsed = mail_parser::MessageParser::new().parse(raw_email).expect("Failed to parse email");
        
        let subject = parsed.subject().unwrap_or("No Subject").to_string();
        assert_eq!(subject, "Hello from MailSense");

        let from = parsed.from()
            .and_then(|f| f.as_list())
            .and_then(|l| l.first())
            .map(|a| a.address().unwrap_or("Unknown"))
            .unwrap_or("Unknown")
            .to_string();
        assert_eq!(from, "alice@example.com");

        let body_text = parsed.body_text(0).as_deref().unwrap_or("").to_string();
        assert_eq!(body_text.trim(), "This is a test email body.");

        let date = parsed.date().map(|d| d.to_rfc3339()).unwrap_or_else(|| "Unknown".to_string());
        assert_eq!(date, "2026-05-03T15:00:00Z");
    }
}
