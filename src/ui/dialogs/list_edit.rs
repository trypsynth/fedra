use wxdragon::prelude::*;

pub fn show_list_edit_dialog(
	parent: &dyn WxWidget,
	initial_title: Option<&str>,
	initial_policy: Option<&str>,
	initial_exclusive: bool,
) -> Option<(String, String, bool)> {
	let title_str = if initial_title.is_some() { "Edit List" } else { "Create List" };
	let dialog = Dialog::builder(parent, title_str).with_size(400, 250).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let title_label = StaticText::builder(&panel).with_label("List Title:").build();
	let title_input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
	if let Some(t) = initial_title {
		title_input.set_value(t);
	}
	let policy_label = StaticText::builder(&panel).with_label("Replies Policy:").build();
	let policy_choices = vec![
		"Show replies to followed users".to_string(),
		"Show replies to list members".to_string(),
		"No replies".to_string(),
	];
	let policy_values = ["followed", "list", "none"];
	let policy_choice = Choice::builder(&panel).with_choices(policy_choices).build();
	let policy_idx = initial_policy.and_then(|p| policy_values.iter().position(|&v| v == p)).unwrap_or(0);
	policy_choice.set_selection(u32::try_from(policy_idx).unwrap_or(0));
	let exclusive_check = CheckBox::builder(&panel).with_label("&Hide these posts from Home timeline").build();
	exclusive_check.set_value(initial_exclusive);
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("Save").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&title_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&title_input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&policy_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&policy_choice, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&exclusive_check, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let title_enter = title_input;
	let dialog_enter = dialog;
	title_enter.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.get_key_code() == Some(13) && !key_event.shift_down() && !key_event.control_down() {
				dialog_enter.end_modal(ID_OK);
				event.skip(false);
			} else {
				event.skip(true);
			}
		} else {
			event.skip(true);
		}
	});
	dialog.centre();
	title_input.set_focus();
	if dialog.show_modal() != ID_OK {
		return None;
	}
	let title = title_input.get_value().trim().to_string();
	if title.is_empty() {
		return None;
	}
	let policy_sel = policy_choice.get_selection().unwrap_or(0) as usize;
	let policy = policy_values.get(policy_sel).unwrap_or(&"followed").to_string();
	Some((title, policy, exclusive_check.get_value()))
}
