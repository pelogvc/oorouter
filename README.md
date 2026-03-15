<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" height="128" />
</p>

<h1 align="center">oorouter</h1>

**Ollama OAuth Router** — Use your existing ChatGPT/Codex subscription as a local AI API.

oorouter acts as a drop-in proxy that exposes Ollama and OpenAI-compatible API endpoints, routing requests through the ChatGPT backend using your Codex OAuth credentials. Any tool that speaks Ollama or OpenAI API can connect to oorouter without modification.

```
Your App (Ollama/OpenAI API) → oorouter → ChatGPT Backend → oorouter → Your App
```

## Features

- **Ollama API compatible** — `/api/chat`, `/api/generate`, `/api/tags`, `/api/show`, and more
- **OpenAI API compatible** — `/v1/chat/completions`, `/v1/models`
- **Streaming support** — SSE-to-NDJSON conversion for real-time responses
- **Desktop app** — Native macOS/Windows/Linux app with system tray (Tauri)
- **Dashboard** — Monitor server status, logs, token usage, and available models
- **Light/Dark mode** — System-aware theme with manual toggle
- **Token tracking** — SQLite-backed usage logging per model and day
- **Auth file watching** — Auto-reloads credentials when `auth.json` changes

## Supported Backends

| Backend | Status |
|---------|--------|
| ChatGPT / Codex (OpenAI) | Supported |
| Claude (Anthropic) | Planned |
| Gemini (Google) | Planned |

## Quick Start

### Desktop App (Recommended)

```bash
# Install dependencies
bun install

# Run in development mode
cargo tauri dev

# Build for production
cargo tauri build
```

### Standalone Proxy Server

```bash
# Run the proxy without the desktop UI
cargo run -p proxy-core --bin proxy-server

# The proxy listens on port 11434 by default (same as Ollama)
curl http://localhost:11434/api/tags
```

### Docker

```bash
docker build -t oorouter .
docker run -p 11434:11434 -v ~/.codex:/root/.codex:ro oorouter
```

## Usage

Point any Ollama or OpenAI-compatible client at `http://localhost:11434`:

```bash
# Ollama CLI
OLLAMA_HOST=http://localhost:11434 ollama run codex

# OpenAI-compatible clients
curl http://localhost:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "codex", "messages": [{"role": "user", "content": "Hello"}]}'

# Continue, Cursor, or any tool that supports Ollama
# Just set the Ollama endpoint to http://localhost:11434
```

## Configuration

Create a `.env` file or pass environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `11434` | Proxy listen port |
| `AUTH_PATH` | `~/.codex/auth.json` | Path to Codex auth credentials |
| `LOG_LEVEL` | `info` | Logging verbosity (`debug`, `info`, `warn`, `error`) |
| `CHATGPT_API_URL` | `https://chatgpt.com/backend-api/codex/responses` | Backend API endpoint |

## Authentication

oorouter reads credentials from `~/.codex/auth.json`:

```json
{
  "tokens": {
    "access_token": "eyJ...",
    "account_id": "...",
    "refresh_token": "..."
  }
}
```

This file is created automatically by the [Codex CLI](https://github.com/openai/codex). Run `codex` once to authenticate, then oorouter will use the same credentials.

## Architecture

Dual-stack application:

- **Rust backend** (`crates/proxy-core`) — axum HTTP server, reqwest client, sqlx/SQLite, SSE streaming
- **React frontend** (`src/`) — Dashboard UI with shadcn/ui, Tailwind CSS v4, Lucide icons
- **Tauri shell** (`src-tauri/`) — Bridges Rust backend and React frontend into a native desktop app

## API Endpoints

### Ollama API

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/chat` | Chat completion |
| POST | `/api/generate` | Text generation |
| GET | `/api/tags` | List models |
| POST | `/api/show` | Model details |
| POST | `/api/embed` | Embeddings (stub) |
| GET | `/api/ps` | Running models (stub) |
| GET | `/api/version` | Version info |

### OpenAI API

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/chat/completions` | Chat completion |
| GET | `/v1/models` | List models |

## Development

```bash
# Frontend dev server
bun run dev

# Run all tests
cargo test

# Run specific test
cargo test -p proxy-core test_default_config

# Integration tests only
cargo test -p proxy-core --test integration

# Type check frontend
npx tsc -b --noEmit
```

## License

MIT
