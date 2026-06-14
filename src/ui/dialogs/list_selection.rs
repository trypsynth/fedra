use wxdragon::prelude::*;

use crate::mastodon::List;

pub fn show_list_selection_dialog(frame: &Frame, lists: &[List], title: &str, button_label: &str) -> Option<List> {
	let dialog = Dialog::builder(frame, title).with_size(300, 400).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label("Select a list").build();
	let list_box = ListBox::builder(&panel).build();
	for list in lists {
		list_box.append(&list.title);
	}
	if !lists.is_empty() {
		list_box.set_selection(0, true);
	}
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label(button_label).build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&list_box, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();
	let result = dialog.show_modal();
	if result == ID_OK {
		let selection = list_box.get_selection().map(|s| s as usize);
		if let Some(index) = selection {
			return lists.get(index).cloned();
		}
	}
	None
}
