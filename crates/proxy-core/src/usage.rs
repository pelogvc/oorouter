use crate::{db::Database, types::codex::CodexSSEResponseUsage};

pub fn token_count_for_log(value: u64, field: &str, model: &str) -> Option<u32> {
    match u32::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => {
            tracing::warn!(
                field,
                model,
                value,
                "token count exceeds u32; omitting from request log"
            );
            None
        }
    }
}

pub fn token_count_for_db(value: u64, field: &str, model: &str) -> Option<i64> {
    match i64::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => {
            tracing::warn!(
                field,
                model,
                value,
                "token count exceeds i64; omitting usage row"
            );
            None
        }
    }
}

pub fn usage_counts_for_log(
    usage: Option<&CodexSSEResponseUsage>,
    model: &str,
) -> (Option<u32>, Option<u32>) {
    match usage {
        Some(usage) => (
            token_count_for_log(usage.input_tokens, "input_tokens", model),
            token_count_for_log(usage.output_tokens, "output_tokens", model),
        ),
        None => (None, None),
    }
}

pub async fn record_token_usage(
    db: &Database,
    model: &str,
    path: &str,
    usage: &CodexSSEResponseUsage,
) {
    let Some(input_tokens) = token_count_for_db(usage.input_tokens, "input_tokens", model) else {
        return;
    };
    let Some(output_tokens) = token_count_for_db(usage.output_tokens, "output_tokens", model)
    else {
        return;
    };

    if let Err(error) = db
        .insert_token_usage(model, "codex", input_tokens, output_tokens, path)
        .await
    {
        tracing::warn!(%error, model, path, "failed to insert token usage");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input_tokens: u64, output_tokens: u64) -> CodexSSEResponseUsage {
        CodexSSEResponseUsage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens.saturating_add(output_tokens),
            input_tokens_details: None,
            output_tokens_details: None,
        }
    }

    #[test]
    fn usage_counts_for_log_omits_values_that_do_not_fit_u32() {
        let usage = usage(u64::from(u32::MAX) + 1, 17);
        let (input, output) = usage_counts_for_log(Some(&usage), "gpt-5.3-codex");
        assert_eq!(input, None);
        assert_eq!(output, Some(17));
    }
}
