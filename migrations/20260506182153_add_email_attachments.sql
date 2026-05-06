-- migration: add_email_attachments.sql

CREATE TABLE IF NOT EXISTS email_attachments (
    id UUID PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES email_documents(message_id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    data BYTEA NOT NULL,
    is_encrypted BOOLEAN NOT NULL DEFAULT FALSE,
    is_decrypted BOOLEAN NOT NULL DEFAULT FALSE,
    decryption_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for quickly fetching attachments by message_id
CREATE INDEX IF NOT EXISTS idx_email_attachments_message_id ON email_attachments(message_id);
