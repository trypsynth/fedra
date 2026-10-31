use wx_utils::{add_ok_cancel_footer, bind_enter_confirms, build_ok_cancel_buttons_on};
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
	let policy_label = StaticText::builder(&panel).with_label("Show replies from:").build();
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
	let (ok_button, cancel_button) = build_ok_cancel_buttons_on(&panel, &dialog, "Save");
	main_sizer.add(&title_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&title_input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&policy_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&policy_choice, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&exclusive_check, 0, SizerFlag::Expand | SizerFlag::All, 8);
	add_ok_cancel_footer(main_sizer, ok_button, cancel_button);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	bind_enter_confirms(&dialog, title_input);
	dialog.centre();
	title_input.set_focus();
	let result = dialog.show_modal();
	let title = title_input.get_value().trim().to_string();
	let policy_sel = policy_choice.get_selection().unwrap_or(0) as usize;
	let exclusive = exclusive_check.get_value();
	dialog.destroy();
	if result != ID_OK || title.is_empty() {
		return None;
	}
	let policy = policy_values.get(policy_sel).unwrap_or(&"followed").to_string();
	Some((title, policy, exclusive))
}
