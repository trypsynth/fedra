use wx_utils::{add_ok_cancel_footer, build_ok_cancel_buttons_on};
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
	let (ok_button, cancel_button) = build_ok_cancel_buttons_on(&panel, &dialog, button_label);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&list_box, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	add_ok_cancel_footer(main_sizer, ok_button, cancel_button);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.centre();
	let result = dialog.show_modal();
	let selection = list_box.get_selection().map(|s| s as usize);
	dialog.destroy();
	if result != ID_OK {
		return None;
	}
	selection.and_then(|index| lists.get(index).cloned())
}
