//! The timeline list: a quiet-list, so posts arriving or changing in the background don't make the
//! screen reader read the selected post out again.

use std::rc::Rc;

use accesskit::NodeId;
#[cfg(windows)]
use live_region::Priority;
use quiet_list::{QuietList, Select, Step};
use wxdragon::{ffi::wxd_Window_t, prelude::*};

#[derive(Clone)]
pub struct TimelineList {
	list: Rc<QuietList<NodeId>>,
	/// The live region announcements go through while Fedra is in the foreground.
	#[cfg(windows)]
	announcer: StaticText,
}

impl WxWidget for TimelineList {
	fn handle_ptr(&self) -> *mut wxd_Window_t {
		self.list.handle_ptr()
	}
}

impl TimelineList {
	pub fn new(parent: &impl WxWidget) -> Self {
		let list = Rc::new(QuietList::new(parent, ""));
		#[cfg(windows)]
		let announcer = {
			let label = StaticText::builder(parent).with_label("").with_size(Size::new(0, 0)).build();
			label.show(false);
			live_region::set_live_region(&label);
			label
		};
		Self {
			list,
			#[cfg(windows)]
			announcer,
		}
	}

	/// Moves the selection as the arrow, Home, End, Page Up or Page Down key `key` would, firing
	/// `on_selection_changed` if it moved. Works whether or not the list has focus, which is what
	/// lets global shortcuts move through a timeline while the window is hidden.
	///
	/// Returns whether `key` is one of those keys, not whether the selection moved.
	pub fn navigate(&self, key: i32) -> bool {
		let Some(step) = Step::from_key(key) else {
			return false;
		};
		if self.list.is_empty() {
			return false;
		}
		self.list.step(step);
		true
	}

	/// The text of the selected entry, as the list shows it.
	pub fn selected_text(&self) -> Option<String> {
		self.list.selected_label()
	}

	pub fn has_focus(&self) -> bool {
		self.list.has_focus()
	}

	pub fn get_selection(&self) -> Option<i32> {
		self.list.selected_index().map(|index| i32::try_from(index).unwrap_or(i32::MAX))
	}

	pub fn get_count(&self) -> i32 {
		i32::try_from(self.list.len()).unwrap_or(i32::MAX)
	}

	pub fn clear(&self) {
		self.list.update_labelled(Vec::new(), Select::Start);
	}

	/// Shows `menu` under the selected entry, or where the mouse is if it was asked for with the
	/// mouse.
	pub fn popup_menu(&self, menu: &mut Menu) {
		self.list.popup_menu(menu);
	}

	/// Runs `callback` when an entry is right-clicked, or the Applications key or Shift+F10 is
	/// pressed in the list.
	pub fn on_context_menu<F>(&self, callback: F)
	where
		F: Fn() + 'static,
	{
		self.list.on_context_menu(callback);
	}

	pub fn on_selection_changed<F>(&self, callback: F)
	where
		F: Fn() + 'static,
	{
		self.list.on_selection_changed(callback);
	}

	/// Shows `callback` every key pressed in the list before the list acts on it. It returns
	/// whether it handled the key, in which case the list leaves the key alone.
	pub fn on_key_down<F>(&self, callback: F)
	where
		F: Fn(&WindowEventData) -> bool + 'static,
	{
		self.list.on_key_down(callback);
	}

	/// Replaces the rows, selecting `selected_id`, or the first entry if it's `None` or gone.
	pub fn update_entries(&self, entries: Vec<(NodeId, String)>, selected_id: Option<NodeId>) {
		self.list.update_labelled(entries, selected_id.map_or(Select::Start, Select::Key));
	}

	/// Moves the selection without firing `on_selection_changed`, so the caller's own model is not
	/// updated. Callers drive this from that model, not the other way round.
	pub fn set_selection(&self, selected_id: Option<NodeId>) {
		self.list.select(selected_id.as_ref());
	}

	/// Speaks `text` through the window's live region, or through prism when no Fedra window is in
	/// the foreground, since a live region in a hidden or background window goes unheard.
	#[cfg(windows)]
	pub fn announce(&self, text: &str) {
		if crate::speech::fedra_in_foreground() {
			live_region::announce_with_priority(self.announcer, text, Priority::Medium);
		} else {
			crate::speech::speak(text);
		}
	}

	#[cfg(not(windows))]
	#[allow(clippy::unused_self, reason = "a method, like the Windows version")]
	pub fn announce(&self, text: &str) {
		crate::speech::speak(text);
	}
}
