-- migration: add_vector_support.sql

-- The 'vector' extension is enabled via the Docker init script, 
-- but we include it here for redundancy.
CREATE EXTENSION IF NOT EXISTS vector;

-- Table for storing email content and embeddings for search
CREATE TABLE IF NOT EXISTS email_documents (
    id UUID PRIMARY KEY,
    message_id TEXT NOT NULL UNIQUE,
    thread_id TEXT NOT NULL,
    in_reply_to TEXT,
    "references" TEXT[] NOT NULL DEFAULT '{}',
    subject TEXT NOT NULL,
    from_address TEXT NOT NULL,
    body_text TEXT NOT NULL,
    date TIMESTAMPTZ NOT NULL,
    embedding vector(768), -- Dimension 768 for Gemini text-embedding-004
    search_vector tsvector GENERATED ALWAYS AS (
        to_tsvector('english', subject || ' ' || body_text)
    ) STORED,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- HNSW index for vector similarity search (L2 distance as suggested by <->)
CREATE INDEX IF NOT EXISTS idx_email_documents_embedding_l2 ON email_documents USING hnsw (embedding vector_l2_ops);

-- GIN index for full-text search
CREATE INDEX IF NOT EXISTS idx_email_documents_search_vector ON email_documents USING gin (search_vector);

-- Index for thread-based retrieval
CREATE INDEX IF NOT EXISTS idx_email_documents_thread_id ON email_documents(thread_id);
