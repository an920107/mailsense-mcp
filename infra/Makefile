# Makefile for MailSense-MCP Infrastructure

IMAGE_NAME = pgvector/pgvector:pg18
POD_FILE = pod.yaml

.PHONY: help db-up db-down db-status

help:
	@echo "Available commands:"
	@echo "  make db-up      - Start the database pod using the official pgvector image"
	@echo "  make db-down    - Stop and remove the database pod"
	@echo "  make db-status  - Check the status of the database pod and containers"

db-up:
	@echo "📦 Starting database pod..."
	@sed "s|path: \./|path: $(CURDIR)/|g" $(POD_FILE) > pod.tmp.yaml
	podman kube play --replace pod.tmp.yaml
	@rm pod.tmp.yaml

db-down:
	@echo "🛑 Stopping and removing database pod..."
	podman pod rm -f mailsense

db-status:
	@echo "📊 Database Pod Status:"
	podman pod ps --filter name=mailsense
	@echo "\n📊 Container Status:"
	podman ps --filter pod=mailsense
