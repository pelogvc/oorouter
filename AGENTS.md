# AGENTS.md

oorouter (Ollama OAuth Router) — Codex OAuth 토큰으로 ChatGPT backend API를 호출하고,
클라이언트에게 Ollama/OpenAI API 형식으로 응답하는 프록시. Tauri 데스크톱 앱으로 패키징.

## 아키텍처

듀얼 스택: **Rust 백엔드** (`crates/proxy-core`) + **React 프론트엔드** (`src/`, Tauri 래핑 `src-tauri/`)

```
Client (Ollama/OpenAI API) → Proxy (axum) → ChatGPT Backend (Responses API) → Proxy → Client
```

### 디렉토리 구조

```
crates/proxy-core/       # Rust 프록시 코어 (axum, reqwest, sqlx)
  src/
    routes/              # HTTP 핸들러 (chat, generate, tags, show, openai, stubs)
    types/               # 요청/응답 타입 (ollama, codex, openai)
    client.rs            # ChatGPT backend HTTP 클라이언트
    converter.rs         # Ollama ↔ Codex 포맷 변환
    streaming.rs         # SSE → NDJSON 스트리밍 변환
    error.rs             # ProxyError enum (thiserror)
    db.rs                # SQLite (sqlx)
    config.rs            # 환경변수 기반 설정
  tests/integration/     # 통합 테스트 (mock backend)
src-tauri/               # Tauri 셸 (Rust → 프론트엔드 바인딩)
src/                     # React 프론트엔드
  components/            # Layout, shadcn/ui 컴포넌트
  pages/                 # Home, Logs, Models, Settings, TokenUsage
  lib/                   # tauri.ts (IPC), use-theme.ts, utils.ts
migrations/              # SQLite 마이그레이션
```

## 빌드 & 실행

```bash
# 프론트엔드
bun install
bun run dev                          # Vite 개발 서버 (localhost:1420)
bun run build                        # tsc -b && vite build → dist/

# Rust 백엔드
cargo build                          # 전체 워크스페이스
cargo build -p proxy-core            # 프록시 코어만
cargo run -p proxy-core --bin proxy-server  # 단독 프록시 서버 실행

# Tauri 앱 (프론트엔드 + 백엔드 통합)
cargo tauri dev                      # 개발 모드
cargo tauri build                    # 프로덕션 빌드

# Docker
docker build -t oorouter .
docker run -p 11434:11434 -v ~/.codex:/root/.codex:ro oorouter
```

## 테스트

```bash
# Rust — 전체
cargo test

# Rust — 크레이트 단위
cargo test -p proxy-core

# Rust — 단일 테스트
cargo test -p proxy-core test_default_config
cargo test -p proxy-core test_custom_port

# Rust — 통합 테스트만
cargo test -p proxy-core --test integration

# Rust — 특정 통합 테스트
cargo test -p proxy-core --test integration test_name

# 프론트엔드 — TypeScript 타입 체크
npx tsc -b --noEmit
```

## 환경변수

`.env.example` 참조:
- `PORT` — 프록시 포트 (기본 11434)
- `AUTH_PATH` — Codex 인증 파일 경로 (기본 `~/.codex/auth.json`)
- `LOG_LEVEL` — debug | info | warn | error
- `CHATGPT_API_URL` — ChatGPT backend URL
- `BACKEND` — 백엔드 타입 (현재 codex만)

## 코드 스타일

### Rust

- **에러 처리**: `thiserror` 기반 `ProxyError` enum 사용. `anyhow`는 binary에서만.
  라우트 핸들러는 `RouteResult` 타입 반환, `map_proxy_error`로 HTTP 응답 변환.
- **네이밍**: snake_case (함수/변수), PascalCase (타입/enum), UPPER_SNAKE (상수)
- **임포트 순서**: std → 외부 크레이트 → crate 내부 (`use crate::...`)
- **구조체**: `#[derive(Debug, Clone, Serialize, Deserialize)]` 패턴.
  API 타입은 `serde(rename_all = "snake_case")` 적용.
- **async**: 모든 HTTP 핸들러는 async. axum `State` extractor로 공유 상태 접근.
- **타입 정의**: `types/` 디렉토리에 API별 분리 (ollama.rs, codex.rs, openai.rs)
- **라우팅**: `routes/mod.rs`의 `create_router()`에서 모든 엔드포인트 등록

### TypeScript / React

- **TypeScript**: strict 모드. `noUnusedLocals`, `noUnusedParameters` 활성화.
- **경로 별칭**: `@/` → `src/` (tsconfig paths + vite alias)
- **임포트 순서**: react → 외부 라이브러리 → `@/lib` → `@/components` → lucide 아이콘
- **컴포넌트**: 함수형 컴포넌트 + `export default function PageName()` (페이지),
  named export (공유 컴포넌트). Props는 인라인 interface.
- **UI 라이브러리**: shadcn/ui (Radix UI + Tailwind). 컴포넌트 경로 `@/components/ui/`.
  Card, Badge, Button, Table, Select, Switch, Input, ScrollArea 사용 가능.
- **스타일링**: Tailwind v4 + CSS 변수. `cn()` 유틸로 조건부 클래스 병합.
  라이트/다크 모드 지원 (`@custom-variant dark`, `.dark` 클래스 토글).
- **Tauri IPC**: `@/lib/tauri.ts`에서 `invoke()` 래핑. 모든 Tauri 호출은 try/catch로 감싸고
  catch 블록은 빈 상태 유지 (브라우저 환경 대응).
- **아이콘**: Lucide React만 사용. 이모지를 아이콘으로 사용 금지.
- **코멘트**: 불필요한 주석 금지. 코드 자체로 의도를 표현할 것.

### 공통

- `console.log` / `println!` 디버그 출력 금지 (프로덕션 코드)
- 타입 안전성 우선: `as any`, `unwrap()` 남용 금지
- 새 의존성 추가 전 기존 라이브러리로 해결 가능한지 확인
- 기존 패턴 따르기: 새 라우트는 기존 핸들러 구조 참고, 새 페이지는 기존 페이지 구조 참고

## 주의사항

- **인증 파일** (`~/.codex/auth.json`)은 절대 커밋하지 않을 것
- **브라우저 위장 헤더**: `client.rs`의 `BROWSER_HEADERS` — ChatGPT API 호출 시 필수
- **SSE 스트리밍**: ChatGPT → SSE, Ollama → NDJSON 변환이 핵심 로직
- **SQLite**: sqlx + 마이그레이션 (`migrations/`). DB 경로 `~/.local/share/oorouter/proxy.db`
- **Tauri 윈도우**: 380×520, 장식 없음, 항상 위, 트레이 아이콘 모드 (tauri.conf.json)
