use wxdragon::prelude::*;

use crate::{html, mastodon::Account, network::ProfileUpdate};

pub fn show_profile_edit_dialog(frame: &Frame, current: &Account) -> Option<ProfileUpdate> {
	let dialog = Dialog::builder(frame, "Edit Profile").with_size(600, 600).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let scroll_win = ScrolledWindow::builder(&panel).build();
	scroll_win.set_scroll_rate(0, 10);
	let content_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let name_label = StaticText::builder(&scroll_win).with_label("Display Name").build();
	let name_text = TextCtrl::builder(&scroll_win).with_value(current.display_name_or_username()).build();
	name_text.set_name("Display Name");
	content_sizer.add(&name_label, 0, SizerFlag::All, 5);
	content_sizer.add(&name_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 5);
	let note_label = StaticText::builder(&scroll_win).with_label("Bio").build();
	let note_text = TextCtrl::builder(&scroll_win)
		.with_value(&html::strip_html(&current.note))
		.with_style(TextCtrlStyle::MultiLine)
		.with_size(Size::new(-1, 100))
		.build();
	note_text.set_name("Bio");
	content_sizer.add(&note_label, 0, SizerFlag::All, 5);
	content_sizer.add(&note_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 5);
	let images_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let avatar_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let avatar_label = StaticText::builder(&scroll_win).with_label("Avatar:").build();
	let avatar_path = TextCtrl::builder(&scroll_win).with_style(TextCtrlStyle::ReadOnly).build();
	avatar_path.set_name("Avatar Path");
	let avatar_btn = Button::builder(&scroll_win).with_label("Change Avatar...").build();
	avatar_sizer.add(&avatar_label, 0, SizerFlag::All, 5);
	avatar_sizer.add(&avatar_path, 0, SizerFlag::Expand | SizerFlag::All, 5);
	avatar_sizer.add(&avatar_btn, 0, SizerFlag::All, 5);
	let header_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let header_label = StaticText::builder(&scroll_win).with_label("Header:").build();
	let header_path = TextCtrl::builder(&scroll_win).with_style(TextCtrlStyle::ReadOnly).build();
	header_path.set_name("Header Path");
	let header_btn = Button::builder(&scroll_win).with_label("Change Header...").build();
	header_sizer.add(&header_label, 0, SizerFlag::All, 5);
	header_sizer.add(&header_path, 0, SizerFlag::Expand | SizerFlag::All, 5);
	header_sizer.add(&header_btn, 0, SizerFlag::All, 5);
	images_sizer.add_sizer(&avatar_sizer, 1, SizerFlag::Expand, 0);
	images_sizer.add_sizer(&header_sizer, 1, SizerFlag::Expand, 0);
	content_sizer.add_sizer(&images_sizer, 0, SizerFlag::Expand, 0);
	let locked_cb = CheckBox::builder(&scroll_win).with_label("Require &follow approval").build();
	locked_cb.set_value(current.locked);
	content_sizer.add(&locked_cb, 0, SizerFlag::All, 5);
	let bot_cb = CheckBox::builder(&scroll_win).with_label("&Bot account").build();
	bot_cb.set_value(current.bot);
	content_sizer.add(&bot_cb, 0, SizerFlag::All, 5);
	let discoverable_cb = CheckBox::builder(&scroll_win).with_label("&Discoverable in directory").build();
	discoverable_cb.set_value(current.discoverable.unwrap_or(false));
	content_sizer.add(&discoverable_cb, 0, SizerFlag::All, 5);
	let mut field_controls = Vec::new();
	for i in 0..4 {
		let row_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let (name_val, val_val) = if i < current.fields.len() {
			(current.fields[i].name.clone(), html::strip_html(&current.fields[i].value))
		} else {
			(String::new(), String::new())
		};
		let title_lbl = format!("Field {} label", i + 1);
		let content_lbl = format!("Field {} content", i + 1);
		let field_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let title_text = StaticText::builder(&scroll_win).with_label(&title_lbl).build();
		let name_ctrl = TextCtrl::builder(&scroll_win).with_value(&name_val).build();
		name_ctrl.set_name(&title_lbl);
		field_sizer.add(&title_text, 0, SizerFlag::All, 2);
		field_sizer.add(&name_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 2);
		let content_sizer_inner = BoxSizer::builder(Orientation::Vertical).build();
		let content_text = StaticText::builder(&scroll_win).with_label(&content_lbl).build();
		let val_ctrl = TextCtrl::builder(&scroll_win).with_value(&val_val).build();
		val_ctrl.set_name(&content_lbl);
		content_sizer_inner.add(&content_text, 0, SizerFlag::All, 2);
		content_sizer_inner.add(&val_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 2);
		row_sizer.add_sizer(&field_sizer, 1, SizerFlag::Expand, 0);
		row_sizer.add_sizer(&content_sizer_inner, 2, SizerFlag::Expand, 0);
		content_sizer.add_sizer(&row_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
		field_controls.push((name_ctrl, val_ctrl));
	}
	let (privacy_choice_opt, sensitive_cb_opt, lang_text_opt) =
		current.source.as_ref().map_or((None, None, None), |source| {
			let privacy_sizer = BoxSizer::builder(Orientation::Horizontal).build();
			let privacy_label = StaticText::builder(&scroll_win).with_label("Default post visibility").build();
			let privacy_choices: Vec<String> =
				vec!["Public".to_string(), "Unlisted".to_string(), "Followers only".to_string()];
			let privacy_choice = Choice::builder(&scroll_win).with_choices(privacy_choices).build();
			let sel = match source.privacy.as_deref() {
				Some("unlisted") => 1,
				Some("private") => 2,
				_ => 0,
			};
			privacy_choice.set_selection(sel);
			privacy_sizer.add(&privacy_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 5);
			privacy_sizer.add(&privacy_choice, 1, SizerFlag::Expand, 0);
			content_sizer.add_sizer(&privacy_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
			let sensitive_cb = CheckBox::builder(&scroll_win).with_label("&Mark media as sensitive by default").build();
			sensitive_cb.set_value(source.sensitive.unwrap_or(false));
			content_sizer.add(&sensitive_cb, 0, SizerFlag::All, 5);
			let lang_sizer = BoxSizer::builder(Orientation::Horizontal).build();
			let lang_label = StaticText::builder(&scroll_win).with_label("Language (ISO code):").build();
			let lang_text = TextCtrl::builder(&scroll_win).with_value(source.language.as_deref().unwrap_or("")).build();
			lang_sizer.add(&lang_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 5);
			lang_sizer.add(&lang_text, 1, SizerFlag::Expand, 0);
			content_sizer.add_sizer(&lang_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
			(Some(privacy_choice), Some(sensitive_cb), Some(lang_text))
		});
	scroll_win.set_sizer(content_sizer, true);
	main_sizer.add(&scroll_win, 1, SizerFlag::Expand, 0);
	let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_btn = Button::builder(&panel).with_id(ID_OK).with_label("Save Changes").build();
	ok_btn.set_default();
	let cancel_btn = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	btn_sizer.add_stretch_spacer(1);
	btn_sizer.add(&ok_btn, 0, SizerFlag::Right, 8);
	btn_sizer.add(&cancel_btn, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 10);
	panel.set_sizer(main_sizer, true);
	let dlg_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dlg_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dlg_sizer, true);
	dialog.centre();
	let avatar_path_ref = avatar_path;
	let panel_ref = panel;
	avatar_btn.on_click(move |_| {
		let dlg = FileDialog::builder(&panel_ref)
			.with_message("Select Avatar")
			.with_wildcard("Image files|*.png;*.jpg;*.jpeg;*.gif")
			.with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
			.build();
		if dlg.show_modal() == ID_OK
			&& let Some(path) = dlg.get_path()
		{
			avatar_path_ref.set_value(&path);
		}
	});
	let header_path_ref = header_path;
	let panel_ref = panel;
	header_btn.on_click(move |_| {
		let dlg = FileDialog::builder(&panel_ref)
			.with_message("Select Header")
			.with_wildcard("Image files|*.png;*.jpg;*.jpeg;*.gif")
			.with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
			.build();
		if dlg.show_modal() == ID_OK
			&& let Some(path) = dlg.get_path()
		{
			header_path_ref.set_value(&path);
		}
	});
	if dialog.show_modal() != ID_OK {
		return None;
	}
	let display_name = name_text.get_value();
	let note = note_text.get_value();
	let avatar = avatar_path.get_value();
	let header = header_path.get_value();
	let locked = locked_cb.get_value();
	let bot = bot_cb.get_value();
	let discoverable = discoverable_cb.get_value();
	let mut fields_attributes = Vec::new();
	for (name_ctrl, val_ctrl) in &field_controls {
		let name = name_ctrl.get_value();
		let val = val_ctrl.get_value();
		// Always send all fields to preserve indices (0..3) so the server knows which to update/clear
		fields_attributes.push((name, val));
	}
	let source = if let (Some(privacy_choice), Some(sensitive_cb), Some(lang_text)) =
		(privacy_choice_opt, sensitive_cb_opt, lang_text_opt)
	{
		let privacy = match privacy_choice.get_selection() {
			Some(1) => "unlisted",
			Some(2) => "private",
			_ => "public",
		}
		.to_string();
		Some(crate::mastodon::Source {
			privacy: Some(privacy),
			sensitive: Some(sensitive_cb.get_value()),
			language: Some(lang_text.get_value()),
		})
	} else {
		None
	};
	Some(ProfileUpdate {
		display_name: Some(display_name),
		note: Some(note),
		avatar: if avatar.is_empty() { None } else { Some(avatar) },
		header: if header.is_empty() { None } else { Some(header) },
		locked: Some(locked),
		bot: Some(bot),
		discoverable: Some(discoverable),
		fields_attributes: Some(fields_attributes),
		source,
	})
}
