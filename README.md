# MailSense-MCP ✉️🧠

> A Headless, AI-Driven Email Processing Engine powered by Rust and the Model
> Context Protocol (MCP).

## 🎯 Project Purpose

**MailSense-MCP** is designed to transform a traditional, passive IMAP inbox
into an active, intelligent, and deeply queryable knowledge base.

Traditional email clients tightly couple the user interface with email
processing. MailSense breaks this paradigm by adopting a **Clean
Architecture**. It acts as a headless, stateful backend engine that focuses
entirely on domain logic: fetching emails, parsing complex MIME structures,
extracting semantic meaning via LLMs, and maintaining a vector database for
Retrieval-Augmented Generation (RAG).

By exposing its capabilities through the **Model Context Protocol (MCP)**,
MailSense-MCP allows any MCP-compatible client (Telegram Bots, Claude Desktop,
custom Tauri apps) to interact with your inbox seamlessly, securely, and
without redundant processing.

## ✨ Core Capabilities

- **Autonomous Categorization & Summarization**: Automatically labels incoming
  emails (Important, Newsletter, Spam) and generates concise summaries.
- **Deadline Extraction & Task Generation**: Parses email bodies to identify
  temporal commitments and deadlines, exposing them for external scheduling
  tools.
- **Thread-Aware Hybrid Search**: Aggregates email threads and stores them using
  Dense Vectors + BM25 (Sparse Vectors) for highly accurate semantic retrieval.
- **Dynamic Attachment Decryption**: Employs a Two-Stage LLM reasoning pipeline
  to deduce password rules from the email body and brute-force encrypted PDFs
  (e.g., electronic tickets, bank statements).
- **Idempotent Operations**: Built-in local state management ensures emails are
  never processed twice, even across system restarts.

## 🛠 Tech Stack & Architecture

This project strictly adheres to asynchronous Rust best practices and modular
design.

### Core Technologies

- **Language**: Rust (Edition 2024)
- **Async Runtime**: `tokio`
- **Protocol**: Model Context Protocol (MCP) via `stdio` or `SSE`.
- **IMAP & Parsing**: `async-imap`, `mail-parser`
- **Database / State**: PostgreSQL (Sole Database)
- **Vector Search**: Qdrant (for semantic search)
- **LLM Integration**: Generic provider interface supporting OpenAI, Anthropic,
  or Local LLMs via Ollama (for strict privacy).

### Workspace Structure (Sub-crates)

To maintain separation of concerns, the project is divided into a Cargo
Workspace:

- `mailsense-core`: Contains the pure domain logic, LLM prompt engineering, and
  database traits.
- `mailsense-imap`: Infrastructure layer handling IMAP connections, fetching,
  and MIME parsing.
- `mailsense-mcp`: The presentation/protocol layer that wraps the core engine
  into MCP Tools and Resources.
- `mailsense-bin`: The executable entry point that wires dependencies together
  (Dependency Injection) and starts the server.

## 🤖 AI Agent Directives

If you are an AI assistant (like Gemini CLI) contributing to this repository,
you **MUST** read and adhere to the guidelines specified in
[`AGENT.md`](./AGENT.md) before writing any code.

**Key rules include:**

1. Never commit directly to `main`.
1. Follow ATDD/SDD (Write tests first).
1. Ensure exact synchronization between codebase and documentation.

## 🚀 Getting Started

### Prerequisites

- **Rust**: Version 1.85 or higher (Edition 2024 requires a recent toolchain).
- **PostgreSQL**: A running instance is required for local state storage.
- **SQLx CLI**: Required for database migrations and query preparation.
- **Markdown Lint**: Required for documentation quality checks.

```bash
cargo install sqlx-cli --no-default-features --features postgres
sudo pacman -S markdownlint
```

### Local Setup

1. **Clone the repository**:

```bash
git clone https://github.com/an920107/mailsense-mcp.git
cd mailsense-mcp
```

1. **Initialize Database (Podman)**:
   This project uses PostgreSQL with the `pgvector` extension. We provide a `Makefile` in the `infra/` directory to automate the custom image build and pod deployment.

```bash
# Build image and start the pod in one command
make -C infra db-up
```

Other database commands:
- `make -C infra db-status`: Check if the database is running.
- `make -C infra db-down`: Stop and remove the database pod.

1. **Configure Environment**:
   Copy the example environment file and fill in your credentials.
   Note: The default `DATABASE_URL` in `.env.example` is configured for the Podman setup.

```bash
cp .env.example .env
```

1. **Initialize Schema**:
   Run SQLx migrations to set up the tables and vector indices.

```bash
sqlx database create
sqlx migrate run
```

1. **Run Tests**:

```bash
cargo test --workspace
```

1. **Build and Run**:

```bash
cargo build -p mailsense-bin
```
