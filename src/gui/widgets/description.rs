//! Clickable package description rendering.
//!
//! We convert bare URLs to markdown links, then render via Iced's markdown widget.

use iced::widget::markdown;
use std::path::PathBuf;

/// Convert bare URLs into markdown links (`[url](url)`).
fn convert_urls_to_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut last = 0;

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

        // Append text before the URL
        out.push_str(&text[last..i]);

        // URL ends at first whitespace (or end of string)
        let end_rel = remaining
            .find(|c: char| c.is_whitespace())
            .unwrap_or(remaining.len());

        let raw_url = &remaining[..end_rel];
        let (url, trailing) = split_trailing_punctuation(raw_url);

        // Convert to markdown link format: [url](url)
        out.push('[');
        out.push_str(url);
        out.push_str("](");
        out.push_str(url);
        out.push(')');
        out.push_str(trailing);

        // Fast-forward iterator to the end of the consumed URL.
        let next = i + end_rel;
        last = next;
        while let Some(&(j, _)) = it.peek() {
            if j < next {
                it.next();
            } else {
                break;
            }
        }
    }

    out.push_str(&text[last..]);
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

/// Parsed description content ready for rendering.
///
/// This caches the parsed markdown items to avoid re-parsing on every frame.
#[derive(Debug, Clone, Default)]
pub struct DescriptionContent {
    items: Vec<markdown::Item>,
}

impl DescriptionContent {
    /// Parses a description string and caches the result.
    ///
    /// URLs are automatically converted to clickable markdown links.
    pub fn parse(description: &str) -> Self {
        let markdown_text = convert_urls_to_markdown(description);
        let items: Vec<markdown::Item> = markdown::parse(&markdown_text).collect();
        Self { items }
    }

    /// Returns the cached markdown items for rendering.
    pub fn items(&self) -> &[markdown::Item] {
        &self.items
    }
}

/// Converts a markdown `Uri` to a `PathBuf` for the URL opener.
pub fn uri_to_path(uri: &str) -> PathBuf {
    PathBuf::from(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_urls() {
        let result = convert_urls_to_markdown("This is plain text");
        assert_eq!(result, "This is plain text");
    }

    #[test]
    fn test_single_url() {
        let result = convert_urls_to_markdown("Check out https://example.com for more");
        assert_eq!(
            result,
            "Check out [https://example.com](https://example.com) for more"
        );
    }

    #[test]
    fn test_url_with_trailing_period() {
        let result = convert_urls_to_markdown("Visit https://example.com.");
        assert_eq!(result, "Visit [https://example.com](https://example.com).");
    }

    #[test]
    fn test_multiple_urls() {
        let result = convert_urls_to_markdown("See https://one.com and https://two.com");
        assert_eq!(
            result,
            "See [https://one.com](https://one.com) and [https://two.com](https://two.com)"
        );
    }

    #[test]
    fn test_url_in_parentheses() {
        let result = convert_urls_to_markdown("App (https://play.google.com)");
        assert_eq!(
            result,
            "App ([https://play.google.com](https://play.google.com))"
        );
    }
}
