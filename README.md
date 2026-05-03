# MailSense-MCP ✉️🧠

> A Headless, AI-Driven Email Processing Engine powered by Rust and the Model Context Protocol (MCP).

## 🎯 Project Purpose

**MailSense-MCP** is designed to transform a traditional, passive IMAP inbox into an active, intelligent, and deeply queryable knowledge base. 

Traditional email clients tightly couple the user interface with email processing. MailSense breaks this paradigm by adopting a **Clean Architecture**. It acts as a headless, stateful backend engine that focuses entirely on domain logic: fetching emails, parsing complex MIME structures, extracting semantic meaning via LLMs, and maintaining a vector database for Retrieval-Augmented Generation (RAG).

By exposing its capabilities through the **Model Context Protocol (MCP)**, MailSense-MCP allows any MCP-compatible client (Telegram Bots, Claude Desktop, custom Tauri apps) to interact with your inbox seamlessly, securely, and without redundant processing.

## ✨ Core Capabilities

- **Autonomous Categorization & Summarization**: Automatically labels incoming emails (Important, Newsletter, Spam) and generates concise summaries.
- **Deadline Extraction & Task Generation**: Parses email bodies to identify temporal commitments and deadlines, exposing them for external scheduling tools.
- **Thread-Aware Hybrid Search**: Aggregates email threads and stores them using Dense Vectors + BM25 (Sparse Vectors) for highly accurate semantic retrieval.
- **Dynamic Attachment Decryption**: Employs a Two-Stage LLM reasoning pipeline to deduce password rules from the email body and brute-force encrypted PDFs (e.g., electronic tickets, bank statements).
- **Idempotent Operations**: Built-in local state management ensures emails are never processed twice, even across system restarts.

## 🛠 Tech Stack & Architecture

This project strictly adheres to asynchronous Rust best practices and modular design.

### Core Technologies

*   **Language**: Rust (Edition 2021)
*   **Async Runtime**: `tokio`
*   **Protocol**: Model Context Protocol (MCP) via `stdio` or `SSE`.
*   **IMAP & Parsing**: `async-imap`, `mail-parser`
*   **Database / State**: SQLite (for idempotency and structured metadata)
*   **Vector Search**: Qdrant (or SQLite with FTS5/Vector extensions)
*   **LLM Integration**: Generic provider interface supporting OpenAI, Anthropic, or Local LLMs via Ollama (for strict privacy).

### Workspace Structure (Sub-crates)

To maintain separation of concerns, the project is divided into a Cargo Workspace:

*   `mailsense-core`: Contains the pure domain logic, LLM prompt engineering, and database traits.
*   `mailsense-imap`: Infrastructure layer handling IMAP connections, fetching, and MIME parsing.
*   `mailsense-mcp`: The presentation/protocol layer that wraps the core engine into MCP Tools and Resources.
*   `mailsense-bin`: The executable entry point that wires dependencies together (Dependency Injection) and starts the server.

## 🤖 AI Agent Directives

If you are an AI assistant (like Gemini CLI) contributing to this repository, you **MUST** read and adhere to the guidelines specified in [`AGENT.md`](./AGENT.md) before writing any code. 

**Key rules include:**

1. Never commit directly to `main`.
2. Follow ATDD/SDD (Write tests first).
3. Ensure exact synchronization between codebase and documentation.

## 🚀 Getting Started

*(Setup instructions, environment variables, and build commands will be populated here as development progresses.)*
