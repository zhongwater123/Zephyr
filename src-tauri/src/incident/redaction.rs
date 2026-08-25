pub fn redact_sensitive(input: &str) -> String {
    input
        .lines()
        .map(redact_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if ["authorization", "api_key", "api-key", "apikey", "x-api-key"]
        .iter()
        .any(|marker| lower.contains(marker))
        || line
            .split_whitespace()
            .any(|token| token.to_ascii_lowercase().starts_with("sk-"))
    {
        return "[REDACTED]".to_string();
    }

    line.split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if token.contains(":\\") || token.starts_with('/') {
                "[LOCAL_PATH]".to_string()
            } else if (lower.starts_with("http://") || lower.starts_with("https://"))
                && token.contains('?')
            {
                format!(
                    "{}?[QUERY_REDACTED]",
                    token.split('?').next().unwrap_or_default()
                )
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_header_redacts_the_whole_line() {
        assert_eq!(
            redact_sensitive("Authorization: Bearer secret"),
            "[REDACTED]"
        );
    }
}
