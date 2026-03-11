/// Copyright (C) 2026 Fazal Majid
///
/// This program is free software: you can redistribute it and/or modify
/// it under the terms of the GNU Affero General Public License as published by
/// the Free Software Foundation, either version 3 of the License, or
/// (at your option) any later version.
///
/// This program is distributed in the hope that it will be useful,
/// but WITHOUT ANY WARRANTY; without even the implied warranty of
/// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
/// GNU Affero General Public License for more details.
///
/// You should have received a copy of the GNU Affero General Public License
/// along with this program.  If not, see <https://www.gnu.org/licenses/>.
///
use scraper::Html;
use url_normalize::{normalize_url, NormalizeUrlError, Options};

// strip all tags completely rather than escaping them as ammonia::clean_text
pub fn clean_text(html: &str) -> String {
    Html::parse_fragment(html)
        .root_element()
        .text()
        // html5ever inserts fake </ text nodes when faced with a truncated
        // </ tag
        .map(|t| match t.find('<') {
            Some(pos) => &t[..pos],
            None => t,
        })
        .collect::<String>()
}

// sanitize URLs
pub fn clean_url(input: &str) -> Result<String, NormalizeUrlError> {
    let result = normalize_url(input, &Options::default())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_text_normal() {
        assert_eq!(
            clean_text(r#"<a href="https://blog.apnic.net/category/community/">Community</a>"#),
            "Community"
        );
    }

    #[test]
    fn test_clean_text_truncated_closing_tag() {
        assert_eq!(
            clean_text(r#"<a href="https://blog.apnic.net/category/community/">Community</"#),
            "Community"
        );
    }

    #[test]
    fn test_clean_text_truncated_at_lt() {
        assert_eq!(
            clean_text(r#"<a href="https://blog.apnic.net/category/community/">Community<"#),
            "Community"
        );
    }
}
