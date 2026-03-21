use std::{cell::RefCell, rc::Rc};

use wxdragon::prelude::*;

use crate::mastodon::{Filter, FilterAction, FilterContext};

use super::common::KEY_RETURN;

pub enum ManageFiltersResult {
	Add,
	Edit(String),
	Delete(String),
	None,
}

pub fn prompt_manage_filters(frame: &Frame, filters: &[Filter]) -> ManageFiltersResult {
	let dialog = Dialog::builder(frame, "Filter Manager").with_size(400, 350).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let filters_label = StaticText::builder(&panel).with_label("Filters:").build();
	let filters_list = ListBox::builder(&panel).build();
	for filter in filters {
		let action_label = match &filter.action {
			FilterAction::Warn => "Hide with warning",
			FilterAction::Hide => "Hide completely",
			FilterAction::Blur => "Hide media with warning",
			FilterAction::Other(s) => s,
		};
		let label = format!("{} ({})", filter.title, action_label);
		filters_list.append(&label);
	}
	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let add_button = Button::builder(&panel).with_label("Add...").build();
	let edit_button = Button::builder(&panel).with_label("Edit...").build();
	let remove_button = Button::builder(&panel).with_label("Delete").build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	close_button.set_default();
	buttons_sizer.add(&add_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&edit_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&remove_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&filters_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&filters_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_CANCEL);
	dialog.set_escape_id(ID_CANCEL);
	edit_button.enable(false);
	remove_button.enable(false);
	let result = Rc::new(RefCell::new(ManageFiltersResult::None));
	let filters_list_select = filters_list;
	let edit_button_select = edit_button;
	let remove_button_select = remove_button;
	filters_list.on_selection_changed(move |_| {
		let has_selection = filters_list_select.get_selection().is_some();
		edit_button_select.enable(has_selection);
		remove_button_select.enable(has_selection);
	});
	let result_add = result.clone();
	add_button.on_click(move |_| {
		*result_add.borrow_mut() = ManageFiltersResult::Add;
		dialog.end_modal(ID_OK);
	});
	let result_edit = result.clone();
	let filters_list_edit = filters_list;
	let filter_ids: Vec<String> = filters.iter().map(|f| f.id.clone()).collect();
	let filter_ids_edit = filter_ids.clone();
	edit_button.on_click(move |_| {
		if let Some(sel) = filters_list_edit.get_selection() {
			let idx = sel as usize;
			if idx < filter_ids_edit.len() {
				*result_edit.borrow_mut() = ManageFiltersResult::Edit(filter_ids_edit[idx].clone());
				dialog.end_modal(ID_OK);
			}
		}
	});
	let result_remove = result.clone();
	let filters_list_remove = filters_list;
	let filter_ids_remove = filter_ids;
	let parent = dialog;
	remove_button.on_click(move |_| {
		if let Some(sel) = filters_list_remove.get_selection() {
			let idx = sel as usize;
			if idx < filter_ids_remove.len() {
				let warning =
					MessageDialog::builder(&parent, "Are you sure you want to delete this filter?", "Delete Filter")
						.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
						.build();
				if warning.show_modal() == ID_YES {
					*result_remove.borrow_mut() = ManageFiltersResult::Delete(filter_ids_remove[idx].clone());
					dialog.end_modal(ID_OK);
				}
			}
		}
	});
	dialog.centre();
	dialog.show_modal();
	result.borrow().clone()
}

pub struct FilterDialogResult {
	pub title: String,
	pub contexts: Vec<FilterContext>,
	pub action: FilterAction,
	pub keywords: Vec<(String, String, bool, bool)>,
	pub expires_in: Option<u32>,
}

#[derive(Clone)]
struct KeywordEntry {
	id: String,
	keyword: String,
	whole_word: bool,
	destroyed: bool,
}

fn prompt_keyword_edit(
	parent: &dyn WxWidget,
	initial_keyword: Option<&str>,
	initial_whole_word: bool,
) -> Option<(String, bool)> {
	let title = if initial_keyword.is_some() { "Edit Keyword" } else { "Add Keyword" };
	let dialog = Dialog::builder(parent, title).with_size(400, 200).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let keyword_label = StaticText::builder(&panel).with_label("Keyword:").build();
	let keyword_input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
	if let Some(k) = initial_keyword {
		keyword_input.set_value(k);
	}
	let whole_word_check = CheckBox::builder(&panel).with_label("Whole word").build();
	whole_word_check.set_value(initial_whole_word);

	main_sizer.add(&keyword_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&keyword_input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&whole_word_check, 0, SizerFlag::Expand | SizerFlag::All, 8);

	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);

	let input_enter = keyword_input;
	let dialog_enter = dialog;
	input_enter.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.get_key_code() == Some(KEY_RETURN) && !key_event.shift_down() && !key_event.control_down() {
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
	keyword_input.set_focus();
	if dialog.show_modal() != ID_OK {
		return None;
	}

	let text = keyword_input.get_value();
	let trimmed = text.trim();
	if trimmed.is_empty() {
		return None;
	}
	Some((trimmed.to_string(), whole_word_check.get_value()))
}

pub fn prompt_filter_edit(frame: &Frame, existing: Option<&Filter>) -> Option<FilterDialogResult> {
	let title = if existing.is_some() { "Edit Filter" } else { "Add Filter" };
	let dialog = Dialog::builder(frame, title).with_size(500, 600).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let title_label = StaticText::builder(&panel).with_label("Filter Title:").build();
	let title_text = TextCtrl::builder(&panel).build();
	if let Some(f) = existing {
		title_text.set_value(&f.title);
	}
	main_sizer.add(&title_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&title_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	let context_label = StaticText::builder(&panel).with_label("Contexts:").build();
	main_sizer.add(&context_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	let context_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let mut context_checks = Vec::new();
	let all_contexts = [
		FilterContext::Home,
		FilterContext::Notifications,
		FilterContext::Public,
		FilterContext::Thread,
		FilterContext::Account,
	];
	for context in &all_contexts {
		let cb = CheckBox::builder(&panel).with_label(&format!("{context}")).build();
		if let Some(f) = existing
			&& f.context.contains(context)
		{
			cb.set_value(true);
		}
		context_sizer.add(&cb, 1, SizerFlag::Expand, 4);
		context_checks.push((cb, context.clone()));
	}
	main_sizer.add_sizer(&context_sizer, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	let action_label = StaticText::builder(&panel).with_label("Action:").build();
	let mut action_choices =
		vec!["Hide with warning".to_string(), "Hide completely".to_string(), "Hide media with warning".to_string()];
	let mut custom_action = None;
	if let Some(f) = existing {
		match &f.action {
			FilterAction::Warn | FilterAction::Hide | FilterAction::Blur => {}
			FilterAction::Other(s) => {
				action_choices.push(format!("Custom: {s}"));
				custom_action = Some(s.clone());
			}
		}
	}
	let action_choice = Choice::builder(&panel).with_choices(action_choices).build();
	if let Some(f) = existing {
		match f.action {
			FilterAction::Warn => action_choice.set_selection(0),
			FilterAction::Hide => action_choice.set_selection(1),
			FilterAction::Blur => action_choice.set_selection(2),
			FilterAction::Other(_) => action_choice.set_selection(3),
		}
	} else {
		action_choice.set_selection(0);
	}
	let action_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	action_sizer.add(&action_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	action_sizer.add(&action_choice, 1, SizerFlag::Expand, 0);
	main_sizer.add_sizer(&action_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	let expiry_label = StaticText::builder(&panel).with_label("Expires in:").build();
	let expiry_choices = vec![
		"Never".to_string(),
		"30 minutes".to_string(),
		"1 hour".to_string(),
		"6 hours".to_string(),
		"12 hours".to_string(),
		"1 day".to_string(),
		"1 week".to_string(),
	];
	let expiry_choice = Choice::builder(&panel).with_choices(expiry_choices).build();
	expiry_choice.set_selection(0);
	let expiry_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	expiry_sizer.add(&expiry_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	expiry_sizer.add(&expiry_choice, 1, SizerFlag::Expand, 0);
	main_sizer.add_sizer(&expiry_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	let keywords_label = StaticText::builder(&panel).with_label("Keywords:").build();
	let keywords_list = ListBox::builder(&panel).build();
	let add_keyword_button = Button::builder(&panel).with_label("Add Keyword...").build();
	let edit_keyword_button = Button::builder(&panel).with_label("Edit Keyword...").build();
	let remove_keyword_button = Button::builder(&panel).with_label("Remove Selected").build();

	main_sizer.add(&keywords_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&keywords_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);

	let keyword_actions_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	keyword_actions_sizer.add(&add_keyword_button, 0, SizerFlag::Right, 8);
	keyword_actions_sizer.add(&edit_keyword_button, 0, SizerFlag::Right, 8);
	keyword_actions_sizer.add_stretch_spacer(1);
	keyword_actions_sizer.add(&remove_keyword_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&keyword_actions_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let save_button = Button::builder(&panel).with_id(ID_OK).with_label("Save").build();
	save_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&save_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let initial_keywords: Vec<KeywordEntry> = existing.map_or_else(Vec::new, |f| {
		f.keywords
			.iter()
			.map(|k| KeywordEntry {
				id: k.id.clone(),
				keyword: k.keyword.clone(),
				whole_word: k.whole_word,
				destroyed: false,
			})
			.collect()
	});

	let keywords = Rc::new(RefCell::new(initial_keywords));
	let refresh_keywords = {
		let keywords = keywords.clone();
		let list = keywords_list;
		move || {
			let previous_sel = list.get_selection();
			list.clear();
			for k in keywords.borrow().iter() {
				if !k.destroyed {
					let label = if k.whole_word { format!("{} (Whole word)", k.keyword) } else { k.keyword.clone() };
					list.append(&label);
				}
			}
			if let Some(sel) = previous_sel
				&& (sel as usize) < list.get_count() as usize
			{
				list.set_selection(sel, true);
			}
		}
	};
	refresh_keywords();
	let list_select = keywords_list;
	let edit_btn_select = edit_keyword_button;
	let remove_btn_select = remove_keyword_button;
	edit_btn_select.enable(false);
	remove_btn_select.enable(false);

	list_select.on_selection_changed(move |_| {
		let has_sel = list_select.get_selection().is_some();
		edit_btn_select.enable(has_sel);
		remove_btn_select.enable(has_sel);
	});
	let keywords_add = keywords.clone();
	let refresh_add = refresh_keywords.clone();
	let dialog_add = dialog;
	add_keyword_button.on_click(move |_| {
		if let Some((keyword, whole_word)) = prompt_keyword_edit(&dialog_add, None, false) {
			keywords_add.borrow_mut().push(KeywordEntry { id: String::new(), keyword, whole_word, destroyed: false });
			refresh_add();
		}
	});
	let keywords_edit = keywords.clone();
	let list_edit = keywords_list;
	let refresh_edit = refresh_keywords.clone();
	let dialog_edit = dialog;
	edit_keyword_button.on_click(move |_| {
		if let Some(sel) = list_edit.get_selection() {
			let idx = sel as usize;
			let mut visual_count = 0;
			let (current_kw, current_whole_word) = {
				let k = keywords_edit.borrow();
				let mut result = None;
				for entry in k.iter() {
					if !entry.destroyed {
						if visual_count == idx {
							result = Some((entry.keyword.clone(), entry.whole_word));
							break;
						}
						visual_count += 1;
					}
				}
				match result {
					Some(r) => r,
					None => return,
				}
			};

			if let Some((new_kw, new_ww)) = prompt_keyword_edit(&dialog_edit, Some(&current_kw), current_whole_word) {
				let mut k_mut = keywords_edit.borrow_mut();
				visual_count = 0;
				for entry in k_mut.iter_mut() {
					if !entry.destroyed {
						if visual_count == idx {
							entry.keyword = new_kw;
							entry.whole_word = new_ww;
							break;
						}
						visual_count += 1;
					}
				}
				drop(k_mut);
				refresh_edit();
			}
		}
	});
	let keywords_remove = keywords.clone();
	let list_remove = keywords_list;
	let refresh_remove = refresh_keywords;
	let edit_btn_remove = edit_keyword_button;
	let remove_btn_remove = remove_keyword_button;

	remove_keyword_button.on_click(move |_| {
		if let Some(sel) = list_remove.get_selection() {
			let idx = sel as usize;
			let mut visual_count = 0;
			let mut found = false;
			{
				let mut k = keywords_remove.borrow_mut();
				for entry in k.iter_mut() {
					if !entry.destroyed {
						if visual_count == idx {
							entry.destroyed = true;
							found = true;
							break;
						}
						visual_count += 1;
					}
				}
			}
			if found {
				refresh_remove();
				edit_btn_remove.enable(false);
				remove_btn_remove.enable(false);
			}
		}
	});

	dialog.centre();
	title_text.set_focus();
	if dialog.show_modal() != ID_OK {
		return None;
	}

	let title = title_text.get_value().trim().to_string();
	if title.is_empty() {
		return None;
	}

	let mut contexts = Vec::new();
	for (cb, ctx) in context_checks {
		if cb.get_value() {
			contexts.push(ctx);
		}
	}
	if contexts.is_empty() {
		contexts.push(FilterContext::Home);
	}

	let action = match action_choice.get_selection() {
		Some(1) => FilterAction::Hide,
		Some(2) => FilterAction::Blur,
		Some(3) => custom_action.map_or(FilterAction::Warn, FilterAction::Other),
		_ => FilterAction::Warn,
	};

	let expires_in = match expiry_choice.get_selection() {
		Some(1) => Some(30 * 60),
		Some(2) => Some(60 * 60),
		Some(3) => Some(6 * 60 * 60),
		Some(4) => Some(12 * 60 * 60),
		Some(5) => Some(24 * 60 * 60),
		Some(6) => Some(7 * 24 * 60 * 60),
		_ => None,
	};

	let final_keywords: Vec<(String, String, bool, bool)> =
		keywords.borrow().iter().map(|k| (k.id.clone(), k.keyword.clone(), k.whole_word, k.destroyed)).collect();

	Some(FilterDialogResult { title, contexts, action, keywords: final_keywords, expires_in })
}

#[derive(Clone)]
