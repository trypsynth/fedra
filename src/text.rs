use pulldown_cmark::{Event, Parser, TagEnd};

use crate::config::DisplayNameEmojiMode;

pub fn markdown_to_text(markdown: &str) -> String {
	let mut text = String::new();
	let parser = Parser::new(markdown);
	for event in parser {
		match event {
			Event::Text(t) => {
				text.push_str(&t);
			}
			Event::End(TagEnd::Paragraph | TagEnd::Heading(_)) => {
				text.push_str(
					"

",
				);
			}
			Event::End(TagEnd::Item) => {
				text.push('\n');
			}
			_ => {}
		}
	}
	let mut result = format!(" {}", text.trim());
	loop {
		let original_len = result.len();
		if let Some(start) = result.find(" #")
			&& let Some(substr) = result.get(start + 2..)
		{
			let num_len = substr.chars().take_while(char::is_ascii_digit).count();
			if num_len > 0 {
				let mut end = start + 2 + num_len;
				if let Some(after_num) = result.get(end..)
					&& (after_num.starts_with(',')
						|| (after_num.starts_with('.')
							&& after_num.get(1..).is_none_or(|s| s.starts_with(char::is_whitespace))))
				{
					end += 1;
				}
				result.replace_range(start..end, "");
			}
		}
		if result.len() == original_len {
			break;
		}
	}
	result.trim_start().to_string()
}

pub fn strip_display_name_emojis(name: &str, mode: DisplayNameEmojiMode) -> String {
	let with_instance_filtered = match mode {
		DisplayNameEmojiMode::InstanceOnly | DisplayNameEmojiMode::All => strip_instance_shortcodes(name),
		DisplayNameEmojiMode::None | DisplayNameEmojiMode::UnicodeOnly => name.to_string(),
	};
	let with_unicode_filtered = match mode {
		DisplayNameEmojiMode::UnicodeOnly | DisplayNameEmojiMode::All => {
			strip_unicode_emoji_chars(&with_instance_filtered)
		}
		DisplayNameEmojiMode::None | DisplayNameEmojiMode::InstanceOnly => with_instance_filtered,
	};
	normalize_spaces(&with_unicode_filtered)
}

fn strip_instance_shortcodes(input: &str) -> String {
	let chars: Vec<char> = input.chars().collect();
	let mut output = String::with_capacity(input.len());
	let mut i = 0;
	while i < chars.len() {
		if chars[i] == ':' {
			let mut j = i + 1;
			while j < chars.len() && chars[j] != ':' {
				j += 1;
			}
			if j < chars.len() && j > i + 1 {
				let token = &chars[i + 1..j];
				if token.len() <= 64 && token.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+'))
				{
					i = j + 1;
					continue;
				}
			}
		}
		output.push(chars[i]);
		i += 1;
	}
	output
}

fn strip_unicode_emoji_chars(input: &str) -> String {
	input.chars().filter(|&ch| !is_emoji_char(ch)).collect()
}

fn is_emoji_char(ch: char) -> bool {
	let code = u32::from(ch);
	matches!(
		code,
		0x1F1E6..=0x1F1FF
			| 0x1F300..=0x1FAFF
			| 0x2300..=0x23FF
			| 0x2600..=0x26FF
			| 0x2700..=0x27BF
			| 0xFE0E..=0xFE0F
			| 0x200D
			| 0x20E3
			| 0xE0020..=0xE007F
	)
}

fn normalize_spaces(input: &str) -> String {
	input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
	use super::strip_display_name_emojis;
	use crate::config::DisplayNameEmojiMode;

	#[test]
	fn strips_unicode_only() {
		let output = strip_display_name_emojis("Alice 😄 :party_parrot:", DisplayNameEmojiMode::UnicodeOnly);
		assert_eq!(output, "Alice :party_parrot:");
	}

	#[test]
	fn strips_instance_only() {
		let output = strip_display_name_emojis("Alice 😄 :party_parrot:", DisplayNameEmojiMode::InstanceOnly);
		assert_eq!(output, "Alice 😄");
	}

	#[test]
	fn strips_all() {
		let output = strip_display_name_emojis("Alice 😄 :party_parrot:", DisplayNameEmojiMode::All);
		assert_eq!(output, "Alice");
	}

	#[test]
	fn keeps_colon_text_that_is_not_shortcode() {
		let output = strip_display_name_emojis("Time 10:30 and A:B", DisplayNameEmojiMode::InstanceOnly);
		assert_eq!(output, "Time 10:30 and A:B");
	}
}
