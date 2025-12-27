//! Package description helpers (selection + link extraction).

use iced::widget::text_editor;
use std::path::PathBuf;

/// Extract all `http(s)://` URLs in display order (deduplicated).
fn extract_urls(text: &str) -> Vec<String> {
    use std::collections::HashSet;

    let mut out = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    // Single pass over valid UTF-8 boundaries.
    let mut it = text.char_indices().peekable();
    while let Some((i, ch)) = it.next() {
        if ch != 'h' {
            continue;
        }

        let remaining = &text[i..];
        if !remaining.starts_with("https://") && !remaining.starts_with("http://") {
            continue;
        }

        // URL ends at first whitespace (or end of string)
        let end_rel = remaining
            .find(|c: char| c.is_whitespace())
            .unwrap_or(remaining.len());

        let raw_url = &remaining[..end_rel];
        let (url, _trailing) = split_trailing_punctuation(raw_url);

        if seen.insert(url.to_string()) {
            out.push(url.to_string());
        }

        // Fast-forward iterator to the end of the consumed URL.
        let next = i + end_rel;
        while let Some(&(j, _)) = it.peek() {
            if j < next {
                it.next();
            } else {
                break;
            }
        }
    }

    out
}

/// Split common trailing punctuation from a URL.
///
/// Returns `(url, trailing_punctuation)`.
fn split_trailing_punctuation(url: &str) -> (&str, &str) {
    let bytes = url.as_bytes();
    let mut cut = bytes.len();

    while cut > 0 {
        let b = bytes[cut - 1];
        let is_punct = matches!(b, b')' | b',' | b'.' | b';' | b':' | b'!' | b'?');

        if !is_punct {
            break;
        }

        // Keep trailing ')' if the URL already contains '(' (e.g., some wiki URLs).
        if b == b')' && url[..cut].contains('(') {
            break;
        }

        cut -= 1;
    }

    (&url[..cut], &url[cut..])
}

#[derive(Debug, Clone, Default)]
pub struct DescriptionContent {
    content: text_editor::Content,
    links: Vec<String>,
}

impl DescriptionContent {
    pub fn parse(description: &str) -> Self {
        Self {
            content: text_editor::Content::with_text(description),
            links: extract_urls(description),
        }
    }

    pub fn content(&self) -> &text_editor::Content {
        &self.content
    }

    pub fn links(&self) -> &[String] {
        &self.links
    }

    pub fn perform(&mut self, action: text_editor::Action) {
        self.content.perform(action);
    }
}

pub fn url_to_path(url: &str) -> PathBuf {
    PathBuf::from(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_urls() {
        let urls = extract_urls("This is plain text");
        assert!(urls.is_empty());
    }

    #[test]
    fn test_single_url() {
        let urls = extract_urls("Check out https://example.com for more");
        assert_eq!(urls, vec!["https://example.com".to_string()]);
    }

    #[test]
    fn test_url_with_trailing_period() {
        let urls = extract_urls("Visit https://example.com.");
        assert_eq!(urls, vec!["https://example.com".to_string()]);
    }

    #[test]
    fn test_multiple_urls() {
        let urls = extract_urls("See https://one.com and https://two.com");
        assert_eq!(
            urls,
            vec!["https://one.com".to_string(), "https://two.com".to_string()]
        );
    }

    #[test]
    fn test_url_in_parentheses() {
        let urls = extract_urls("App (https://play.google.com)");
        assert_eq!(urls, vec!["https://play.google.com".to_string()]);
    }
}
