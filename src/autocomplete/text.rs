//! Mention editing uses UTF-8 byte offsets; native positions are adapted at the UI boundary.
use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
pub struct MentionEdit {
	pub range: Range<usize>,
	pub filter: String,
	pub leading_space: bool,
}

fn opening(c: char) -> bool {
	c.is_whitespace() || "([{\"',:;!?".contains(c)
}

fn closing(c: char) -> bool {
	")]}\"',:;!?.".contains(c)
}

fn component(c: char, domain: bool) -> bool {
	c.is_alphanumeric() || if domain { c == '-' } else { c == '_' }
}

fn mention_end(text: &str, start: usize) -> usize {
	let mut end = start + 1;
	let mut domain = false;
	let mut previous = false;
	let mut chars = text[end..].char_indices().peekable();
	while let Some((offset, c)) = chars.next() {
		if c == '@' && !domain {
			domain = true;
			previous = false;
		} else if component(c, domain) {
			previous = true;
		} else if c == '.' && previous && chars.peek().is_some_and(|(_, next)| component(*next, domain)) {
			previous = false;
		} else {
			break;
		}
		end = start + 1 + offset + c.len_utf8();
	}
	end
}

pub fn prepare(text: &str, caret: usize, selection: Range<usize>) -> MentionEdit {
	assert!(text.is_char_boundary(caret));
	if !selection.is_empty() {
		let selected = &text[selection.clone()];
		let filter = if selected.starts_with('@') && mention_end(selected, 0) == selected.len() {
			selected.to_owned()
		} else {
			String::new()
		};
		return MentionEdit { range: selection, filter, leading_space: false };
	}
	// Scan only the token touching the caret, never an earlier whitespace-delimited token.
	let token_start =
		text[..caret].rfind(char::is_whitespace).map_or(0, |i| i + text[i..].chars().next().unwrap().len_utf8());
	for (offset, c) in text[token_start..caret].char_indices() {
		let start = token_start + offset;
		let prefix = &text[token_start..start];
		if prefix.contains("://") || prefix.starts_with("mailto:") || prefix.starts_with("www.") {
			break;
		}
		if c == '@' && (start == 0 || text[..start].chars().next_back().is_some_and(opening)) {
			let end = mention_end(text, start);
			if caret <= end {
				return MentionEdit { range: start..end, filter: text[start..caret].to_owned(), leading_space: false };
			}
		}
	}
	let end = text[caret..].find(char::is_whitespace).map_or(text.len(), |i| caret + i);
	let word_end = token_start + text[token_start..end].trim_end_matches(closing).len();
	if caret > token_start && caret <= word_end {
		return MentionEdit { range: word_end..word_end, filter: String::new(), leading_space: true };
	}
	MentionEdit { range: caret..caret, filter: String::new(), leading_space: false }
}

impl MentionEdit {
	/// Replacement and following space are passed to the native control as one undoable edit.
	pub fn replacement(&self, original: &str, address: &str) -> (String, usize) {
		let mut replacement = if self.leading_space { format!(" {address}") } else { address.to_owned() };
		let caret = self.range.start + replacement.len();
		if original[self.range.end..].chars().next().is_some_and(|c| !c.is_whitespace() && !closing(c)) {
			replacement.push(' ');
		}
		(replacement, caret)
	}
}

/// wxMSW multiline EDIT positions count CRLF as two UTF-16 units, even though `GetValue` returns LF.
pub fn native_to_byte(text: &str, position: usize, crlf: bool) -> usize {
	let mut native = 0;
	for (byte, c) in text.char_indices() {
		let width = c.len_utf16() + usize::from(crlf && c == '\n');
		if native + width > position {
			return byte;
		}
		native += width;
	}
	text.len()
}

pub fn byte_to_native(text: &str, byte: usize, crlf: bool) -> usize {
	text[..byte].chars().map(|c| c.len_utf16() + usize::from(crlf && c == '\n')).sum()
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn design_examples() {
		for (input, filter, output) in [
			("Hello @al|", "@al", "Hello @alice@example.org|"),
			("Hello @al|ice", "@al", "Hello @alice@example.org|"),
			("Hello @|", "@", "Hello @alice@example.org|"),
			("@alice@exa|mple.net", "@alice@exa", "@alice@example.org|"),
			("(@al|), thanks", "@al", "(@alice@example.org|), thanks"),
			("Hello @bob, how ar|e", "", "Hello @bob, how are @alice@example.org|"),
			("hel|lo, friend", "", "hello @alice@example.org|, friend"),
			("Hello |world", "", "Hello @alice@example.org| world"),
			("mail a@exa|mple.org", "", "mail a@example.org @alice@example.org|"),
			("https://a/@al|ice", "", "https://a/@alice @alice@example.org|"),
			("https://a/?@al|ice", "", "https://a/?@alice @alice@example.org|"),
			("mailto:@al|ice", "", "mailto:@alice @alice@example.org|"),
			("😀\n@a|.", "@a", "😀\n@alice@example.org|."),
		] {
			let caret = input.find('|').unwrap();
			let mut text = input.replace('|', "");
			let edit = prepare(&text, caret, caret..caret);
			assert_eq!(edit.filter, filter, "{input}");
			let (replacement, new_caret) = edit.replacement(&text, "@alice@example.org");
			text.replace_range(edit.range, &replacement);
			text.insert(new_caret, '|');
			assert_eq!(text, output, "{input}");
		}
	}
	#[test]
	fn selections_and_native_positions() {
		assert_eq!(prepare("hi @alice!", 6, 3..6).filter, "@al");
		assert_eq!(prepare("hi @alice!", 6, 0..6).filter, "");
		for crlf in [true, false] {
			let text = "😀 hi\n@é\nend";
			for byte in text.char_indices().map(|(i, _)| i).chain([text.len()]) {
				assert_eq!(native_to_byte(text, byte_to_native(text, byte, crlf), crlf), byte);
			}
		}
	}
}
