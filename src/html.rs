use std::{borrow::Cow, sync::LazyLock};

use clearurls::UrlCleaner;
use scraper::Selector;

static LINKS: LazyLock<Selector> = LazyLock::new(|| Selector::parse("a").unwrap());
// The ruleset is hundreds of regexes; compiling it per link made opening links noticeably slow.
static CLEANER: LazyLock<Option<UrlCleaner>> = LazyLock::new(|| UrlCleaner::from_embedded_rules().ok());

pub fn strip_html(html: &str) -> String {
	let fragment = scraper::Html::parse_fragment(html);
	let mut output = String::new();
	for child in fragment.root_element().children() {
		append_text(child, &mut output);
	}
	normalize_text(&output)
}

fn append_text(node: ego_tree::NodeRef<scraper::node::Node>, output: &mut String) {
	match node.value() {
		scraper::node::Node::Text(text) => {
			// Newlines right after a <br> or block boundary are source formatting
			// (e.g. `</p>\n<p>`), not extra blank lines.
			if output.is_empty() || output.ends_with('\n') {
				if text.contains('\n') && text.trim_ascii().is_empty() {
					return;
				}
				output.push_str(text.trim_start_matches(['\r', '\n']));
			} else {
				output.push_str(text);
			}
		}
		scraper::node::Node::Element(element) => {
			let name = element.name();
			if name == "br" {
				output.push('\n');
				return;
			}
			let is_paragraph = matches!(name, "p" | "blockquote" | "pre" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
			let is_other_block =
				matches!(name, "div" | "li" | "ul" | "ol" | "section" | "article" | "header" | "footer");
			if is_paragraph {
				push_paragraph_break(output);
			} else if is_other_block {
				push_newline(output);
			}
			for child in node.children() {
				append_text(child, output);
			}
			if is_paragraph {
				push_paragraph_break(output);
			} else if is_other_block {
				push_newline(output);
			}
		}
		_ => {}
	}
}

fn push_newline(output: &mut String) {
	if !output.ends_with('\n') {
		output.push('\n');
	}
}

fn push_paragraph_break(output: &mut String) {
	if output.is_empty() {
		return;
	}
	if !output.ends_with('\n') {
		output.push_str("\n\n");
	} else if !output.ends_with("\n\n") {
		output.push('\n');
	}
}

fn normalize_text(input: &str) -> String {
	let mut output = String::with_capacity(input.len());
	let mut last_was_space = false;
	let mut last_was_newline = false;
	for ch in input.chars() {
		match ch {
			'\r' => {}
			'\n' => {
				output.push('\n');
				last_was_space = false;
				last_was_newline = true;
			}
			// Non-breaking space: always preserve (Mastodon uses &nbsp; for indentation)
			'\u{00A0}' => {
				output.push(ch);
				last_was_space = false;
				last_was_newline = false;
			}
			c if c.is_whitespace() => {
				if last_was_newline {
					// Preserve indentation at the start of a line
					output.push(c);
				} else if !last_was_space {
					output.push(' ');
					last_was_space = true;
				}
			}
			c => {
				output.push(c);
				last_was_space = false;
				last_was_newline = false;
			}
		}
	}
	let mut cleaned = String::new();
	for line in output.lines() {
		if !cleaned.is_empty() {
			cleaned.push('\n');
		}
		cleaned.push_str(line.trim_end()); // trim_end preserves leading indentation
	}
	let mut final_out = String::new();
	let mut blank_run = 0;
	for line in cleaned.split('\n') {
		if line.is_empty() {
			blank_run += 1;
			if blank_run <= 2 && !final_out.is_empty() {
				final_out.push('\n');
			}
		} else {
			blank_run = 0;
			if !final_out.is_empty() {
				final_out.push('\n');
			}
			final_out.push_str(line);
		}
	}
	final_out.trim().to_string()
}

pub fn clean_url(raw: &str) -> String {
	let Some(cleaner) = CLEANER.as_ref() else {
		return raw.to_string();
	};
	cleaner.clear_single_url_str(raw).map_or_else(|_| raw.to_string(), Cow::into_owned)
}

#[derive(Debug, Clone)]
pub struct Link {
	pub url: String,
}

/// Extract links that are Mastodon `@mention`s (elements with `class="mention"`),
/// returning `(href, display_text)` pairs. Used to find mentions the API did not resolve.
pub fn extract_mention_links(html: &str) -> Vec<(String, String)> {
	let fragment = scraper::Html::parse_fragment(html);
	let mut result = Vec::new();
	for element in fragment.select(&LINKS) {
		let Some(href) = element.value().attr("href") else { continue };
		let class_attr = element.value().attr("class").unwrap_or("");
		let is_mention = class_attr.split_whitespace().any(|c| c == "mention")
			&& !class_attr.split_whitespace().any(|c| c == "hashtag");
		if !is_mention {
			continue;
		}
		let text: String = element.text().collect();
		result.push((href.to_string(), text.trim().to_string()));
	}
	result
}

pub fn extract_links(html: &str) -> Vec<Link> {
	let fragment = scraper::Html::parse_fragment(html);
	let mut links = Vec::new();
	for element in fragment.select(&LINKS) {
		if let Some(href) = element.value().attr("href") {
			if let Some(class) = element.value().attr("class") {
				let classes: Vec<&str> = class.split_whitespace().collect();
				if classes.contains(&"mention") || classes.contains(&"hashtag") {
					continue;
				}
			}
			links.push(Link { url: href.to_string() });
		}
	}
	links
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn preserves_single_blank_line_between_paragraphs() {
		let html = "<p>Line 1<br>Line 2</p><p>Line 4</p>";
		let text = strip_html(html);
		assert_eq!(text, "Line 1\nLine 2\n\nLine 4");
	}

	#[test]
	fn preserves_multiple_blank_lines() {
		let html = "<p>Line 1</p><p><br></p><p>Line 4</p>";
		let text = strip_html(html);
		assert_eq!(text, "Line 1\n\n\nLine 4");
	}

	#[test]
	fn preserves_multiple_br_tags() {
		let html = "<p>Line 1<br><br>Line 3</p>";
		let text = strip_html(html);
		assert_eq!(text, "Line 1\n\nLine 3");
	}

	#[test]
	fn caps_excessive_blank_lines_at_two() {
		let html = "<p>Line 1<br><br><br><br><br>Line 2</p>";
		let text = strip_html(html);
		assert_eq!(text, "Line 1\n\n\nLine 2");
	}

	#[test]
	fn handles_nested_divs_and_paragraphs() {
		let html = "<div><p>Hello</p><p>World</p></div>";
		let text = strip_html(html);
		assert_eq!(text, "Hello\n\nWorld");
	}

	#[test]
	fn ignores_source_newlines_between_tags() {
		let html = "<p>Line 1<br>\nLine 2</p>\n<p>Line 4</p>\r\n";
		let text = strip_html(html);
		assert_eq!(text, "Line 1\nLine 2\n\nLine 4");
	}

	#[test]
	fn handles_list_items() {
		let html = "<ul><li>Item 1</li><li>Item 2</li></ul>";
		let text = strip_html(html);
		assert_eq!(text, "Item 1\nItem 2");
	}

	#[test]
	fn preserves_leading_indentation_with_blank_lines() {
		let html = "<p>Line 1<br>&nbsp;&nbsp;Indented</p><p>&nbsp;&nbsp;Next Paragraph</p>";
		let text = strip_html(html);
		assert_eq!(text, "Line 1\n\u{00A0}\u{00A0}Indented\n\n\u{00A0}\u{00A0}Next Paragraph");
	}
}
