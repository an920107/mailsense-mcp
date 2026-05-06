-- migration: drop_password_recipes.sql

ALTER TABLE email_documents DROP COLUMN IF EXISTS password_recipes;