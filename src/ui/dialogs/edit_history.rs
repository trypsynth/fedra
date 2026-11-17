//! Every version of an edited post: a list of versions, newest first, and the selected one's text.

use wxdragon::prelude::*;

use crate::mastodon::StatusVersion;

/// `versions` as the edit history gives them, oldest first.
pub fn show_edit_history_dialog(parent: &Frame, versions: &[StatusVersion]) {
	let dialog = Dialog::builder(parent, "Edit History").with_size(600, 500).build();
	let panel = Panel::builder(&dialog).build();
	let versions_label = StaticText::builder(&panel).with_label("&Versions:").build();
	let list = ListBox::builder(&panel).build();
	let newest_first: Vec<&StatusVersion> = versions.iter().rev().collect();
	for (i, version) in newest_first.iter().enumerate() {
		list.append(&version_label(i, newest_first.len(), version));
	}
	let text_label = StaticText::builder(&panel).with_label("&Text:").build();
	let text = TextCtrl::builder(&panel).with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly).build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("&Close").build();
	let shown: Vec<String> = newest_first.iter().map(|version| version.text()).collect();
	let show = move |index: usize| {
		if let Some(value) = shown.get(index) {
			text.set_value(value);
		}
	};
	show(0);
	list.set_selection(0, true);
	list.on_selection_changed(move |_| {
		if let Some(index) = list.get_selection() {
			show(index as usize);
		}
	});
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	main_sizer.add(&versions_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&list, 1, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&text_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&text, 2, SizerFlag::Expand | SizerFlag::All, 8);
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();
	list.set_focus();
	dialog.show_modal();
	dialog.destroy();
}

/// "Current version, 2 hours ago", "Edit 2, 5 hours ago" or "Original, 3 days ago", for the version at `index` of
/// `count`, newest first.
fn version_label(index: usize, count: usize, version: &StatusVersion) -> String {
	let which = if index == 0 {
		"Current version".to_string()
	} else if index + 1 == count {
		"Original".to_string()
	} else {
		format!("Edit {}", count - 1 - index)
	};
	let when = version.relative_time();
	if when.is_empty() { which } else { format!("{which}, {when}") }
}

#[cfg(test)]
mod tests {
	use super::version_label;
	use crate::mastodon::StatusVersion;

	fn version() -> StatusVersion {
		serde_json::from_value(serde_json::json!({ "content": "<p>hi</p>", "created_at": "" })).unwrap()
	}

	#[test]
	fn labels_count_edits_up_from_the_original() {
		let labels: Vec<String> = (0..4).map(|i| version_label(i, 4, &version())).collect();
		assert_eq!(labels, ["Current version", "Edit 2", "Edit 1", "Original"]);
	}
}
