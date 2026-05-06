use crate::domain::{StorageProvider, Task, TaskStatus};
use anyhow::Context;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgStorage {
    pool: PgPool,
}

/// Data Transfer Object for Task to handle SQLx mapping without coupling domain
#[derive(sqlx::FromRow)]
struct TaskDto {
    id: Uuid,
    task_type: String,
    status: String,
    payload: serde_json::Value,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl From<TaskDto> for Task {
    fn from(dto: TaskDto) -> Self {
        let status = match dto.status.as_str() {
            "InProgress" => TaskStatus::InProgress,
            "Completed" => TaskStatus::Completed,
            "Failed" => TaskStatus::Failed,
            _ => TaskStatus::Pending,
        };

        Self {
            id: dto.id,
            task_type: dto.task_type,
            status,
            payload: dto.payload,
            created_at: dto.created_at,
            updated_at: dto.updated_at,
        }
    }
}

impl PgStorage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPool::connect(database_url).await?;
        Ok(Self::new(pool))
    }

    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        sqlx::migrate!("../migrations").run(&self.pool).await?;
        Ok(())
    }
}

#[async_trait]
impl StorageProvider for PgStorage {
    async fn is_email_processed(&self, message_id: &str) -> anyhow::Result<bool> {
        let exists = sqlx::query!(
            "SELECT EXISTS(SELECT 1 FROM processed_emails WHERE message_id = $1) as \"exists!\"",
            message_id
        )
        .fetch_one(&self.pool)
        .await?
        .exists;

        Ok(exists)
    }

    async fn mark_email_processed(&self, message_id: &str) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO processed_emails (id, message_id) VALUES ($1, $2) ON CONFLICT (message_id) DO NOTHING",
            Uuid::new_v4(),
            message_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_email_by_id(
        &self,
        message_id: &str,
    ) -> anyhow::Result<Option<crate::domain::EmailMessage>> {
        let row = sqlx::query!(
            r#"
            SELECT 
                message_id, thread_id, in_reply_to, "references", subject, from_address, body_text, date
            FROM email_documents
            WHERE message_id = $1
            "#,
            message_id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = row {
            Ok(Some(crate::domain::EmailMessage {
                message_id: r.message_id,
                thread_id: Some(r.thread_id),
                in_reply_to: r.in_reply_to,
                references: r.references,
                subject: r.subject,
                from: r.from_address,
                body: r.body_text,
                date: r.date.to_rfc3339(),
                attachments: vec![], // Attachments are not stored in the documents table
                analysis: None,
            }))
        } else {
            Ok(None)
        }
    }

    async fn enqueue_task(
        &self,
        task_type: &str,
        payload: serde_json::Value,
    ) -> anyhow::Result<Task> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let status = TaskStatus::Pending;

        let dto = sqlx::query_as!(
            TaskDto,
            r#"
            INSERT INTO tasks (id, task_type, status, payload, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, task_type, status, payload, created_at, updated_at
            "#,
            id,
            task_type,
            status.as_str(),
            payload,
            now,
            now
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(dto.into())
    }

    async fn pick_next_task(&self) -> anyhow::Result<Option<Task>> {
        let mut tx = self.pool.begin().await?;

        let dto = sqlx::query_as!(
            TaskDto,
            r#"
            UPDATE tasks
            SET status = 'InProgress', updated_at = NOW()
            WHERE id = (
                SELECT id FROM tasks
                WHERE status = 'Pending'
                ORDER BY created_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, task_type, status, payload, created_at, updated_at
            "#
        )
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(dto.map(|d| d.into()))
    }

    async fn update_task_status(&self, id: Uuid, status: TaskStatus) -> anyhow::Result<()> {
        sqlx::query!(
            "UPDATE tasks SET status = $1, updated_at = NOW() WHERE id = $2",
            status.as_str(),
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn store_email_document(
        &self,
        email: &crate::domain::EmailMessage,
        thread_id: &str,
        embedding: Option<Vec<f32>>,
        analysis: Option<crate::domain::EmailAnalysis>,
    ) -> anyhow::Result<()> {
        let embedding_vector = embedding.map(pgvector::Vector::from);

        // Explicitly handle invalid dates instead of silent fallback to NOW() (Addressing PR 3193688036)
        let date = chrono::DateTime::parse_from_rfc3339(&email.date)
            .map(|dt| dt.with_timezone(&Utc))
            .context("Invalid RFC3339 date format in email.date")?;

        let (summary, intent, deadlines) = if let Some(a) = analysis {
            (
                Some(a.summary),
                Some(a.intent.as_str().to_string()),
                Some(a.extracted_deadlines),
            )
        } else {
            (None, None, None)
        };

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO email_documents (
                id, message_id, thread_id, in_reply_to, "references", 
                subject, from_address, body_text, date, embedding,
                summary, intent, deadlines
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (message_id) DO UPDATE SET
                thread_id = EXCLUDED.thread_id,
                in_reply_to = EXCLUDED.in_reply_to,
                "references" = EXCLUDED."references",
                subject = EXCLUDED.subject,
                from_address = EXCLUDED.from_address,
                body_text = EXCLUDED.body_text,
                date = EXCLUDED.date,
                embedding = EXCLUDED.embedding,
                summary = EXCLUDED.summary,
                intent = EXCLUDED.intent,
                deadlines = EXCLUDED.deadlines
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(&email.message_id)
        .bind(thread_id)
        .bind(&email.in_reply_to)
        .bind(&email.references)
        .bind(&email.subject)
        .bind(&email.from)
        .bind(&email.body)
        .bind(date)
        .bind(embedding_vector)
        .bind(summary)
        .bind(intent)
        .bind(deadlines)
        .execute(&mut *tx)
        .await?;

        // Persist attachments
        // First delete existing attachments for this message_id (if any, due to upsert)
        sqlx::query!(
            "DELETE FROM email_attachments WHERE message_id = $1",
            email.message_id
        )
        .execute(&mut *tx)
        .await?;

        for attachment in &email.attachments {
            sqlx::query!(
                r#"
                INSERT INTO email_attachments (
                    id, message_id, filename, mime_type, data, 
                    is_encrypted, is_decrypted, decryption_error
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
                Uuid::new_v4(),
                email.message_id,
                attachment.filename,
                attachment.mime_type,
                attachment.data,
                attachment.is_encrypted,
                attachment.is_decrypted,
                attachment.decryption_error
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    async fn hybrid_search(
        &self,
        query_text: &str,
        query_embedding: Option<Vec<f32>>,
        intent: Option<crate::domain::EmailIntent>,
        limit: u32,
    ) -> anyhow::Result<Vec<crate::domain::EmailMessage>> {
        let embedding_vector = query_embedding.map(pgvector::Vector::from);
        let intent_str = intent.map(|i| i.as_str().to_string());

        let rows = sqlx::query(
            r#"
            SELECT 
                message_id, thread_id, in_reply_to, "references", subject, from_address, body_text, date,
                summary, intent, deadlines
            FROM email_documents
            WHERE 
                (search_vector @@ websearch_to_tsquery('english', $1)
                OR ($2::vector IS NOT NULL AND embedding IS NOT NULL))
                AND ($4::TEXT IS NULL OR intent = $4)
            ORDER BY 
                (ts_rank(search_vector, websearch_to_tsquery('english', $1)) * 0.4 + 
                 COALESCE(
                    CASE WHEN $2::vector IS NOT NULL AND embedding IS NOT NULL 
                    THEN (1.0 / (1.0 + (embedding <-> $2))) 
                    ELSE 0 
                    END, 0
                 ) * 0.6) DESC
            LIMIT $3
            "#,
        )
        .bind(query_text)
        .bind(embedding_vector)
        .bind(limit as i64)
        .bind(intent_str)
        .fetch_all(&self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            use sqlx::Row;

            let analysis = if let (Ok(summary), Ok(intent_str)) = (
                row.try_get::<String, _>("summary"),
                row.try_get::<String, _>("intent"),
            ) {
                let intent = match intent_str.as_str() {
                    "ActionRequired" => crate::domain::EmailIntent::ActionRequired,
                    "FYI" => crate::domain::EmailIntent::FYI,
                    "Update" => crate::domain::EmailIntent::Update,
                    _ => crate::domain::EmailIntent::Spam,
                };

                Some(crate::domain::EmailAnalysis {
                    intent,
                    tags: vec![], // Tags are not stored separately yet
                    summary,
                    extracted_deadlines: row.try_get("deadlines").unwrap_or_default(),
                    password_recipes: None, // No longer stored
                })
            } else {
                None
            };

            messages.push(crate::domain::EmailMessage {
                message_id: row.get("message_id"),
                thread_id: Some(row.get("thread_id")),
                in_reply_to: row.get("in_reply_to"),
                references: row.get("references"),
                subject: row.get("subject"),
                from: row.get("from_address"),
                body: row.get("body_text"),
                date: row.get::<chrono::DateTime<Utc>, _>("date").to_rfc3339(),
                attachments: vec![],
                analysis,
            });
        }

        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> PgStorage {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
        let storage = PgStorage::connect(&database_url)
            .await
            .expect("Failed to connect to test DB");
        storage
            .run_migrations()
            .await
            .expect("Failed to run migrations");
        storage
    }

    #[tokio::test]
    #[ignore] // Requires a running Postgres DB
    async fn test_processed_email_tracking() {
        let storage = setup_test_db().await;
        let message_id = format!("test-email-{}", Uuid::new_v4());

        // Use transaction for cleanup
        let mut tx = storage.pool.begin().await.unwrap();

        let exists = sqlx::query!(
            "SELECT EXISTS(SELECT 1 FROM processed_emails WHERE message_id = $1) as \"exists!\"",
            message_id
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .exists;
        assert!(!exists);

        sqlx::query!(
            "INSERT INTO processed_emails (id, message_id) VALUES ($1, $2)",
            Uuid::new_v4(),
            message_id
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        let exists = sqlx::query!(
            "SELECT EXISTS(SELECT 1 FROM processed_emails WHERE message_id = $1) as \"exists!\"",
            message_id
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .exists;
        assert!(exists);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires a running Postgres DB
    async fn test_task_queue_flow() {
        let storage = setup_test_db().await;
        let payload = serde_json::json!({"key": "value"});

        // We can't easily use rollback for pick_next_task because it uses its own transaction internally.
        // But we can cleanup manually or use a unique task type.
        let task_type = format!("test_task_{}", Uuid::new_v4());

        let task = storage
            .enqueue_task(&task_type, payload.clone())
            .await
            .unwrap();
        assert_eq!(task.task_type, task_type);
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.payload, payload);

        // This pick_next_task call will only pick OUR unique task if others are not Pending.
        // To be safe, we verify it's the one we just created.
        let picked = storage
            .pick_next_task()
            .await
            .unwrap()
            .expect("Should have picked a task");
        assert_eq!(picked.id, task.id);
        assert_eq!(picked.status, TaskStatus::InProgress);

        storage
            .update_task_status(picked.id, TaskStatus::Completed)
            .await
            .unwrap();

        // Cleanup
        sqlx::query!("DELETE FROM tasks WHERE id = $1", picked.id)
            .execute(&storage.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_email_document_storage_and_hybrid_search() {
        dotenvy::dotenv().ok();
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let storage = PgStorage::connect(&database_url).await.unwrap();

        let email = crate::domain::EmailMessage {
            message_id: format!("test-search-{}", Uuid::new_v4()),
            thread_id: None,
            in_reply_to: None,
            references: vec![],
            subject: "Meeting about Rust".to_string(),
            from: "alice@example.com".to_string(),
            body: "Let's discuss the new async traits implementation.".to_string(),
            date: "2026-05-06T10:00:00Z".to_string(),
            attachments: vec![],
            analysis: None,
        };

        // Test Storage (Upsert)
        storage
            .store_email_document(&email, "thread-123", None, None)
            .await
            .unwrap();

        // Test Hybrid Search (FTS part)
        let results = storage
            .hybrid_search("async traits", None, None, 5)
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].subject, "Meeting about Rust");

        // Test Storage with Embedding
        let dummy_embedding = vec![0.1; 768];
        storage
            .store_email_document(&email, "thread-123", Some(dummy_embedding.clone()), None)
            .await
            .unwrap();

        // Test Hybrid Search (Vector part)
        let results = storage
            .hybrid_search("rust", Some(dummy_embedding), None, 5)
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].message_id, email.message_id);

        // Cleanup
        sqlx::query!(
            "DELETE FROM email_documents WHERE message_id = $1",
            email.message_id
        )
        .execute(&storage.pool)
        .await
        .unwrap();
    }
}
