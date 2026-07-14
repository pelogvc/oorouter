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
- **Optional client authentication** — Protect only OpenAI-compatible `/v1/*` routes with one or more Bearer API keys

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
cargo run --quiet -p proxy-core --bin proxy-server

# The proxy listens on port 11434 by default (same as Ollama)
curl http://localhost:11434/api/tags
```

The standalone server binds to `127.0.0.1` by default. Use `--host <ip>` to
select another interface. Client authentication is off when no `--api-key` is
provided. Supplying one or more keys enables Bearer authentication for
OpenAI-compatible `/v1/*` endpoints only; Ollama `/api/*`, `/health`, and `/`
remain public.

```bash
API_KEY_ONE="sk-$(openssl rand -base64 96 | tr -dc 'A-Za-z0-9' | head -c 64)"
API_KEY_TWO="sk-$(openssl rand -base64 96 | tr -dc 'A-Za-z0-9' | head -c 64)"

cargo run --quiet -p proxy-core --bin proxy-server -- \
  --api-key "$API_KEY_ONE" \
  --api-key "$API_KEY_TWO"
```

Each key must have the format `sk-` followed by exactly 64 ASCII letters or
digits. Duplicate values are ignored. Standalone keys remain in process memory
and are not written to the desktop SQLite database. Because command-line secrets
can be visible in shell history and process listings, take the same precautions
you use for other command-line credentials. `--quiet` prevents Cargo from
printing the full `Running` command with the supplied keys.

### Docker

Build the multi-stage image from the repository root:

```bash
docker build --pull -t oorouter .
docker volume create oorouter-data
```

The entrypoint uses root only during a restricted initialization step. It copies
the read-only Codex auth file to a private, non-persistent runtime path, prepares
the data directory, removes inherited capabilities, and then runs both `tini`
and the standalone server as UID/GID `10001`. This allows a native Linux
`~/.codex/auth.json` with mode `0600` to remain private on the host while still
being readable by the fixed non-root runtime user.

Publish the port on the host loopback address to keep the default local-only
access boundary. Mount Codex configuration read-only at `/config/codex`, while
SQLite usage data uses a separate writable named volume:

```bash
docker run --detach --rm --name oorouter \
  --publish 127.0.0.1:11434:11434 \
  --mount type=bind,src="$HOME/.codex",dst=/config/codex,readonly \
  --mount type=volume,src=oorouter-data,dst=/data \
  oorouter
```

The named volume is recommended. If a host bind mount is required for SQLite,
use a directory dedicated to this container. Initialization changes that
directory to numeric owner `10001:10001` and mode `0700`; do not point `/data`
at a shared or general-purpose directory:

```bash
mkdir -p "$HOME/.local/share/oorouter-docker"

docker run --detach --rm --name oorouter \
  --publish 127.0.0.1:11434:11434 \
  --mount type=bind,src="$HOME/.codex",dst=/config/codex,readonly \
  --mount type=bind,src="$HOME/.local/share/oorouter-docker",dst=/data \
  oorouter
```

The auth file is copied only at container startup. Restart the container after
Codex refreshes `auth.json` so the private runtime copy is refreshed.

With no key argument, client authentication is off and both Ollama and
OpenAI-compatible endpoints keep their existing behavior. To protect `/v1/*`,
repeat `--api-key` as needed. The entrypoint always appends the required
`--host 0.0.0.0` after user arguments, so overriding the image command with API
key arguments cannot accidentally restore loopback-only container binding:

```bash
API_KEY_ONE="sk-$(openssl rand -base64 96 | tr -dc 'A-Za-z0-9' | head -c 64)"
API_KEY_TWO="sk-$(openssl rand -base64 96 | tr -dc 'A-Za-z0-9' | head -c 64)"

docker run --detach --rm --name oorouter \
  --publish 127.0.0.1:11434:11434 \
  --mount type=bind,src="$HOME/.codex",dst=/config/codex,readonly \
  --mount type=volume,src=oorouter-data,dst=/data \
  oorouter \
  --api-key "$API_KEY_ONE" \
  --api-key "$API_KEY_TWO"
```

Stop the no-key example before starting the authenticated variant if you use the
same container name and port. Once the keyed container is running, the public
and protected paths can be checked independently:

```bash
curl http://127.0.0.1:11434/health
curl http://127.0.0.1:11434/api/tags
curl http://127.0.0.1:11434/v1/models \
  --header "Authorization: Bearer $API_KEY_ONE"
```

Valid Bearer keys pass registered OpenAI-compatible requests, while missing or
invalid keys receive `401 Unauthorized`. Ollama routes and `/health` remain
accessible without a key. CLI keys are runtime-only: they are not written to
`oorouter-data`, and restarting the container without `--api-key` turns client
authentication off again. The read-only Codex mount is upstream authentication
configuration and is independent of these client API keys.

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
