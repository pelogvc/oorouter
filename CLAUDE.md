# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Codex OAuth 토큰을 사용하여 Ollama API 호환 프록시를 제공하는 TypeScript(Bun) 프로젝트.
ChatGPT Plus/Codex 인증(~/.codex/auth.json)을 통해 ChatGPT backend API를 호출하되,
클라이언트에게는 Ollama API 형식으로 응답한다. Docker로 배포 가능.

### Request Flow

```
Client (Ollama API format) → Proxy → ChatGPT Backend (Responses API) → Proxy → Client (Ollama API format)
```

### Reference

- **Ollama API 스펙**: https://github.com/ollama/ollama/blob/main/docs/api.md
- **참고 구현체**: https://github.com/Securiteru/codex-openai-proxy (OpenAI API 호환 프록시, 동일한 인증 방식 사용)

## Build & Run

```bash
bun install
bun run dev                # 개발 서버 (watch 모드)
bun run start              # 프로덕션 실행

# 환경변수로 설정
PORT=11434 AUTH_PATH=~/.codex/auth.json bun run start
```

## Test & Lint

```bash
bun test                   # 전체 테스트
bun test --watch           # watch 모드
bun test src/routes/       # 특정 디렉토리 테스트
bun test --grep "chat"     # 패턴 매칭 테스트
```

## Docker

```bash
docker build -t codex-ollama-proxy .
docker run -p 11434:11434 -v ~/.codex:/root/.codex:ro codex-ollama-proxy
```

## Architecture

### Codex Auth (`~/.codex/auth.json`)

```json
{
  "OPENAI_API_KEY": "sk-proj-...",
  "tokens": {
    "access_token": "eyJ...",
    "account_id": "uuid",
    "refresh_token": "..."
  }
}
```

인증 우선순위: `tokens.access_token` + `tokens.account_id` (ChatGPT Plus) → `OPENAI_API_KEY` (표준 OpenAI)

### ChatGPT Backend API

- Endpoint: `https://chatgpt.com/backend-api/codex/responses`
- 브라우저 위장 헤더 필수 (User-Agent, Referer, Origin, Sec-Fetch-* 등)
- `OpenAI-Beta: responses=experimental` 헤더 필요
- SSE(Server-Sent Events) 스트리밍 응답 처리

### Ollama API Endpoints (구현 대상)

| Method | Path | 설명 |
|--------|------|------|
| POST | `/api/generate` | 텍스트 생성 (단일 프롬프트) |
| POST | `/api/chat` | 채팅 완성 (메시지 배열) |
| GET | `/api/tags` | 사용 가능한 모델 목록 |
| POST | `/api/show` | 모델 상세 정보 |
| POST | `/api/embed` | 임베딩 생성 |
| GET | `/api/ps` | 로드된 모델 목록 |
| GET | `/api/version` | 버전 정보 |
| POST | `/api/copy` | 모델 복사 (stub) |
| DELETE | `/api/delete` | 모델 삭제 (stub) |
| POST | `/api/pull` | 모델 다운로드 (stub) |
| POST | `/api/push` | 모델 업로드 (stub) |

핵심 엔드포인트는 `/api/generate`, `/api/chat`, `/api/tags`이며,
나머지는 호환성을 위한 stub 응답으로 처리 가능.

### Format Conversion (핵심 로직)

**Ollama Chat Request → ChatGPT Responses API:**
```
Ollama: { model, messages: [{role, content}], stream }
  ↓ 변환
Responses API: { model, instructions, input: [{type:"message", role, content:[{type:"input_text", text}]}], stream }
```

**ChatGPT Responses API → Ollama Chat Response:**
```
SSE events (response.output_text.delta 등)
  ↓ 변환
Ollama: { model, message: {role, content}, done, total_duration, ... }
```

### Streaming

- Ollama 스트리밍: NDJSON (줄바꿈으로 구분된 JSON 객체)
- ChatGPT 스트리밍: SSE (`data: {...}\n\n` 형식)
- 스트리밍 비활성화: `"stream": false` 시 단일 JSON 응답

## Project Structure

```
src/
  index.ts          # 엔트리포인트, 서버 설정
  config.ts         # 환경변수, CLI 인자 파싱
  auth.ts           # Codex auth.json 로딩 및 토큰 관리
  routes/
    generate.ts     # POST /api/generate
    chat.ts         # POST /api/chat
    tags.ts         # GET /api/tags
    show.ts         # POST /api/show
    embed.ts        # POST /api/embed
    models.ts       # GET /api/ps, copy, delete, pull, push (stubs)
    version.ts      # GET /api/version
  codex/
    client.ts       # ChatGPT backend HTTP 클라이언트
    converter.ts    # Ollama ↔ Responses API 포맷 변환
    streaming.ts    # SSE → NDJSON 스트리밍 변환
  types/
    ollama.ts       # Ollama API 요청/응답 타입
    codex.ts        # Codex Responses API 타입
Dockerfile
docker-compose.yml
```

## Tech Stack

- **런타임**: Bun
- **언어**: TypeScript
- **HTTP 서버**: Bun.serve (내장 HTTP 서버)
- **컨테이너**: Docker (oven/bun 기반 이미지)
- **테스트**: bun:test (내장 테스트 러너)
