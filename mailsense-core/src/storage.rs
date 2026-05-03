use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use chrono::Utc;
use crate::domain::{Task, TaskStatus, StorageProvider};

pub struct PgStorage {
    pool: PgPool,
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
        sqlx::migrate!("../migrations")
            .run(&self.pool)
            .await?;
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

    async fn enqueue_task(&self, task_type: &str, payload: serde_json::Value) -> anyhow::Result<Task> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let status = "Pending";

        let task = sqlx::query_as!(
            crate::domain::Task,
            r#"
            INSERT INTO tasks (id, task_type, status, payload, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, task_type, status as "status: TaskStatus", payload, created_at, updated_at
            "#,
            id,
            task_type,
            status,
            payload,
            now,
            now
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(task)
    }

    async fn pick_next_task(&self) -> anyhow::Result<Option<Task>> {
        let mut tx = self.pool.begin().await?;

        let task = sqlx::query_as!(
            crate::domain::Task,
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
            RETURNING id, task_type, status as "status: TaskStatus", payload, created_at, updated_at
            "#
        )
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(task)
    }

    async fn update_task_status(&self, id: Uuid, status: TaskStatus) -> anyhow::Result<()> {
        let status_str = match status {
            TaskStatus::Pending => "Pending",
            TaskStatus::InProgress => "InProgress",
            TaskStatus::Completed => "Completed",
            TaskStatus::Failed => "Failed",
        };

        sqlx::query!(
            "UPDATE tasks SET status = $1, updated_at = NOW() WHERE id = $2",
            status_str,
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
