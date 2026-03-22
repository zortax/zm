/// Parsed search query with modifiers, exact phrases, and free text.
#[derive(Debug, Clone, Default)]
pub struct ParsedQuery {
    /// Free-text words for semantic/keyword search (not quoted, not modifiers).
    pub free_text: String,
    /// Double-quoted exact phrases for SQL LIKE matching.
    pub exact_phrases: Vec<String>,
    /// Structured filters extracted from modifier prefixes.
    pub modifiers: QueryModifiers,
}

#[derive(Debug, Clone, Default)]
pub struct QueryModifiers {
    /// `in:<folder>` — filter by mailbox_name
    pub mailbox: Option<String>,
    /// `from:<value>` — filter by from_email or from_name
    pub from: Option<String>,
    /// `to:<value>` — filter by to_addresses
    pub to: Option<String>,
    /// `subject:<text>` — restrict keyword search to subject only
    pub subject: Option<String>,
}

impl ParsedQuery {
    /// True when there are keyword constraints (exact phrases, subject modifier, or free text
    /// that should be used for keyword matching in keyword-only mode).
    pub fn has_keyword_terms(&self) -> bool {
        !self.exact_phrases.is_empty() || self.modifiers.subject.is_some()
    }

    /// True when there are modifier filters that constrain the result set.
    pub fn has_modifiers(&self) -> bool {
        self.modifiers.mailbox.is_some()
            || self.modifiers.from.is_some()
            || self.modifiers.to.is_some()
            || self.modifiers.subject.is_some()
    }

    /// Collect all terms that should be used for SQL LIKE keyword matching.
    /// Includes exact phrases and, when in keyword-only mode, free text words.
    pub fn keyword_terms(&self, include_free_text: bool) -> Vec<String> {
        let mut terms: Vec<String> = self.exact_phrases.clone();
        if include_free_text && !self.free_text.is_empty() {
            // Split free text into individual words for keyword matching
            for word in self.free_text.split_whitespace() {
                terms.push(word.to_string());
            }
        }
        terms
    }
}

/// Parse a raw search query string into structured components.
///
/// Supports:
/// - Double-quoted exact phrases: `"invoice 2026"`
/// - Modifiers: `in:INBOX`, `from:alice`, `to:bob`, `subject:meeting`
/// - Modifier values can be quoted: `from:"John Doe"`
/// - Everything else becomes free text for semantic search
pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut result = ParsedQuery::default();
    let mut free_words: Vec<&str> = Vec::new();

    let chars: Vec<char> = raw.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }

        // Check for double-quoted phrase
        if chars[i] == '"' {
            i += 1;
            let start = i;
            while i < len && chars[i] != '"' {
                i += 1;
            }
            let phrase: String = chars[start..i].iter().collect();
            let phrase = phrase.trim().to_string();
            if !phrase.is_empty() {
                result.exact_phrases.push(phrase);
            }
            if i < len {
                i += 1; // skip closing quote
            }
            continue;
        }

        // Read a token (until whitespace or quote)
        let start = i;
        while i < len && !chars[i].is_whitespace() && chars[i] != '"' {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();

        // Check for modifier prefix
        if let Some(value) = try_extract_modifier(&token, &chars, &mut i, len) {
            let (prefix, val) = value;
            match prefix {
                "in" => result.modifiers.mailbox = Some(val),
                "from" => result.modifiers.from = Some(val),
                "to" => result.modifiers.to = Some(val),
                "subject" => result.modifiers.subject = Some(val),
                _ => {}
            }
        } else {
            free_words.push(&raw[byte_offset(raw, start)..byte_offset(raw, i)]);
        }
    }

    result.free_text = free_words.join(" ").trim().to_string();
    result
}

/// Try to extract a modifier like `from:value` or `from:"quoted value"`.
/// Returns Some((prefix, value)) if the token is a known modifier.
fn try_extract_modifier(
    token: &str,
    chars: &[char],
    i: &mut usize,
    len: usize,
) -> Option<(&'static str, String)> {
    let known = ["in:", "from:", "to:", "subject:"];

    let token_lower = token.to_lowercase();
    for prefix in &known {
        if token_lower.starts_with(prefix) {
            let value_part = &token[prefix.len()..];

            // If the value part is empty and next char is a quote, read quoted value
            if value_part.is_empty() && *i < len && chars[*i] == '"' {
                *i += 1; // skip opening quote
                let start = *i;
                while *i < len && chars[*i] != '"' {
                    *i += 1;
                }
                let val: String = chars[start..*i].iter().collect();
                if *i < len {
                    *i += 1; // skip closing quote
                }
                let name = &prefix[..prefix.len() - 1];
                return Some((known_prefix_name(name), val.trim().to_string()));
            }

            // Value is inline (possibly quoted)
            let val = value_part.trim_matches('"').to_string();
            if !val.is_empty() {
                let name = &prefix[..prefix.len() - 1];
                return Some((known_prefix_name(name), val));
            }

            return None;
        }
    }
    None
}

fn known_prefix_name(name: &str) -> &'static str {
    match name {
        "in" => "in",
        "from" => "from",
        "to" => "to",
        "subject" => "subject",
        _ => "in",
    }
}

/// Convert a char index to a byte offset in the original string.
fn byte_offset(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_query() {
        let q = parse_query("hello world");
        assert_eq!(q.free_text, "hello world");
        assert!(q.exact_phrases.is_empty());
        assert!(!q.has_modifiers());
    }

    #[test]
    fn exact_phrase() {
        let q = parse_query(r#""exact phrase" other words"#);
        assert_eq!(q.exact_phrases, vec!["exact phrase"]);
        assert_eq!(q.free_text, "other words");
    }

    #[test]
    fn multiple_phrases() {
        let q = parse_query(r#""first" middle "second""#);
        assert_eq!(q.exact_phrases, vec!["first", "second"]);
        assert_eq!(q.free_text, "middle");
    }

    #[test]
    fn modifier_inline() {
        let q = parse_query("from:alice in:INBOX hello");
        assert_eq!(q.modifiers.from.as_deref(), Some("alice"));
        assert_eq!(q.modifiers.mailbox.as_deref(), Some("INBOX"));
        assert_eq!(q.free_text, "hello");
    }

    #[test]
    fn modifier_quoted_value() {
        let q = parse_query(r#"from:"John Doe" meeting"#);
        assert_eq!(q.modifiers.from.as_deref(), Some("John Doe"));
        assert_eq!(q.free_text, "meeting");
    }

    #[test]
    fn subject_modifier() {
        let q = parse_query("subject:invoice from:bob");
        assert_eq!(q.modifiers.subject.as_deref(), Some("invoice"));
        assert_eq!(q.modifiers.from.as_deref(), Some("bob"));
        assert!(q.free_text.is_empty());
    }

    #[test]
    fn mixed_everything() {
        let q = parse_query(r#"in:Sent "exact match" from:alice free text"#);
        assert_eq!(q.modifiers.mailbox.as_deref(), Some("Sent"));
        assert_eq!(q.modifiers.from.as_deref(), Some("alice"));
        assert_eq!(q.exact_phrases, vec!["exact match"]);
        assert_eq!(q.free_text, "free text");
    }

    #[test]
    fn keyword_terms_without_free_text() {
        let q = parse_query(r#""invoice" hello world"#);
        let terms = q.keyword_terms(false);
        assert_eq!(terms, vec!["invoice"]);
    }

    #[test]
    fn keyword_terms_with_free_text() {
        let q = parse_query(r#""invoice" hello world"#);
        let terms = q.keyword_terms(true);
        assert_eq!(terms, vec!["invoice", "hello", "world"]);
    }
}
