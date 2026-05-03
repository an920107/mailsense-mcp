-- Initial schema for MailSense-MCP

-- Table for tracking processed emails (Idempotency)
CREATE TABLE IF NOT EXISTS processed_emails (
    id UUID PRIMARY KEY,
    message_id TEXT NOT NULL UNIQUE,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Table for background tasks
CREATE TABLE IF NOT EXISTS tasks (
    id UUID PRIMARY KEY,
    task_type TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('Pending', 'InProgress', 'Completed', 'Failed')),
    payload JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for picking next pending task efficiently
CREATE INDEX IF NOT EXISTS idx_tasks_status_created ON tasks(status, created_at) WHERE status = 'Pending';
