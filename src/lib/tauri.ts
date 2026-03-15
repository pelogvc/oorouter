import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export { invoke, listen };
export type { UnlistenFn };

export async function getServerStatus() {
  return invoke<{
    running: boolean;
    port: number;
    uptime_secs: number;
    auth_mode: string;
    error?: string;
  }>("get_server_status");
}

export async function startServer() {
  return invoke<void>("start_server");
}

export async function stopServer() {
  return invoke<void>("stop_server");
}

export async function getSettings() {
  return invoke<Array<{ key: string; value: string }>>("get_settings");
}

export async function updateSetting(key: string, value: string) {
  return invoke<void>("update_setting", { key, value });
}

export async function getRecentLogs(limit: number = 100) {
  return invoke<Array<{
    id: string;
    timestamp: string;
    method: string;
    path: string;
    model?: string;
    status: number;
    duration_ms: number;
    input_tokens?: number;
    output_tokens?: number;
  }>>("get_recent_logs", { limit });
}

export async function getTokenUsage(days: number = 7) {
  return invoke<Array<{
    date: string;
    model: string;
    input_tokens: number;
    output_tokens: number;
    total_tokens: number;
    request_count: number;
  }>>("get_token_usage", { days });
}

export async function getModels() {
  return invoke<Array<{
    id: string;
    name: string;
    visible: boolean;
    context_length: number;
    supports_vision: boolean;
  }>>("get_models");
}
