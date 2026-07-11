import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  listen as tauriListen,
  type EventCallback,
  type EventName,
  type Options,
  type UnlistenFn,
} from "@tauri-apps/api/event";

export type { UnlistenFn };

const DEFAULT_BROWSER_PROXY_PORT = 11434;
const BROWSER_PROXY_PORT_CANDIDATES = [DEFAULT_BROWSER_PROXY_PORT, 11435];
const BROWSER_PROXY_REQUEST_TIMEOUT_MS = 3_000;
let browserProxyUrl = `http://localhost:${DEFAULT_BROWSER_PROXY_PORT}`;
let browserServerFirstSeenAt: number | null = null;

function isRunningInTauri(): boolean {
  try {
    return isTauri();
  } catch {
    return false;
  }
}

export function listen<T>(
  event: EventName,
  handler: EventCallback<T>,
  options?: Options
): Promise<UnlistenFn> {
  if (!isRunningInTauri()) {
    return Promise.resolve(() => undefined);
  }

  return tauriListen(event, handler, options);
}

export function isTauriRuntime(): boolean {
  return isRunningInTauri();
}

export interface ServerStatus {
  running: boolean;
  port: number;
  uptime_secs: number;
  auth_mode: string;
  error?: string;
}

export interface Setting {
  key: string;
  value: string;
}

export interface LogEntry {
  id: string;
  timestamp: string;
  method: string;
  path: string;
  model?: string;
  status: number;
  duration_ms: number;
  input_tokens?: number;
  output_tokens?: number;
}

export interface TokenUsageRow {
  date: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  request_count: number;
}

export interface Model {
  id: string;
  name: string;
  visible: boolean;
  context_length: number;
  supports_vision: boolean;
}

export type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "installing"
  | "installed"
  | "error";

export interface UpdateState {
  status: UpdateStatus;
  currentVersion: string;
  version?: string;
  date?: string;
  body?: string;
  downloadedBytes: number;
  contentLength?: number;
  error?: string;
  visible: boolean;
  manual: boolean;
}

interface OllamaTagsResponse {
  models: unknown[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function readString(record: Record<string, unknown>, key: string): string {
  const value = record[key];
  if (typeof value !== "string") {
    throw new Error(`Invalid ${key}: expected string`);
  }
  return value;
}

function readOptionalString(record: Record<string, unknown>, key: string): string | undefined {
  const value = record[key];
  if (value === null || value === undefined) return undefined;
  if (typeof value !== "string") {
    throw new Error(`Invalid ${key}: expected string`);
  }
  return value;
}

function readNumber(record: Record<string, unknown>, key: string): number {
  const value = record[key];
  if (typeof value !== "number" || !Number.isSafeInteger(value)) {
    throw new Error(`Invalid ${key}: expected safe integer`);
  }
  return value;
}

function readOptionalNumber(record: Record<string, unknown>, key: string): number | undefined {
  const value = record[key];
  if (value === null || value === undefined) return undefined;
  if (typeof value !== "number" || !Number.isSafeInteger(value)) {
    throw new Error(`Invalid ${key}: expected safe integer`);
  }
  return value;
}

function readBoolean(record: Record<string, unknown>, key: string): boolean {
  const value = record[key];
  if (typeof value !== "boolean") {
    throw new Error(`Invalid ${key}: expected boolean`);
  }
  return value;
}

function readArray<T>(value: unknown, parser: (item: unknown) => T): T[] {
  if (!Array.isArray(value)) {
    throw new Error("Invalid response: expected array");
  }
  return value.map(parser);
}

function parseRecord(value: unknown): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error("Invalid response: expected object");
  }
  return value;
}

function parseServerStatus(value: unknown): ServerStatus {
  const record = parseRecord(value);
  return {
    running: readBoolean(record, "running"),
    port: readNumber(record, "port"),
    uptime_secs: readNumber(record, "uptime_secs"),
    auth_mode: readString(record, "auth_mode"),
    error: readOptionalString(record, "error"),
  };
}

function parseSetting(value: unknown): Setting {
  const record = parseRecord(value);
  return {
    key: readString(record, "key"),
    value: readString(record, "value"),
  };
}

function parseLogEntry(value: unknown): LogEntry {
  const record = parseRecord(value);
  return {
    id: readString(record, "id"),
    timestamp: readString(record, "timestamp"),
    method: readString(record, "method"),
    path: readString(record, "path"),
    model: readOptionalString(record, "model"),
    status: readNumber(record, "status"),
    duration_ms: readNumber(record, "duration_ms"),
    input_tokens: readOptionalNumber(record, "input_tokens"),
    output_tokens: readOptionalNumber(record, "output_tokens"),
  };
}

function parseTokenUsageRow(value: unknown): TokenUsageRow {
  const record = parseRecord(value);
  return {
    date: readString(record, "date"),
    model: readString(record, "model"),
    input_tokens: readNumber(record, "input_tokens"),
    output_tokens: readNumber(record, "output_tokens"),
    total_tokens: readNumber(record, "total_tokens"),
    request_count: readNumber(record, "request_count"),
  };
}

function parseModel(value: unknown): Model {
  const record = parseRecord(value);
  return {
    id: readString(record, "id"),
    name: readString(record, "name"),
    visible: readBoolean(record, "visible"),
    context_length: readNumber(record, "context_length"),
    supports_vision: readBoolean(record, "supports_vision"),
  };
}

function parseUpdateStatus(value: string): UpdateStatus {
  if (
    value === "idle" ||
    value === "checking" ||
    value === "available" ||
    value === "installing" ||
    value === "installed" ||
    value === "error"
  ) {
    return value;
  }
  throw new Error(`Invalid update status: ${value}`);
}

export function parseUpdateState(value: unknown): UpdateState {
  const record = parseRecord(value);
  return {
    status: parseUpdateStatus(readString(record, "status")),
    currentVersion: readString(record, "currentVersion"),
    version: readOptionalString(record, "version"),
    date: readOptionalString(record, "date"),
    body: readOptionalString(record, "body"),
    downloadedBytes: readNumber(record, "downloadedBytes"),
    contentLength: readOptionalNumber(record, "contentLength"),
    error: readOptionalString(record, "error"),
    visible: readBoolean(record, "visible"),
    manual: readBoolean(record, "manual"),
  };
}

function parseOllamaTagsResponse(value: unknown): OllamaTagsResponse {
  const record = parseRecord(value);
  const models = record.models;
  if (!Array.isArray(models)) {
    throw new Error("Invalid models: expected array");
  }
  return { models };
}

function browserModelContextLength(id: string): number {
  if (id === "gpt-5.6-sol" || id === "gpt-5.6-terra" || id === "gpt-5.6-luna") {
    return 372_000;
  }
  if (id === "gpt-5.5" || id === "gpt-5.4" || id === "gpt-5.4-pro") {
    return 1_050_000;
  }
  if (id === "gpt-5.3-codex-spark") {
    return 128_000;
  }
  return 400_000;
}

function parseBrowserModel(value: unknown): Model {
  const record = parseRecord(value);
  const id = readString(record, "name").replace(/:latest$/, "");
  return {
    id,
    name: id,
    visible: true,
    context_length: browserModelContextLength(id),
    supports_vision: !id.includes("spark"),
  };
}

function getBrowserProxyPort(): number {
  const parsed = new URL(browserProxyUrl);
  const port = Number(parsed.port);
  return Number.isInteger(port) ? port : DEFAULT_BROWSER_PROXY_PORT;
}

function getBrowserProxyUrls(): string[] {
  return [
    browserProxyUrl,
    ...BROWSER_PROXY_PORT_CANDIDATES.map((port) => `http://localhost:${port}`),
  ].filter((url, index, urls) => urls.indexOf(url) === index);
}

async function fetchProxyJson(path: string): Promise<unknown> {
  let lastError: unknown = null;
  for (const url of getBrowserProxyUrls()) {
    const controller = new AbortController();
    const timeout = window.setTimeout(
      () => controller.abort(),
      BROWSER_PROXY_REQUEST_TIMEOUT_MS
    );
    try {
      const response = await fetch(`${url}${path}`, { signal: controller.signal });
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      browserProxyUrl = url;
      return response.json();
    } catch (error) {
      lastError = error;
    } finally {
      window.clearTimeout(timeout);
    }
  }

  throw lastError instanceof Error ? lastError : new Error(String(lastError));
}

export async function getServerStatus(): Promise<ServerStatus> {
  if (!isRunningInTauri()) {
    try {
      await fetchProxyJson("/api/version");
      browserServerFirstSeenAt ??= Date.now();
      return {
        running: true,
        port: getBrowserProxyPort(),
        uptime_secs: Math.floor((Date.now() - browserServerFirstSeenAt) / 1000),
        auth_mode: "Browser",
      };
    } catch (err) {
      browserServerFirstSeenAt = null;
      return {
        running: false,
        port: getBrowserProxyPort(),
        uptime_secs: 0,
        auth_mode: "Browser",
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  return parseServerStatus(await invoke<unknown>("get_server_status"));
}

export async function startServer() {
  if (!isRunningInTauri()) {
    throw new Error("Start the server from the Tauri app or run cargo tauri dev.");
  }
  return invoke<void>("start_server");
}

export async function stopServer() {
  if (!isRunningInTauri()) {
    throw new Error("Stop the server from the Tauri app.");
  }
  return invoke<void>("stop_server");
}

export async function getSettings(): Promise<Setting[]> {
  if (!isRunningInTauri()) {
    return [
      { key: "port", value: String(getBrowserProxyPort()) },
      { key: "auth_path", value: "~/.codex/auth.json" },
      { key: "auto_start", value: "true" },
    ];
  }
  return readArray(await invoke<unknown>("get_settings"), parseSetting);
}

export async function updateSetting(key: string, value: string) {
  if (!isRunningInTauri()) {
    void key;
    void value;
    throw new Error("Settings can only be updated from the Tauri app.");
  }
  return invoke<void>("update_setting", { key, value });
}

export async function getRecentLogs(limit: number = 100): Promise<LogEntry[]> {
  if (!isRunningInTauri()) {
    return readArray(
      await fetchProxyJson(`/api/logs?limit=${encodeURIComponent(limit)}`),
      parseLogEntry
    );
  }
  return readArray(await invoke<unknown>("get_recent_logs", { limit }), parseLogEntry);
}

export async function getTokenUsage(days: number = 7): Promise<TokenUsageRow[]> {
  if (!isRunningInTauri()) {
    return readArray(
      await fetchProxyJson(`/api/token-usage?days=${encodeURIComponent(days)}`),
      parseTokenUsageRow
    );
  }
  return readArray(await invoke<unknown>("get_token_usage", { days }), parseTokenUsageRow);
}

export async function getModels(): Promise<Model[]> {
  if (!isRunningInTauri()) {
    const data = parseOllamaTagsResponse(await fetchProxyJson("/api/tags"));
    return data.models.map(parseBrowserModel);
  }

  return readArray(await invoke<unknown>("get_models"), parseModel);
}

export async function getUpdateState(): Promise<UpdateState> {
  if (!isRunningInTauri()) {
    return {
      status: "idle",
      currentVersion: "browser",
      downloadedBytes: 0,
      visible: false,
      manual: false,
    };
  }

  return parseUpdateState(await invoke<unknown>("get_update_state"));
}

export async function checkForUpdates(manual = false): Promise<UpdateState> {
  if (!isRunningInTauri()) {
    const state = await getUpdateState();
    return { ...state, manual };
  }

  return parseUpdateState(await invoke<unknown>("check_for_updates", { manual }));
}

export async function installUpdate(): Promise<UpdateState> {
  if (!isRunningInTauri()) {
    throw new Error("Updates can only be installed from the Tauri app.");
  }

  return parseUpdateState(await invoke<unknown>("install_update"));
}

export async function restartApp(): Promise<void> {
  if (!isRunningInTauri()) {
    throw new Error("Restart the desktop app manually.");
  }

  return invoke<void>("restart_app");
}
