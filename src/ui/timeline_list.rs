use std::{cell::RefCell, rc::Rc, time::Instant};

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Role, Tree, TreeUpdate};
use accesskit_windows::SubclassingAdapter;
use windows::Win32::Foundation::HWND;
use wxdragon::{prelude::*, widgets::panel::PanelStyle};

struct TimelineActionHandler {
	cb_ptr: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl ActionHandler for TimelineActionHandler {
	fn do_action(&mut self, request: ActionRequest) {
		let ptr = self.cb_ptr.load(std::sync::atomic::Ordering::Relaxed);
		if ptr != 0 {
			let cb = unsafe { &*(ptr as *const Box<dyn Fn(ActionRequest)>) };
			cb(request);
		}
	}
}

pub const ROOT_ID: NodeId = NodeId(1);
struct ListState {
	entries: Vec<(NodeId, String)>,
	selected_index: Option<usize>,
	search_buffer: String,
	last_search_time: Option<Instant>,
}

struct TimelineActivationHandler {
	state: Rc<RefCell<ListState>>,
}

impl ActivationHandler for TimelineActivationHandler {
	fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
		let state = self.state.borrow();
		let mut root = Node::new(Role::ListBox);
		root.set_size_of_set(state.entries.len());
		let mut children = Vec::with_capacity(state.entries.len());
		let mut nodes = Vec::with_capacity(state.entries.len() + 1);
		let focus_id = if let Some(idx) = state.selected_index {
			state.entries.get(idx).map(|(id, _)| *id).unwrap_or(ROOT_ID)
		} else {
			state.entries.first().map(|(id, _)| *id).unwrap_or(ROOT_ID)
		};
		for (i, (id, text)) in state.entries.iter().enumerate() {
			children.push(*id);
			let mut node = Node::new(Role::ListBoxOption);
			node.set_label(text.clone());
			node.add_action(accesskit::Action::Focus);
			node.set_position_in_set(i);
			if *id == focus_id {
				node.set_selected(true);
			}
			nodes.push((*id, node));
		}

		root.set_children(children);
		nodes.push((ROOT_ID, root));
		Some(TreeUpdate { nodes, tree: Some(Tree::new(ROOT_ID)), focus: focus_id, tree_id: accesskit::TreeId::ROOT })
	}
}

struct Inner {
	adapter: SubclassingAdapter,
	state: Rc<RefCell<ListState>>,
	on_selection_changed: Option<Box<dyn Fn()>>,
	on_key_down: Option<Box<dyn Fn(&WindowEventData)>>,
	action_cb_raw: usize,
}

impl Drop for Inner {
	fn drop(&mut self) {
		if self.action_cb_raw != 0 {
			let _ = unsafe { Box::from_raw(self.action_cb_raw as *mut Box<dyn Fn(ActionRequest)>) };
		}
	}
}

#[derive(Clone)]
pub struct TimelineList {
	panel: Panel,
	inner: Rc<RefCell<Inner>>,
}

impl wxdragon::WxWidget for TimelineList {
	fn handle_ptr(&self) -> *mut wxdragon::ffi::wxd_Window_t {
		self.panel.handle_ptr()
	}
}

impl TimelineList {
	pub fn new(parent: &impl wxdragon::WxWidget) -> Self {
		let panel = Panel::builder(parent).with_style(PanelStyle::TabTraversal).build();
		panel.set_name("");
		panel.set_label("");
		unsafe {
			wxdragon::ffi::wxd_Window_SetWindowStyle(
				panel.as_ptr() as *mut _,
				wxdragon::ffi::wxd_Window_GetWindowStyle(panel.as_ptr() as *mut _) | 0x00040000,
			);
		}

		let hwnd = HWND(panel.get_handle() as *mut _);
		let list_state = Rc::new(RefCell::new(ListState {
			entries: Vec::new(),
			selected_index: None,
			search_buffer: String::new(),
			last_search_time: None,
		}));
		let cb_ptr = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
		let adapter = SubclassingAdapter::new(
			hwnd,
			TimelineActivationHandler { state: list_state.clone() },
			TimelineActionHandler { cb_ptr: cb_ptr.clone() },
		);

		let inner = Rc::new(RefCell::new(Inner {
			adapter,
			state: list_state,
			on_selection_changed: None,
			on_key_down: None,
			action_cb_raw: 0,
		}));

		let tl = Self { panel, inner };

		let weak_inner = Rc::downgrade(&tl.inner);
		let panel_copy = tl.panel;
		let callback: Box<dyn Fn(ActionRequest)> = Box::new(move |request| {
			if request.action == accesskit::Action::Focus {
				if let Some(inner_rc) = weak_inner.upgrade() {
					let temp_tl = TimelineList { panel: panel_copy, inner: inner_rc };
					temp_tl.set_selection(Some(request.target_node));

					let cb =
						temp_tl.inner.borrow().on_selection_changed.as_ref().map(|cb| std::ptr::from_ref(cb.as_ref()));
					if let Some(cb_ptr) = cb {
						unsafe { (*cb_ptr)() };
					}
				}
			}
		});
		let raw_cb = Box::into_raw(Box::new(callback)) as usize;
		tl.inner.borrow_mut().action_cb_raw = raw_cb;
		cb_ptr.store(raw_cb, std::sync::atomic::Ordering::Relaxed);

		tl.setup_keyboard();
		tl
	}

	fn setup_keyboard(&self) {
		let tl_clone = self.clone();
		self.panel.on_key_down(move |event| {
			let handled = tl_clone.handle_key_down(&event);
			let cb = tl_clone.inner.borrow().on_key_down.as_ref().map(|cb| std::ptr::from_ref(cb.as_ref()));
			if let Some(cb_ptr) = cb {
				unsafe { (*cb_ptr)(&event) };
			}
			if let WindowEventData::Keyboard(key_event) = event {
				key_event.event.skip(!handled);
			}
		});
	}

	fn handle_key_down(&self, event: &WindowEventData) -> bool {
		let WindowEventData::Keyboard(key_event) = event else {
			return false;
		};
		let Some(key) = key_event.get_key_code() else {
			return false;
		};
		if key == 9 {
			// TAB
			let forward = !key_event.shift_down();
			self.panel.navigate(forward);
			return true;
		}
		let state_rc = { self.inner.borrow().state.clone() };
		let mut state = state_rc.borrow_mut();
		if state.entries.is_empty() {
			return false;
		}

		let count = state.entries.len();
		let current = state.selected_index.unwrap_or(0);
		let mut new_idx = current;

		match key {
			315 => {
				// UP
				if current > 0 {
					new_idx = current - 1;
				}
			}
			317 => {
				// DOWN
				if current + 1 < count {
					new_idx = current + 1;
				}
			}
			313 => {
				// HOME
				new_idx = 0;
			}
			312 => {
				// END
				new_idx = count - 1;
			}
			_ => return false,
		}
		if new_idx != current || state.selected_index.is_none() {
			let old_idx = state.selected_index;
			state.selected_index = Some(new_idx);

			let focus_id = state.entries[new_idx].0;
			let mut nodes = Vec::new();
			if let Some(old) = old_idx {
				if old != new_idx {
					if let Some((old_id, old_text)) = state.entries.get(old) {
						let mut old_node = Node::new(Role::ListBoxOption);
						old_node.set_label(old_text.clone());
						old_node.add_action(accesskit::Action::Focus);
						old_node.set_position_in_set(old);
						nodes.push((*old_id, old_node));
					}
				}
			}

			if let Some((new_id, new_text)) = state.entries.get(new_idx) {
				let mut new_node = Node::new(Role::ListBoxOption);
				new_node.set_label(new_text.clone());
				new_node.add_action(accesskit::Action::Focus);
				new_node.set_position_in_set(new_idx);
				new_node.set_selected(true);
				nodes.push((*new_id, new_node));
			}
			let update = TreeUpdate { nodes, tree: None, focus: focus_id, tree_id: accesskit::TreeId::ROOT };
			drop(state);
			let mut inner = self.inner.borrow_mut();
			if let Some(events) = inner.adapter.update_if_active(|| update) {
				events.raise();
			}
			let cb = inner.on_selection_changed.as_ref().map(|cb| std::ptr::from_ref(cb.as_ref()));
			drop(inner);
			if let Some(cb_ptr) = cb {
				unsafe { (*cb_ptr)() };
			}
		}

		true
	}

	pub fn get_selection(&self) -> Option<i32> {
		self.inner.borrow().state.borrow().selected_index.map(|i| i as i32)
	}

	pub fn get_count(&self) -> i32 {
		self.inner.borrow().state.borrow().entries.len() as i32
	}

	pub fn clear(&self) {
		let state_rc = { self.inner.borrow().state.clone() };
		let mut state = state_rc.borrow_mut();
		state.entries.clear();
		state.selected_index = None;
		drop(state);
		let update = TreeUpdate {
			nodes: vec![(ROOT_ID, Node::new(Role::ListBox))],
			tree: None,
			focus: ROOT_ID,
			tree_id: accesskit::TreeId::ROOT,
		};
		let mut inner = self.inner.borrow_mut();
		if let Some(events) = inner.adapter.update_if_active(|| update) {
			events.raise();
		}
	}

	pub fn bind_internal<F>(&self, event_type: EventType, callback: F)
	where
		F: FnMut(Event) + 'static,
	{
		self.panel.bind_internal(event_type, callback);
	}

	pub fn popup_menu(&self, menu: &mut Menu, pos: Option<Point>) {
		self.panel.popup_menu(menu, pos);
	}

	pub fn on_selection_changed<F>(&self, callback: F)
	where
		F: Fn() + 'static,
	{
		self.inner.borrow_mut().on_selection_changed = Some(Box::new(callback));
	}

	pub fn on_key_down<F>(&self, callback: F)
	where
		F: Fn(&WindowEventData) + 'static,
	{
		self.inner.borrow_mut().on_key_down = Some(Box::new(callback));
	}

	pub fn update_entries(&self, entries: &[(NodeId, String)], selected_id: Option<NodeId>) {
		let mut seen_ids = std::collections::HashSet::new();
		let mut unique_entries = Vec::with_capacity(entries.len());
		for (id, text) in entries {
			if seen_ids.insert(*id) {
				unique_entries.push((*id, text.clone()));
			}
		}

		let mut root = Node::new(Role::ListBox);
		root.set_size_of_set(unique_entries.len());
		let mut children = Vec::with_capacity(unique_entries.len());
		let mut nodes = Vec::with_capacity(unique_entries.len() + 1);
		let valid_focus = selected_id
			.filter(|id| unique_entries.iter().any(|(eid, _)| eid == id))
			.or_else(|| unique_entries.first().map(|(id, _)| *id));
		let focus_id = valid_focus.unwrap_or(ROOT_ID);
		for (i, (id, text)) in unique_entries.iter().enumerate() {
			children.push(*id);
			let mut node = Node::new(Role::ListBoxOption);
			node.set_label(text.clone());
			node.add_action(accesskit::Action::Focus);
			node.set_position_in_set(i);
			if *id == focus_id {
				node.set_selected(true);
			}
			nodes.push((*id, node));
		}
		let state_rc = { self.inner.borrow().state.clone() };
		let mut state = state_rc.borrow_mut();
		state.entries = unique_entries;
		if let Some(id) = valid_focus {
			state.selected_index = state.entries.iter().position(|(nid, _)| *nid == id);
		} else {
			state.selected_index = None;
		}
		drop(state);

		root.set_children(children);
		nodes.push((ROOT_ID, root));
		let update = TreeUpdate { nodes, tree: None, focus: focus_id, tree_id: accesskit::TreeId::ROOT };
		let mut inner = self.inner.borrow_mut();
		if let Some(events) = inner.adapter.update_if_active(|| update) {
			events.raise();
		}
	}

	pub fn set_selection(&self, selected_id: Option<NodeId>) {
		let state_rc = { self.inner.borrow().state.clone() };
		let mut state = state_rc.borrow_mut();
		let old_idx = state.selected_index;
		let valid_focus = selected_id.filter(|id| state.entries.iter().any(|(eid, _)| eid == id));
		let focus_id = valid_focus.unwrap_or(ROOT_ID);
		let new_idx = valid_focus.and_then(|id| state.entries.iter().position(|(nid, _)| *nid == id));
		let mut nodes = Vec::new();
		if let Some(old) = old_idx {
			if Some(old) != new_idx {
				if let Some((old_id, old_text)) = state.entries.get(old) {
					let mut old_node = Node::new(Role::ListBoxOption);
					old_node.set_label(old_text.clone());
					old_node.add_action(accesskit::Action::Focus);
					old_node.set_position_in_set(old);
					nodes.push((*old_id, old_node));
				}
			}
		}
		if let Some(new) = new_idx {
			if let Some((new_id, new_text)) = state.entries.get(new) {
				let mut new_node = Node::new(Role::ListBoxOption);
				new_node.set_label(new_text.clone());
				new_node.add_action(accesskit::Action::Focus);
				new_node.set_position_in_set(new);
				new_node.set_selected(true);
				nodes.push((*new_id, new_node));
			}
		}
		state.selected_index = new_idx;
		drop(state);
		let update = TreeUpdate { nodes, tree: None, focus: focus_id, tree_id: accesskit::TreeId::ROOT };
		let mut inner = self.inner.borrow_mut();
		if let Some(events) = inner.adapter.update_if_active(|| update) {
			events.raise();
		}
	}

	pub fn type_ahead(&self, ch: char) {
		let state_rc = { self.inner.borrow().state.clone() };
		let mut state = state_rc.borrow_mut();
		if state.entries.is_empty() {
			return;
		}

		let now = Instant::now();
		let expired = state.last_search_time.map_or(true, |t| now.duration_since(t).as_millis() > 1000);
		if expired {
			state.search_buffer.clear();
		}
		state.last_search_time = Some(now);

		let lower_ch = ch.to_lowercase().next().unwrap_or(ch);
		let is_repeat = state.search_buffer.len() == 1 && state.search_buffer.starts_with(lower_ch);

		if is_repeat {
			let start = state.selected_index.map_or(0, |i| i + 1);
			let count = state.entries.len();
			let prefix: String = [lower_ch].into_iter().collect();
			let found = (0..count).find_map(|offset| {
				let idx = (start + offset) % count;
				let text = &state.entries[idx].1;
				if text.to_lowercase().starts_with(&prefix) { Some(idx) } else { None }
			});
			if let Some(idx) = found {
				state.selected_index = Some(idx);
				let focus_id = state.entries[idx].0;
				drop(state);
				self.announce_selection(idx, focus_id);
			}
		} else {
			state.search_buffer.push(lower_ch);
			let prefix = state.search_buffer.clone();
			let start = if prefix.len() == 1 {
				state.selected_index.map_or(0, |i| i + 1)
			} else {
				state.selected_index.unwrap_or(0)
			};
			let count = state.entries.len();
			let found = (0..count).find_map(|offset| {
				let idx = (start + offset) % count;
				let text = &state.entries[idx].1;
				if text.to_lowercase().starts_with(&prefix) { Some(idx) } else { None }
			});
			if let Some(idx) = found {
				state.selected_index = Some(idx);
				let focus_id = state.entries[idx].0;
				drop(state);
				self.announce_selection(idx, focus_id);
			}
		}
	}

	fn announce_selection(&self, idx: usize, focus_id: NodeId) {
		let state_rc = { self.inner.borrow().state.clone() };
		let state = state_rc.borrow();
		let mut nodes = Vec::new();
		if let Some((id, text)) = state.entries.get(idx) {
			let mut node = Node::new(Role::ListBoxOption);
			node.set_label(text.clone());
			node.add_action(accesskit::Action::Focus);
			node.set_position_in_set(idx);
			node.set_selected(true);
			nodes.push((*id, node));
		}
		drop(state);
		let update = TreeUpdate { nodes, tree: None, focus: focus_id, tree_id: accesskit::TreeId::ROOT };
		let mut inner = self.inner.borrow_mut();
		if let Some(events) = inner.adapter.update_if_active(|| update) {
			events.raise();
		}
		let cb = inner.on_selection_changed.as_ref().map(|cb| std::ptr::from_ref(cb.as_ref()));
		drop(inner);
		if let Some(cb_ptr) = cb {
			unsafe { (*cb_ptr)() };
		}
	}
}
