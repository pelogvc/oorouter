use std::collections::VecDeque;
use std::sync::{Arc, RwLock};


pub const MAX_LOG_ENTRIES: usize = 500;

/// Logs are never persisted to SQLite (GR4 — in-memory only).
pub type LogBuffer = Arc<RwLock<VecDeque<LogEntry>>>;

/// No request/response bodies — metadata only.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub status: u16,
    pub duration_ms: u64,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}


pub fn new_log_buffer() -> LogBuffer {
    Arc::new(RwLock::new(VecDeque::with_capacity(MAX_LOG_ENTRIES)))
}

/// Evicts oldest entry when buffer reaches MAX_LOG_ENTRIES.
pub fn push_log(buffer: &LogBuffer, entry: LogEntry) {
    let Ok(mut buf) = buffer.write() else {
        tracing::warn!("LogBuffer lock poisoned; dropping log entry");
        return;
    };
    if buf.len() >= MAX_LOG_ENTRIES {
        buf.pop_front();
    }
    buf.push_back(entry);
}

/// Returns entries newest-first.
pub fn get_recent_logs(buffer: &LogBuffer, limit: usize) -> Vec<LogEntry> {
    let Ok(buf) = buffer.read() else {
        tracing::warn!("LogBuffer lock poisoned; returning empty");
        return Vec::new();
    };
    buf.iter()
        .rev()
        .take(limit)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, status: u16) -> LogEntry {
        LogEntry {
            id: id.to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            method: "POST".to_string(),
            path: "/api/chat".to_string(),
            model: Some("gpt-5.3-codex".to_string()),
            status,
            duration_ms: 100,
            input_tokens: Some(10),
            output_tokens: Some(20),
        }
    }

    #[test]
    fn test_new_buffer_is_empty() {
        let buf = new_log_buffer();
        let logs = get_recent_logs(&buf, 10);
        assert!(logs.is_empty());
    }

    #[test]
    fn test_push_and_get() {
        let buf = new_log_buffer();
        push_log(&buf, make_entry("1", 200));
        push_log(&buf, make_entry("2", 200));
        push_log(&buf, make_entry("3", 500));

        let logs = get_recent_logs(&buf, 10);
        assert_eq!(logs.len(), 3);
        // newest first
        assert_eq!(logs[0].id, "3");
        assert_eq!(logs[1].id, "2");
        assert_eq!(logs[2].id, "1");
    }

    #[test]
    fn test_limit() {
        let buf = new_log_buffer();
        for i in 0..5 {
            push_log(&buf, make_entry(&i.to_string(), 200));
        }
        let logs = get_recent_logs(&buf, 2);
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].id, "4");
        assert_eq!(logs[1].id, "3");
    }

    #[test]
    fn test_eviction_at_max() {
        let buf = new_log_buffer();
        for i in 0..(MAX_LOG_ENTRIES + 10) {
            push_log(&buf, make_entry(&i.to_string(), 200));
        }
        let guard = buf.read().expect("lock");
        assert_eq!(guard.len(), MAX_LOG_ENTRIES);
        // oldest should be entry #10 (first 10 evicted)
        assert_eq!(guard.front().map(|e| e.id.as_str()), Some("10"));
    }

    #[test]
    fn test_serializable() {
        let entry = make_entry("test", 200);
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(json.contains("\"id\":\"test\""));
        assert!(json.contains("\"status\":200"));
    }
}
