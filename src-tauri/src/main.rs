#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    codex_ollama_proxy_lib::run()
}
