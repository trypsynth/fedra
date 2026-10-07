use std::{cell::Cell, rc::Rc};

use wxdragon::prelude::*;

use crate::{mastodon::NotificationRequest, timeline::TimelineTextOptions};

pub enum MessageRequestAction {
	Accept(usize),
	Dismiss(usize),
	Close,
}

/// Shows the waiting message requests until the user accepts or dismisses one, or closes the
/// dialog. `selected` restores the selection after an earlier action.
pub fn prompt_message_requests(
	frame: &Frame,
	requests: &[NotificationRequest],
	options: &TimelineTextOptions,
	selected: Option<usize>,
) -> MessageRequestAction {
	let dialog = Dialog::builder(frame, "Message Requests").with_size(500, 400).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let label = if requests.is_empty() { "No message &requests" } else { "Message &requests:" };
	let requests_label = StaticText::builder(&panel).with_label(label).build();
	let requests_list = ListBox::builder(&panel).build();
	for request in requests {
		let name = request.account.timeline_display_name(options);
		let count = request.notifications_count;
		let messages = if count == 1 { "1 message".to_string() } else { format!("{count} messages") };
		let latest = request.last_status.as_ref().map(|status| format!(": {}", status.simple_display()));
		requests_list.append(&format!("{name} (@{}), {messages}{}", request.account.acct, latest.unwrap_or_default()));
	}
	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let accept_button = Button::builder(&panel).with_label("&Accept").build();
	let dismiss_button = Button::builder(&panel).with_label("&Dismiss").build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("&Close").build();
	buttons_sizer.add(&accept_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&dismiss_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&requests_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&requests_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_escape_id(ID_CANCEL);
	if !requests.is_empty() {
		let index = selected.unwrap_or(0).min(requests.len() - 1);
		requests_list.set_selection(u32::try_from(index).unwrap_or(0), true);
	}
	accept_button.enable(!requests.is_empty());
	dismiss_button.enable(!requests.is_empty());
	let action: Rc<Cell<Option<(bool, usize)>>> = Rc::new(Cell::new(None));
	for (button, accept) in [(accept_button, true), (dismiss_button, false)] {
		let action = action.clone();
		button.on_click(move |_| {
			if let Some(index) = requests_list.get_selection() {
				action.set(Some((accept, index as usize)));
				dialog.end_modal(ID_OK);
			}
		});
	}
	dialog.centre();
	requests_list.set_focus();
	dialog.show_modal();
	dialog.destroy();
	match action.get() {
		Some((true, index)) => MessageRequestAction::Accept(index),
		Some((false, index)) => MessageRequestAction::Dismiss(index),
		None => MessageRequestAction::Close,
	}
}
