use std::fmt::Write;

use wxdragon::{
	event::{WebViewEventData, WebViewEvents},
	prelude::*,
	widgets::WebView,
};

use crate::{
	ID_BOOST, ID_FAVORITE, ID_REPLY, UiCommand,
	mastodon::{MediaAttachment, Status},
};

fn escape_html(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for ch in text.chars() {
		match ch {
			'&' => out.push_str("&amp;"),
			'<' => out.push_str("&lt;"),
			'>' => out.push_str("&gt;"),
			'"' => out.push_str("&quot;"),
			'\'' => out.push_str("&#39;"),
			_ => out.push(ch),
		}
	}
	out
}

/// Lists each attachment with its alt text under a heading, so screen reader
/// users can jump to it and read the descriptions like the rest of the post.
fn media_html(media: &[MediaAttachment]) -> String {
	if media.is_empty() {
		return String::new();
	}
	let mut html = String::from("<h3>Media</h3><ul>");
	for (index, attachment) in media.iter().enumerate() {
		let kind = match attachment.kind.as_str() {
			"image" => "Image",
			"gifv" => "GIF",
			"video" => "Video",
			"audio" => "Audio",
			_ => "Attachment",
		};
		let description = attachment
			.description
			.as_deref()
			.map(str::trim)
			.filter(|d| !d.is_empty())
			.map_or_else(|| "No description".to_string(), escape_html);
		let _ = write!(html, "<li>{kind} {}: {description}</li>", index + 1);
	}
	html.push_str("</ul>");
	html
}

fn strip_quote_html(html: &str) -> String {
	if let Some(start) = html.find("<span class=\"quote-inline\">")
		&& let Some(end) = html[start..].find("</span>")
	{
		let mut cleaned = String::new();
		cleaned.push_str(&html[..start]);
		cleaned.push_str(&html[start + end + 7..]);
		return cleaned;
	}
	let start_idx = usize::from(html.starts_with("<p>")) * 3;
	if let Some(re_idx) = html[start_idx..].find("RE: ")
		&& re_idx < 30
	{
		let actual_re_idx = start_idx + re_idx;
		let mut end_cut = actual_re_idx;
		if let Some(a_idx) = html[actual_re_idx..].find("</a>") {
			end_cut = actual_re_idx + a_idx + 4;
		} else if let Some(br_idx) = html[actual_re_idx..].find("<br") {
			end_cut = actual_re_idx + br_idx;
		} else if let Some(p_idx) = html[actual_re_idx..].find("</p>") {
			end_cut = actual_re_idx + p_idx;
		}
		let remainder = &html[end_cut..];
		let mut rest = remainder.trim_start();
		if let Some(stripped) = rest.strip_prefix("</span>") {
			rest = stripped.trim_start();
		}
		if let Some(stripped) =
			rest.strip_prefix("<br>").or_else(|| rest.strip_prefix("<br />")).or_else(|| rest.strip_prefix("<br/>"))
		{
			rest = stripped.trim_start();
		} else if let Some(stripped) = rest.strip_prefix("</p>") {
			rest = stripped.trim_start();
		}
		let prefix = &html[..actual_re_idx];
		if prefix != "<p>" {
			return format!("{prefix}{rest}");
		}
		if rest.starts_with("<p>") {
			return rest.to_string();
		}
		return format!("<p>{rest}");
	}
	html.to_string()
}

pub fn show_post_view_dialog(parent: &Frame, status: &Status) -> Option<UiCommand> {
	let title = format!("Post by {}", status.account.display_name_or_username());
	let dialog = Dialog::builder(parent, &title).with_size(600, 500).build();
	let panel = Panel::builder(&dialog).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	let web_view = WebView::builder(&panel).build();
	web_view.add_script_message_handler("wx");
	let dialog_close_msg = dialog;
	web_view.on_script_message_received(move |event: WebViewEventData| {
		if let Some(msg) = event.get_string() {
			if msg == "close_dialog" {
				dialog_close_msg.end_modal(ID_CANCEL);
			} else if let Some(url) = msg.strip_prefix("open_link:") {
				let _ = wxdragon::utils::launch_default_browser(url, wxdragon::utils::BrowserLaunchFlags::Default);
			}
		}
	});
	let mut content = if status.spoiler_text.is_empty() {
		status.content.clone()
	} else {
		format!("<p><strong>Content Warning: {}</strong></p><hr>{}", status.spoiler_text, status.content)
	};
	content.push_str(&media_html(&status.media_attachments));
	if let Some(quote) = status.quote.as_ref().and_then(|q| q.quoted_status.as_ref()) {
		content = strip_quote_html(&content);
		let quote_author = quote.account.display_name_or_username();
		let quote_acct = &quote.account.acct;
		let quote_content = if quote.spoiler_text.is_empty() {
			quote.content.clone()
		} else {
			format!("<p><strong>Content Warning: {}</strong></p><hr>{}", quote.spoiler_text, quote.content)
		};
		let quote_media = media_html(&quote.media_attachments);
		content = format!(
			"{content}
			<blockquote style=\"border-left: 4px solid #ccc; margin-left: 0; padding-left: 10px; color: #555;\">
				<strong>{quote_author} <small>({quote_acct})</small></strong>
				{quote_content}
				{quote_media}
			</blockquote>"
		);
	}
	let html = format!(
		"<html>
		<head>
			<title>{}</title>
			<style>
				body {{ font-family: sans-serif; padding: 10px; }}
				img {{ max-width: 100%; height: auto; }}
				video {{ max-width: 100%; height: auto; }}
				p, span, div, pre, code {{ white-space: pre-wrap; }}
			</style>
		</head>
		<body>
			<h2>{} <small>({})</small></h2>
			{}
		</body>
		</html>",
		title,
		status.account.display_name_or_username(),
		status.account.acct,
		content
	);
	web_view.set_page(&html, "");
	let web_view_for_load = web_view;
	web_view.on_loaded(move |_| {
		web_view_for_load.run_script(
			"function addEvent(elem, event, handler) { \
				if (elem.addEventListener) { \
					elem.addEventListener(event, handler, false); \
				} else if (elem.attachEvent) { \
					elem.attachEvent('on' + event, handler); \
				} \
			} \
			addEvent(document, 'keydown', function(event) { \
				if (event.key === 'Escape' || event.keyCode === 27) { \
					window.wx.postMessage('close_dialog'); \
				} \
			}); \
			addEvent(document, 'click', function(event) { \
				event = event || window.event; \
				var target = event.target || event.srcElement; \
				while (target && target.tagName !== 'A') { target = target.parentNode; } \
				if (target && target.tagName === 'A' && target.href) { \
					if (event.preventDefault) event.preventDefault(); \
					else event.returnValue = false; \
					window.wx.postMessage('open_link:' + target.href); \
				} \
			});",
		);
	});
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let reply_btn = Button::builder(&panel).with_id(ID_REPLY).with_label("Reply").build();
	let boost_btn = Button::builder(&panel)
		.with_id(ID_BOOST)
		.with_label(if status.reblogged { "Unboost" } else { "Boost" })
		.build();
	let fav_btn = Button::builder(&panel)
		.with_id(ID_FAVORITE)
		.with_label(if status.favourited { "Unfavorite" } else { "Favorite" })
		.build();
	let close_btn = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	close_btn.set_default();
	button_sizer.add(&reply_btn, 0, SizerFlag::All, 5);
	button_sizer.add(&boost_btn, 0, SizerFlag::All, 5);
	button_sizer.add(&fav_btn, 0, SizerFlag::All, 5);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&close_btn, 0, SizerFlag::All, 5);
	sizer.add(&web_view, 1, SizerFlag::Expand | SizerFlag::All, 5);
	sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
	panel.set_sizer(sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();
	let dialog_reply = dialog;
	reply_btn.on_click(move |_| {
		dialog_reply.end_modal(ID_REPLY);
	});
	let dialog_boost = dialog;
	boost_btn.on_click(move |_| {
		dialog_boost.end_modal(ID_BOOST);
	});
	let dialog_fav = dialog;
	fav_btn.on_click(move |_| {
		dialog_fav.end_modal(ID_FAVORITE);
	});
	let dialog_close = dialog;
	close_btn.on_click(move |_| {
		dialog_close.end_modal(ID_CANCEL);
	});
	let result = dialog.show_modal();
	dialog.destroy();
	match result {
		ID_REPLY => Some(UiCommand::Reply { reply_all: true }),
		ID_BOOST => Some(UiCommand::Boost),
		ID_FAVORITE => Some(UiCommand::Favorite),
		_ => None,
	}
}
