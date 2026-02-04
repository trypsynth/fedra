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
			output.push_str(text);
		}
		scraper::node::Node::Element(element) => {
			let name = element.name();
			if name == "br" {
				push_newline(output);
				return;
			}
			let is_block = matches!(
				name,
				"p" | "div"
					| "li" | "ul" | "ol"
					| "blockquote" | "pre"
					| "section" | "article"
					| "header" | "footer"
					| "h1" | "h2" | "h3"
					| "h4" | "h5" | "h6"
			);
			if is_block {
				push_newline(output);
			}
			for child in node.children() {
				append_text(child, output);
			}
			if is_block {
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

fn normalize_text(input: &str) -> String {
	let mut output = String::with_capacity(input.len());
	let mut last_was_space = false;
	let mut last_was_newline = false;
	for ch in input.chars() {
		match ch {
			'\r' => {}
			'\n' => {
				if !output.ends_with('\n') {
					output.push('\n');
				}
				last_was_space = false;
				last_was_newline = true;
			}
			c if c.is_whitespace() => {
				if !last_was_space && !last_was_newline {
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
		cleaned.push_str(line.trim());
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

#[derive(Debug, Clone)]
pub struct Link {
	pub url: String,
}

pub fn extract_links(html: &str) -> Vec<Link> {
	let fragment = scraper::Html::parse_fragment(html);
	let mut links = Vec::new();
	let selector = scraper::Selector::parse("a").unwrap();
	for element in fragment.select(&selector) {
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
