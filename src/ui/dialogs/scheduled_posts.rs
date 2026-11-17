//! The user's scheduled posts: a list, soonest first, the selected one's text, and buttons to reschedule or delete it.

use std::{cell::RefCell, rc::Rc};

use wxdragon::prelude::*;

use super::compose::prompt_for_schedule;
use crate::mastodon::{ScheduledStatus, friendly_time_local};

/// What to do to the scheduled post with this id, and where it sat in the list.
pub enum ScheduledPostAction {
	Reschedule { id: String, scheduled_at: String },
	Delete { id: String },
}

/// Shows `posts`, already sorted, with the one at `selected` highlighted. Gives back what the user chose to do and the
/// index it was done to, or `None` when the dialog was closed.
pub fn show_scheduled_posts_dialog(
	parent: &Frame,
	posts: &[ScheduledStatus],
	selected: usize,
) -> Option<(ScheduledPostAction, usize)> {
	const ID_RESCHEDULE: i32 = 27_001;
	const ID_DELETE: i32 = 27_002;
	let dialog = Dialog::builder(parent, &format!("Scheduled Posts ({})", posts.len())).with_size(600, 500).build();
	let panel = Panel::builder(&dialog).build();
	let list_label = StaticText::builder(&panel).with_label("&Scheduled posts:").build();
	let list = ListBox::builder(&panel).build();
	for post in posts {
		list.append(&format!("{}: {}", friendly_time_local(&post.scheduled_at), post.preview()));
	}
	let text_label = StaticText::builder(&panel).with_label("&Text:").build();
	let text = TextCtrl::builder(&panel).with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly).build();
	let reschedule_button = Button::builder(&panel).with_id(ID_RESCHEDULE).with_label("&Reschedule...").build();
	let delete_button = Button::builder(&panel).with_id(ID_DELETE).with_label("&Delete").build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("&Close").build();
	let details: Vec<String> = posts.iter().map(ScheduledStatus::details).collect();
	let show = move |index: usize| {
		if let Some(value) = details.get(index) {
			text.set_value(value);
		}
	};
	let selected = selected.min(posts.len().saturating_sub(1));
	show(selected);
	list.set_selection(u32::try_from(selected).unwrap_or(0), true);
	list.on_selection_changed(move |_| {
		if let Some(index) = list.get_selection() {
			show(index as usize);
		}
	});
	let chosen: Rc<RefCell<Option<(ScheduledPostAction, usize)>>> = Rc::new(RefCell::new(None));
	let ids: Rc<Vec<(String, String)>> =
		Rc::new(posts.iter().map(|p| (p.id.clone(), p.scheduled_at.clone())).collect());
	let (chosen_reschedule, ids_reschedule) = (chosen.clone(), ids.clone());
	reschedule_button.on_click(move |_| {
		let Some(index) = list.get_selection().map(|i| i as usize) else { return };
		let Some((id, current)) = ids_reschedule.get(index) else { return };
		if let Some(Some(scheduled_at)) = prompt_for_schedule(&dialog, Some(current), false) {
			*chosen_reschedule.borrow_mut() =
				Some((ScheduledPostAction::Reschedule { id: id.clone(), scheduled_at }, index));
			dialog.end_modal(ID_RESCHEDULE);
		}
	});
	let chosen_delete = chosen.clone();
	delete_button.on_click(move |_| {
		let Some(index) = list.get_selection().map(|i| i as usize) else { return };
		let Some((id, _)) = ids.get(index) else { return };
		if wx_utils::confirm(&dialog, "Are you sure you want to delete this scheduled post?", "Delete Scheduled Post") {
			*chosen_delete.borrow_mut() = Some((ScheduledPostAction::Delete { id: id.clone() }, index));
			dialog.end_modal(ID_DELETE);
		}
	});
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&list, 1, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&text_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&text, 2, SizerFlag::Expand | SizerFlag::All, 8);
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	button_sizer.add(&reschedule_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&delete_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();
	list.set_focus();
	dialog.show_modal();
	dialog.destroy();
	chosen.borrow_mut().take()
}
