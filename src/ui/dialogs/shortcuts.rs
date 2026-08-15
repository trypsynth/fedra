use std::{cell::RefCell, rc::Rc};

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Role, Tree, TreeUpdate};
use accesskit_windows::SubclassingAdapter;
use windows::Win32::Foundation::HWND;
use wxdragon::prelude::*;

use crate::config::{ActionId, EnterBehaviorPreset, KeyChord, ModeShortcuts, ShortcutsConfig};

const LR_ROOT_ID: NodeId = NodeId(1);
const LR_ANNOUNCEMENT_ID: NodeId = NodeId(2);

struct DetectedKeyActivationHandler;

impl ActivationHandler for DetectedKeyActivationHandler {
	fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
		let mut root = Node::new(Role::Window);
		root.set_children(vec![LR_ANNOUNCEMENT_ID]);

		let mut ann_node = Node::new(Role::Label);
		ann_node.set_value("");
		ann_node.set_live(accesskit::Live::Polite);

		Some(TreeUpdate {
			nodes: vec![(LR_ANNOUNCEMENT_ID, ann_node), (LR_ROOT_ID, root)],
			tree: Some(Tree::new(LR_ROOT_ID)),
			focus: LR_ROOT_ID,
			tree_id: accesskit::TreeId::ROOT,
		})
	}
}

struct DetectedKeyActionHandler;

impl ActionHandler for DetectedKeyActionHandler {
	fn do_action(&mut self, _request: ActionRequest) {}
}

/// A screen-reader "polite" live region that announces the currently detected
/// key chord as the user types it, since a plain `StaticText::set_label` update
/// is silent to assistive tech.
#[derive(Clone)]
struct DetectedKeyLiveRegion {
	adapter: Rc<RefCell<SubclassingAdapter>>,
	last_announcement: Rc<RefCell<Option<String>>>,
}

impl DetectedKeyLiveRegion {
	fn new(dialog: &Dialog) -> Self {
		let hwnd = HWND(dialog.get_handle() as *mut _);
		let adapter = SubclassingAdapter::new(hwnd, DetectedKeyActivationHandler, DetectedKeyActionHandler);
		Self { adapter: Rc::new(RefCell::new(adapter)), last_announcement: Rc::new(RefCell::new(None)) }
	}

	fn announce(&self, text: &str) {
		let mut new_text = text.to_string();
		let mut last = self.last_announcement.borrow_mut();
		if last.as_deref() == Some(new_text.as_str()) {
			// Force screen readers to re-announce identical text by nudging it.
			new_text.push('\u{00A0}');
		}
		*last = Some(new_text.clone());

		let mut node = Node::new(Role::Label);
		node.set_value(new_text);
		node.set_live(accesskit::Live::Polite);

		let mut root = Node::new(Role::Window);
		root.set_children(vec![LR_ANNOUNCEMENT_ID]);

		let update = TreeUpdate {
			nodes: vec![(LR_ANNOUNCEMENT_ID, node), (LR_ROOT_ID, root)],
			tree: None,
			focus: LR_ROOT_ID,
			tree_id: accesskit::TreeId::ROOT,
		};
		let mut adapter = self.adapter.borrow_mut();
		if let Some(events) = adapter.update_if_active(|| update) {
			events.raise();
		}
	}
}

pub fn prompt_for_shortcuts(parent: &dyn WxWidget, initial: &ShortcutsConfig) -> Option<ShortcutsConfig> {
	let config_state = Rc::new(RefCell::new(initial.clone()));

	let dialog = Dialog::builder(parent, "Customize Keyboard Shortcuts").with_size(550, 560).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let notebook = Notebook::builder(&panel).build();

	let quick_tab = build_tab(&notebook, config_state.clone(), true, &dialog);
	notebook.add_page(&quick_tab, "Quick Keys Mode", true, None);

	let normal_tab = build_tab(&notebook, config_state.clone(), false, &dialog);
	notebook.add_page(&normal_tab, "Normal Mode", false, None);

	main_sizer.add(&notebook, 1, SizerFlag::Expand | SizerFlag::All, 8);

	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();

	if dialog.show_modal() == ID_OK {
		let result = config_state.borrow().clone();
		Some(result)
	} else {
		None
	}
}

fn build_tab(
	notebook: &Notebook,
	config_state: Rc<RefCell<ShortcutsConfig>>,
	is_quick: bool,
	parent_dialog: &Dialog,
) -> Panel {
	let panel = Panel::builder(notebook).with_style(PanelStyle::TabTraversal).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();

	let preset_label = StaticText::builder(&panel).with_label("&Enter key behavior:").build();
	let preset_choices: Vec<String> = EnterBehaviorPreset::all().iter().map(|p| p.display_name().to_string()).collect();
	let preset_choice =
		ComboBox::builder(&panel).with_choices(preset_choices).with_style(ComboBoxStyle::ReadOnly).build();

	let initial_preset = config_state.borrow().active_mode(is_quick).enter_behavior_preset(is_quick);
	let preset_index = match initial_preset {
		EnterBehaviorPreset::EnterLinksAltThread => 0,
		EnterBehaviorPreset::EnterThreadAltLinks => 1,
		EnterBehaviorPreset::Custom => 2,
	};
	preset_choice.set_selection(preset_index);

	let preset_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	preset_sizer.add(&preset_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	preset_sizer.add(&preset_choice, 1, SizerFlag::Expand, 0);
	sizer.add_sizer(&preset_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	let list_label = StaticText::builder(&panel).with_label("&Shortcuts:").build();
	sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);

	let list_box = ListBox::builder(&panel).build();
	let actions = ActionId::all();
	for &action in actions {
		let item_text = format_list_item(&config_state.borrow(), is_quick, action);
		list_box.append(&item_text);
	}
	if !actions.is_empty() {
		list_box.set_selection(0, true);
	}
	sizer.add(&list_box, 1, SizerFlag::Expand | SizerFlag::All, 8);

	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let set_button = Button::builder(&panel).with_label("&Set Shortcut...").build();
	let clear_button = Button::builder(&panel).with_label("&Clear Shortcut").build();
	let reset_button = Button::builder(&panel).with_label("&Reset to Default").build();
	let reset_all_button = Button::builder(&panel).with_label("Reset &All to Defaults").build();

	buttons_sizer.add(&set_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&clear_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&reset_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&reset_all_button, 0, SizerFlag::Right, 8);
	sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(sizer, true);

	let refresh_ui = {
		let config_state = config_state.clone();
		let list_box = list_box;
		let preset_choice = preset_choice;
		move || {
			let current_sel = list_box.get_selection().unwrap_or(0);
			list_box.freeze();
			list_box.clear();
			for &action in ActionId::all() {
				let item_text = format_list_item(&config_state.borrow(), is_quick, action);
				list_box.append(&item_text);
			}
			if !ActionId::all().is_empty() {
				let count = list_box.get_count();
				let sel = if current_sel < count { current_sel } else { 0 };
				list_box.set_selection(sel, true);
			}
			list_box.thaw();

			let current_preset = config_state.borrow().active_mode(is_quick).enter_behavior_preset(is_quick);
			let idx = match current_preset {
				EnterBehaviorPreset::EnterLinksAltThread => 0,
				EnterBehaviorPreset::EnterThreadAltLinks => 1,
				EnterBehaviorPreset::Custom => 2,
			};
			preset_choice.set_selection(idx);
		}
	};

	let refresh_on_preset = refresh_ui.clone();
	let config_on_preset = config_state.clone();
	preset_choice.on_selection_changed(move |event| {
		if let Some(idx) = event.get_selection() {
			let preset = match idx {
				0 => EnterBehaviorPreset::EnterLinksAltThread,
				1 => EnterBehaviorPreset::EnterThreadAltLinks,
				_ => EnterBehaviorPreset::Custom,
			};
			config_on_preset.borrow_mut().active_mode_mut(is_quick).set_enter_behavior(is_quick, preset);
			refresh_on_preset();
		}
	});

	let trigger_set_shortcut = {
		let config_state = config_state.clone();
		let list_box = list_box;
		let parent = *parent_dialog;
		let refresh_ui = refresh_ui.clone();
		move || {
			let Some(sel) = list_box.get_selection() else { return };
			let actions = ActionId::all();
			let Ok(idx) = usize::try_from(sel) else { return };
			let Some(&action) = actions.get(idx) else { return };

			let current_chord = config_state.borrow().get_chord(is_quick, action);
			if let Some(result) = prompt_for_key_chord(&parent, action, current_chord.as_ref()) {
				if let Some(new_chord) = &result {
					let conflict =
						find_conflict(&config_state.borrow().active_mode(is_quick), is_quick, action, new_chord);
					if let Some(other_action) = conflict {
						let msg = format!(
							"'{}' is already assigned to '{}'. Reassign it to '{}'?",
							new_chord.to_shortcut_string(),
							other_action.display_name(),
							action.display_name()
						);
						let warn = MessageDialog::builder(&parent, &msg, "Shortcut Conflict")
							.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
							.build();
						if warn.show_modal() != ID_YES {
							return;
						}
						config_state.borrow_mut().active_mode_mut(is_quick).set_chord(other_action, None);
					}
				}
				config_state.borrow_mut().active_mode_mut(is_quick).set_chord(action, result);
				refresh_ui();
			}
		}
	};

	let set_on_click = trigger_set_shortcut.clone();
	set_button.on_click(move |_| {
		set_on_click();
	});

	let set_on_dclick = trigger_set_shortcut;
	list_box.on_item_double_clicked(move |_| {
		set_on_dclick();
	});

	let config_on_clear = config_state.clone();
	let list_on_clear = list_box;
	let refresh_on_clear = refresh_ui.clone();
	clear_button.on_click(move |_| {
		let Some(sel) = list_on_clear.get_selection() else { return };
		let actions = ActionId::all();
		let Ok(idx) = usize::try_from(sel) else { return };
		let Some(&action) = actions.get(idx) else { return };
		config_on_clear.borrow_mut().active_mode_mut(is_quick).set_chord(action, None);
		refresh_on_clear();
	});

	let config_on_reset = config_state.clone();
	let list_on_reset = list_box;
	let refresh_on_reset = refresh_ui.clone();
	reset_button.on_click(move |_| {
		let Some(sel) = list_on_reset.get_selection() else { return };
		let actions = ActionId::all();
		let Ok(idx) = usize::try_from(sel) else { return };
		let Some(&action) = actions.get(idx) else { return };
		config_on_reset.borrow_mut().active_mode_mut(is_quick).reset_action(action);
		refresh_on_reset();
	});

	let config_on_reset_all = config_state;
	let refresh_on_reset_all = refresh_ui;
	let parent_reset_all = *parent_dialog;
	reset_all_button.on_click(move |_| {
		let warn = MessageDialog::builder(
			&parent_reset_all,
			"Reset all shortcuts in this mode to their default values?",
			"Reset Shortcuts",
		)
		.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconQuestion)
		.build();
		if warn.show_modal() == ID_YES {
			config_on_reset_all.borrow_mut().active_mode_mut(is_quick).reset_all();
			refresh_on_reset_all();
		}
	});

	panel
}

fn format_list_item(config: &ShortcutsConfig, is_quick: bool, action: ActionId) -> String {
	let chord_str = config.active_mode(is_quick).get_display_str(action, is_quick);
	format!("{}: {}", action.display_name(), chord_str)
}

fn find_conflict(
	mode: &ModeShortcuts,
	is_quick: bool,
	target_action: ActionId,
	target_chord: &KeyChord,
) -> Option<ActionId> {
	for &action in ActionId::all() {
		if action == target_action {
			continue;
		}
		if let Some(chord) = mode.get_chord(action, is_quick) {
			if chord == *target_chord {
				return Some(action);
			}
		}
	}
	None
}

const ID_CLEAR_SHORTCUT: i32 = 10099;

fn prompt_for_key_chord(
	parent: &dyn WxWidget,
	action: ActionId,
	initial: Option<&KeyChord>,
) -> Option<Option<KeyChord>> {
	let title = format!("Set Shortcut for {}", action.display_name());
	let dialog = Dialog::builder(parent, &title).with_size(400, 260).build();
	let live_region = DetectedKeyLiveRegion::new(&dialog);
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let info_text = format!("Configure shortcut for {}:", action.display_name());
	let info_label = StaticText::builder(&panel).with_label(&info_text).build();
	main_sizer.add(&info_label, 0, SizerFlag::Expand | SizerFlag::All, 8);

	let ctrl_cb = CheckBox::builder(&panel).with_label("&Ctrl").build();
	let alt_cb = CheckBox::builder(&panel).with_label("&Alt").build();
	let shift_cb = CheckBox::builder(&panel).with_label("&Shift").build();

	if let Some(chord) = initial {
		ctrl_cb.set_value(chord.ctrl);
		alt_cb.set_value(chord.alt);
		shift_cb.set_value(chord.shift);
	}

	let mod_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	mod_sizer.add(&ctrl_cb, 0, SizerFlag::Right, 12);
	mod_sizer.add(&alt_cb, 0, SizerFlag::Right, 12);
	mod_sizer.add(&shift_cb, 0, SizerFlag::Right, 12);
	main_sizer.add_sizer(&mod_sizer, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 8);

	let key_label = StaticText::builder(&panel).with_label("&Key:").build();
	let key_choices = vec![
		"Enter".to_string(),
		"Space".to_string(),
		"Tab".to_string(),
		"Backspace".to_string(),
		"Delete".to_string(),
		"Escape".to_string(),
		"F1".to_string(),
		"F2".to_string(),
		"F3".to_string(),
		"F4".to_string(),
		"F5".to_string(),
		"F6".to_string(),
		"F7".to_string(),
		"F8".to_string(),
		"F9".to_string(),
		"F10".to_string(),
		"F11".to_string(),
		"F12".to_string(),
		"Left".to_string(),
		"Right".to_string(),
		"Up".to_string(),
		"Down".to_string(),
		"Home".to_string(),
		"End".to_string(),
		"PageUp".to_string(),
		"PageDown".to_string(),
		"A".to_string(),
		"B".to_string(),
		"C".to_string(),
		"D".to_string(),
		"E".to_string(),
		"F".to_string(),
		"G".to_string(),
		"H".to_string(),
		"I".to_string(),
		"J".to_string(),
		"K".to_string(),
		"L".to_string(),
		"M".to_string(),
		"N".to_string(),
		"O".to_string(),
		"P".to_string(),
		"Q".to_string(),
		"R".to_string(),
		"S".to_string(),
		"T".to_string(),
		"U".to_string(),
		"V".to_string(),
		"W".to_string(),
		"X".to_string(),
		"Y".to_string(),
		"Z".to_string(),
		"0".to_string(),
		"1".to_string(),
		"2".to_string(),
		"3".to_string(),
		"4".to_string(),
		"5".to_string(),
		"6".to_string(),
		"7".to_string(),
		"8".to_string(),
		"9".to_string(),
		",".to_string(),
		".".to_string(),
		"/".to_string(),
		"[".to_string(),
		"]".to_string(),
		"\\".to_string(),
		"-".to_string(),
		"=".to_string(),
		";".to_string(),
		"'".to_string(),
		"`".to_string(),
	];
	let key_combo = ComboBox::builder(&panel).with_choices(key_choices).with_style(ComboBoxStyle::ReadOnly).build();
	if let Some(chord) = initial {
		key_combo.set_value(&chord.key);
	}

	let key_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	key_sizer.add(&key_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	key_sizer.add(&key_combo, 1, SizerFlag::Expand, 0);
	main_sizer.add_sizer(&key_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	let hint_label = StaticText::builder(&panel)
		.with_label("Tip: click in the key field and press the key combination you want.")
		.build();
	main_sizer.add(&hint_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);

	let preview_label = StaticText::builder(&panel).with_label("Detected: (none)").build();
	main_sizer.add(&preview_label, 0, SizerFlag::Expand | SizerFlag::All, 8);

	let current_shortcut_text = {
		let ctrl_cb = ctrl_cb;
		let alt_cb = alt_cb;
		let shift_cb = shift_cb;
		let key_combo = key_combo;
		move || {
			let key = key_combo.get_value();
			let trimmed = key.trim().to_string();
			if trimmed.is_empty() {
				None
			} else {
				let chord = KeyChord::new(ctrl_cb.get_value(), alt_cb.get_value(), shift_cb.get_value(), &trimmed);
				Some(chord.to_shortcut_string())
			}
		}
	};

	let refresh_preview_label = {
		let current_shortcut_text = current_shortcut_text.clone();
		let preview_label = preview_label;
		move || {
			let text =
				current_shortcut_text().map_or_else(|| "Detected: (none)".to_string(), |s| format!("Detected: {s}"));
			preview_label.set_label(&text);
		}
	};
	refresh_preview_label();

	// Only the live region should announce user-driven changes, not the dialog's
	// initial state, so it starts out separate from `refresh_preview_label`.
	let update_preview = {
		let current_shortcut_text = current_shortcut_text;
		let refresh_preview_label = refresh_preview_label.clone();
		let live_region = live_region.clone();
		move || {
			refresh_preview_label();
			let announce_text =
				current_shortcut_text().map_or_else(|| "No key detected".to_string(), |s| format!("Detected: {s}"));
			live_region.announce(&announce_text);
		}
	};

	let ctrl_cb_cap = ctrl_cb;
	let alt_cb_cap = alt_cb;
	let shift_cb_cap = shift_cb;
	let key_combo_cap = key_combo;
	let update_preview_key = update_preview.clone();
	key_combo.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if let Some(k) = key_event.get_key_code() {
				if k != 9 {
					if let Some(parsed) = KeyChord::from_key_code(
						k,
						key_event.control_down(),
						key_event.alt_down(),
						key_event.shift_down(),
					) {
						ctrl_cb_cap.set_value(parsed.ctrl);
						alt_cb_cap.set_value(parsed.alt);
						shift_cb_cap.set_value(parsed.shift);
						key_combo_cap.set_value(&parsed.key);
						update_preview_key();
						event.skip(false);
						return;
					}
				}
			}
		}
		event.skip(true);
	});

	let update_preview_combo = update_preview.clone();
	key_combo.on_selection_changed(move |_| {
		update_preview_combo();
	});

	let update_preview_ctrl = update_preview.clone();
	ctrl_cb.on_toggled(move |_| update_preview_ctrl());
	let update_preview_alt = update_preview.clone();
	alt_cb.on_toggled(move |_| update_preview_alt());
	let update_preview_shift = update_preview.clone();
	shift_cb.on_toggled(move |_| update_preview_shift());

	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
	ok_button.set_default();
	let clear_button = Button::builder(&panel).with_id(ID_CLEAR_SHORTCUT).with_label("&Clear").build();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();

	button_sizer.add(&clear_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);

	let dialog_clear = dialog;
	clear_button.on_click(move |_| {
		dialog_clear.end_modal(ID_CLEAR_SHORTCUT);
	});

	dialog.centre();
	key_combo.set_focus();

	let res = dialog.show_modal();
	if res == ID_CLEAR_SHORTCUT {
		Some(None)
	} else if res == ID_OK {
		let key_text = key_combo.get_value();
		let trimmed = key_text.trim();
		if trimmed.is_empty() {
			Some(None)
		} else {
			let chord = KeyChord::new(ctrl_cb.get_value(), alt_cb.get_value(), shift_cb.get_value(), trimmed);
			Some(Some(chord))
		}
	} else {
		None
	}
}
