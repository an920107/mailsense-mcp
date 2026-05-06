-- migration: drop_tasks_table.sql

DROP TABLE IF EXISTS tasks CASCADE;
DROP INDEX IF EXISTS idx_tasks_status_created;
