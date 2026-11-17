use wxdragon::prelude::*;

use crate::html::Link;

/// One thing a post offers to open, and its address when it's a link, for the Copy button.
pub struct OpenChoice {
	pub label: String,
	pub url: Option<String>,
}

pub fn show_link_selection_dialog(frame: &Frame, links: &[Link]) -> Option<String> {
	let choices: Vec<OpenChoice> =
		links.iter().map(|link| OpenChoice { label: link.url.clone(), url: Some(link.url.clone()) }).collect();
	let index = show_open_dialog(frame, "Select Link", "Links found in post:", &choices)?;
	links.get(index).map(|link| link.url.clone())
}

/// Lets the user pick one of `choices`, giving back its index.
pub fn show_open_dialog(frame: &Frame, title: &str, label: &str, choices: &[OpenChoice]) -> Option<usize> {
	let dialog = Dialog::builder(frame, title).with_size(500, 300).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label(label).build();
	let list = ListBox::builder(&panel).build();
	for choice in choices {
		list.append(&choice.label);
	}
	if !choices.is_empty() {
		list.set_selection(0, true);
	}
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let open_button = Button::builder(&panel).with_id(ID_OK).with_label("Open").build();
	open_button.set_default();
	let copy_button = Button::builder(&panel).with_label("Copy").build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	button_sizer.add(&open_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&copy_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let urls: Vec<Option<String>> = choices.iter().map(|choice| choice.url.clone()).collect();
	let selected_url = move || list.get_selection().and_then(|i| urls.get(i as usize).cloned().flatten());
	let copy = {
		let selected_url = selected_url.clone();
		move || {
			if let Some(url) = selected_url() {
				let clipboard = Clipboard::get();
				let _ = clipboard.set_text(&url);
				let _ = clipboard.flush();
			}
		}
	};
	copy_button.enable(selected_url().is_some());
	let copy_from_button = copy.clone();
	copy_button.on_click(move |_| copy_from_button());
	list.on_selection_changed(move |_| copy_button.enable(selected_url().is_some()));
	list.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event
			&& key_event.control_down()
			&& key_event.get_key_code() == Some(i32::from(b'C'))
		{
			copy();
		} else {
			event.skip(true);
		}
	});
	dialog.centre();
	let result = dialog.show_modal();
	let selection = list.get_selection();
	dialog.destroy();
	if result != ID_OK {
		return None;
	}
	selection.map(|i| i as usize).filter(|&i| i < choices.len())
}
