use wxdragon::prelude::*;

pub fn show_update_dialog(parent: &dyn WxWidget, new_version: &str, changelog: &str) -> bool {
	let padding = 10;
	let dialog_title = format!("Update to {new_version}");
	let dialog = Dialog::builder(parent, &dialog_title).build();
	let panel = Panel::builder(&dialog).build();
	let message =
		StaticText::builder(&panel).with_label("A new version of Fedra is available. Here's what's new:").build();
	let changelog_ctrl = TextCtrl::builder(&panel)
		.with_value(changelog)
		.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly | TextCtrlStyle::Rich2)
		.with_size(Size::new(500, 300))
		.build();
	let yes_button = Button::builder(&panel).with_id(ID_OK).with_label("Yes").build();
	let no_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("No").build();
	dialog.set_escape_id(ID_CANCEL);
	dialog.set_affirmative_id(ID_OK);
	let content_sizer = BoxSizer::builder(Orientation::Vertical).build();
	content_sizer.add(&message, 0, SizerFlag::All, padding);
	content_sizer.add(
		&changelog_ctrl,
		1,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		padding,
	);
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&yes_button, 0, SizerFlag::Right, padding);
	button_sizer.add(&no_button, 0, SizerFlag::Right, padding);
	content_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 0);
	panel.set_sizer(content_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer_and_fit(dialog_sizer, true);
	dialog.centre();
	dialog.raise();
	changelog_ctrl.set_focus();
	dialog.show_modal() == ID_OK
}
