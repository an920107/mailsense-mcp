-- Add columns for storing analysis results
ALTER TABLE email_documents 
ADD COLUMN IF NOT EXISTS summary TEXT,
ADD COLUMN IF NOT EXISTS intent TEXT,
ADD COLUMN IF NOT EXISTS deadlines TEXT[] DEFAULT '{}',
ADD COLUMN IF NOT EXISTS password_recipes JSONB;

-- Create an index for intent filtering
CREATE INDEX IF NOT EXISTS idx_email_documents_intent ON email_documents(intent);
