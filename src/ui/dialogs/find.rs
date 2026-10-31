use wx_utils::{add_ok_cancel_footer, bind_enter_confirms, build_ok_cancel_buttons_on};
use wxdragon::prelude::*;

pub fn show_find_dialog(parent: &dyn WxWidget) -> Option<String> {
	let dialog = Dialog::builder(parent, "Find text in timeline").with_size(350, 150).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let label = StaticText::builder(&panel).with_label("Search for").build();
	let input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
	let (find_button, cancel_button) = build_ok_cancel_buttons_on(&panel, &dialog, "Find");
	main_sizer.add(&label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	add_ok_cancel_footer(main_sizer, find_button, cancel_button);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	bind_enter_confirms(&dialog, input);
	dialog.centre();
	input.set_focus();
	let result = dialog.show_modal();
	let text = input.get_value();
	dialog.destroy();
	(result == ID_OK && !text.is_empty()).then_some(text)
}
