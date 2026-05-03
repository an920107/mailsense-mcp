use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use chrono::Utc;
use crate::domain::{Task, TaskStatus, StorageProvider};

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
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> PgStorage {
        dotenvy::dotenv().ok();
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
        let storage = PgStorage::connect(&database_url).await.expect("Failed to connect to test DB");
        storage.run_migrations().await.expect("Failed to run migrations");
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
        .await.unwrap().exists;
        assert!(!exists);

        sqlx::query!(
            "INSERT INTO processed_emails (id, message_id) VALUES ($1, $2)",
            Uuid::new_v4(),
            message_id
        )
        .execute(&mut *tx)
        .await.unwrap();

        let exists = sqlx::query!(
            "SELECT EXISTS(SELECT 1 FROM processed_emails WHERE message_id = $1) as \"exists!\"",
            message_id
        )
        .fetch_one(&mut *tx)
        .await.unwrap().exists;
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
        
        let task = storage.enqueue_task(&task_type, payload.clone()).await.unwrap();
        assert_eq!(task.task_type, task_type);
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.payload, payload);

        // This pick_next_task call will only pick OUR unique task if others are not Pending.
        // To be safe, we verify it's the one we just created.
        let picked = storage.pick_next_task().await.unwrap().expect("Should have picked a task");
        assert_eq!(picked.id, task.id);
        assert_eq!(picked.status, TaskStatus::InProgress);

        storage.update_task_status(picked.id, TaskStatus::Completed).await.unwrap();
        
        // Cleanup
        sqlx::query!("DELETE FROM tasks WHERE id = $1", picked.id)
            .execute(&storage.pool)
            .await.unwrap();
    }
}
