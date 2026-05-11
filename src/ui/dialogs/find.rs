use wxdragon::prelude::*;

use super::KEY_RETURN;

pub fn show_find_dialog(parent: &dyn WxWidget) -> Option<String> {
	let dialog = Dialog::builder(parent, "Find text in timeline").with_size(350, 150).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let label = StaticText::builder(&panel).with_label("Search for").build();
	let input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let find_button = Button::builder(&panel).with_id(ID_OK).with_label("Find").build();
	find_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&find_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	input.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.get_key_code() == Some(KEY_RETURN) && !key_event.shift_down() && !key_event.control_down() {
				dialog.end_modal(ID_OK);
				event.skip(false);
			} else {
				event.skip(true);
			}
		} else {
			event.skip(true);
		}
	});
	dialog.centre();
	input.set_focus();
	if dialog.show_modal() == ID_OK {
		let text = input.get_value();
		if !text.is_empty() {
			return Some(text);
		}
	}
	None
}
