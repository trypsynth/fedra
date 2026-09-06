//! Modal picker notifications bypass the application's (possibly occupied) command handler.
use std::{cell::RefCell, rc::Rc, sync::Arc};

use wxdragon::{
	prelude::*,
	widgets::list_ctrl::{ListColumnFormat, ListCtrl, ListCtrlStyle, ListItemState},
};

use crate::autocomplete::{Availability, Entry, Matches, Session, Snapshot, text};

thread_local! {
	static SUBSCRIBER: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
}

#[cfg(test)]
type PickerTestHook = Rc<dyn Fn(Dialog, TextCtrl, ListCtrl, Button)>;
#[cfg(test)]
thread_local! { static TEST_HOOK: RefCell<Option<PickerTestHook>> = RefCell::new(None); }

pub fn dispatch_update() {
	let callback = SUBSCRIBER.with(|slot| slot.borrow().clone());
	if let Some(callback) = callback {
		callback();
	}
}

struct Subscription;
impl Drop for Subscription {
	fn drop(&mut self) {
		SUBSCRIBER.with(|slot| slot.borrow_mut().take());
	}
}

struct Rows {
	snapshot: Arc<Snapshot>,
	matches: Matches,
}
impl Rows {
	fn entry(&self, row: i32) -> Option<&Entry> {
		let index = self.matches.get(usize::try_from(row).ok()?)?;
		self.snapshot.entries.get(index).map(|(_, e)| e)
	}
}

fn unavailable(parent: Dialog, message: &str) {
	let dialog = Dialog::builder(&parent, "User autocomplete").with_size(510, 180).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	let label = StaticText::builder(&dialog).with_label(message).build();
	let ok = Button::builder(&dialog).with_id(ID_OK).with_label("OK").build();
	ok.set_default();
	sizer.add(&label, 1, SizerFlag::Expand | SizerFlag::All, 12);
	sizer.add(&ok, 0, SizerFlag::AlignRight | SizerFlag::All, 12);
	dialog.set_sizer(sizer, true);
	ok.on_click(move |_| dialog.end_modal(ID_OK));
	ok.set_focus();
	dialog.show_modal();
	dialog.destroy();
}

fn choose(parent: Dialog, session: &Session, initial_filter: &str) -> Option<Entry> {
	if !session.eligible() {
		return None;
	}
	let snapshot = session.snapshot();
	let message = match &snapshot.availability {
		Availability::Loading => Some("User autocomplete is loading. Please try again shortly."),
		Availability::Building => Some("User autocomplete is building its cache. Please try again shortly."),
		Availability::Unavailable(message) => Some(message.as_str()),
		Availability::Usable | Availability::UsablePartial | Availability::CompletedEmpty => None,
	};
	if let Some(message) = message {
		unavailable(parent, message);
		return None;
	}
	let dialog = Dialog::builder(&parent, "User autocomplete").with_size(620, 480).build();
	let panel = Panel::builder(&dialog).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	let filter_label = StaticText::builder(&panel).with_label("&Filter").build();
	let filter = TextCtrl::builder(&panel).with_value(initial_filter).build();
	filter.set_name("Filter");
	let users_label = StaticText::builder(&panel).with_label("&Users").build();
	let users = ListCtrl::builder(&panel)
		.with_style(ListCtrlStyle::Report | ListCtrlStyle::Virtual | ListCtrlStyle::SingleSel | ListCtrlStyle::NoHeader)
		.build();
	users.set_name("Users");
	users.insert_column(0, "Users", ListColumnFormat::Left, 570);
	let buttons = BoxSizer::builder(Orientation::Horizontal).build();
	let ok = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
	let cancel = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	ok.set_default();
	buttons.add_stretch_spacer(1);
	buttons.add(&ok, 0, SizerFlag::Right, 8);
	buttons.add(&cancel, 0, SizerFlag::Right, 8);
	sizer.add(&filter_label, 0, SizerFlag::All, 8);
	sizer.add(&filter, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	sizer.add(&users_label, 0, SizerFlag::All, 8);
	sizer.add(&users, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	if let Some(error) = &snapshot.persistence_error {
		let label =
			StaticText::builder(&panel).with_label(&format!("Cache changes are not being saved: {error}")).build();
		sizer.add(&label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	}
	sizer.add_sizer(&buttons, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(sizer, true);
	let outer = BoxSizer::builder(Orientation::Vertical).build();
	outer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(outer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let rows = Rc::new(RefCell::new(Rows { matches: snapshot.matching(initial_filter), snapshot }));
	let text_rows = rows.clone();
	users.set_virtual_text_callback(move |row, _| {
		text_rows.borrow().entry(i32::try_from(row).unwrap_or(-1)).map_or_else(|| "No matches".into(), Entry::label)
	});
	let refresh_rows = rows.clone();
	let refresh_session = session.clone();
	let refresh: Rc<dyn Fn(bool)> = Rc::new(move |background| {
		if !refresh_session.eligible() {
			dialog.end_modal(ID_CANCEL);
			return;
		}
		let snapshot = refresh_session.snapshot();
		if background
			&& snapshot.revision == refresh_rows.borrow().snapshot.revision
			&& Arc::ptr_eq(&snapshot, &refresh_rows.borrow().snapshot)
		{
			return;
		}
		let selected = if background {
			refresh_rows.borrow().entry(users.get_first_selected_item()).map(|e| e.id.clone())
		} else {
			None
		};
		let updated = Rows { matches: snapshot.matching(&filter.get_value()), snapshot };
		let count = updated.matches.len();
		let selection = selected
			.and_then(|id| {
				(0..count).position(|row| {
					updated.matches.get(row).is_some_and(|index| updated.snapshot.entries[index].1.id == id)
				})
			})
			.unwrap_or(0);
		*refresh_rows.borrow_mut() = updated;
		// Drop RefCell borrows before native calls, which can synchronously request row text.
		users.set_item_state(-1, ListItemState::default(), ListItemState::Selected | ListItemState::Focused);
		users.set_item_count(i64::try_from(count.max(1)).unwrap_or(i64::MAX));
		let selection = i64::try_from(selection).unwrap_or(0);
		users.set_item_state(
			selection,
			ListItemState::Selected | ListItemState::Focused,
			ListItemState::Selected | ListItemState::Focused,
		);
		users.ensure_visible(selection);
		users.refresh_items(0, i64::try_from(count.max(1) - 1).unwrap_or(0));
		ok.enable(count > 0);
	});
	refresh(false);
	let update = refresh.clone();
	filter.on_text_changed(move |_| update(false));
	let update = refresh;
	SUBSCRIBER.with(|slot| *slot.borrow_mut() = Some(Rc::new(move || update(true))));
	let subscription = Subscription;
	let selection_rows = rows.clone();
	users.on_item_selected(move |_| {
		ok.enable(selection_rows.borrow().entry(users.get_first_selected_item()).is_some());
	});
	let deselect_rows = rows.clone();
	users.on_item_deselected(move |_| {
		ok.enable(deselect_rows.borrow().entry(users.get_first_selected_item()).is_some());
	});
	let accept_rows = rows.clone();
	let accept_session = session.clone();
	let accept: Rc<dyn Fn()> = Rc::new(move || {
		if !accept_session.eligible() {
			dialog.end_modal(ID_CANCEL);
			return;
		}
		if let Some(id) = accept_rows.borrow().entry(users.get_first_selected_item()).map(|e| e.id.clone())
			&& accept_session.snapshot().entries.iter().any(|(_, e)| e.id == id)
		{
			dialog.end_modal(ID_OK);
		}
	});
	let click = accept.clone();
	ok.on_click(move |_| click());
	cancel.on_click(move |_| dialog.end_modal(ID_CANCEL));
	let activate = accept.clone();
	users.on_item_activated(move |_| activate());
	// CHAR_HOOK runs before default buttons or parent accelerators, including on No matches.
	let handle_key: Rc<dyn Fn(WindowEventData)> = Rc::new(move |data| {
		if let WindowEventData::Keyboard(key) = &data {
			match key.get_key_code() {
				Some(13) => {
					accept();
					data.skip(false);
					return;
				}
				Some(27) => {
					dialog.end_modal(ID_CANCEL);
					data.skip(false);
					return;
				}
				Some(70) if key.alt_down() => {
					filter.set_focus();
					data.skip(false);
					return;
				}
				Some(85) if key.alt_down() => {
					users.set_focus();
					data.skip(false);
					return;
				}
				_ => (),
			}
		}
		data.skip(true);
	});
	let hook = handle_key.clone();
	dialog.bind_internal(EventType::CHAR_HOOK, move |event| hook(WindowEventData::new(event)));
	let filter_keys = handle_key.clone();
	filter.on_key_down(move |event| filter_keys(event));
	users.bind_internal(EventType::KEY_DOWN, move |event| handle_key(WindowEventData::new(event)));
	dialog.on_close(move |event| {
		dialog.end_modal(ID_CANCEL);
		event.skip(false);
	});
	dialog.centre();
	if initial_filter.chars().any(char::is_alphanumeric) {
		users.set_focus();
	} else {
		filter.set_focus();
	}
	#[cfg(test)]
	TEST_HOOK.with(|hook| {
		if let Some(hook) = hook.borrow().as_ref() {
			hook(dialog, filter, users, ok);
		}
	});
	let result = dialog.show_modal();
	drop(subscription);
	let entry = if result == ID_OK && session.eligible() {
		let id = rows.borrow().entry(users.get_first_selected_item()).map(|e| e.id.clone());
		id.and_then(|id| session.snapshot().entries.iter().find(|(_, e)| e.id == id).map(|(_, e)| e.clone()))
	} else {
		None
	};
	users.clear_virtual_text_callback();
	dialog.destroy();
	entry
}

pub fn insert(parent: Dialog, body: TextCtrl, session: &Session) {
	let original = body.get_value();
	let (from, to) = body.get_selection();
	let byte = |position| text::native_to_byte(&original, usize::try_from(position).unwrap_or(0), cfg!(windows));
	let edit = text::prepare(&original, byte(body.get_insertion_point()), byte(from)..byte(to));
	if let Some(entry) = choose(parent, session, &edit.filter)
		&& session.eligible()
	{
		let (replacement, caret) = edit.replacement(&original, &entry.address);
		let from = text::byte_to_native(&original, edit.range.start, cfg!(windows));
		let to = text::byte_to_native(&original, edit.range.end, cfg!(windows));
		let mut updated = original;
		updated.replace_range(edit.range, &replacement);
		let native_caret = text::byte_to_native(&updated, caret, cfg!(windows));
		undoable_replace(body, from, to, &replacement);
		body.set_insertion_point(i64::try_from(native_caret).unwrap_or(i64::MAX));
	}
	body.set_focus();
}

#[cfg(windows)]
fn undoable_replace(body: TextCtrl, from: usize, to: usize, text: &str) {
	use windows::Win32::{
		Foundation::{HWND, LPARAM, WPARAM},
		UI::{
			Controls::{EM_REPLACESEL, EM_SETSEL},
			WindowsAndMessaging::SendMessageW,
		},
	};
	let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
	unsafe {
		SendMessageW(
			HWND(body.get_handle()),
			EM_SETSEL,
			Some(WPARAM(from)),
			Some(LPARAM(isize::try_from(to).unwrap_or(isize::MAX))),
		);
		SendMessageW(HWND(body.get_handle()), EM_REPLACESEL, Some(WPARAM(1)), Some(LPARAM(text.as_ptr() as isize)));
	}
}

#[cfg(all(test, windows))]
mod native_tests {
	use std::{
		cell::Cell,
		sync::{
			Mutex,
			atomic::{AtomicBool, Ordering},
		},
		time::{Duration, Instant},
	};

	use windows::Win32::{
		Foundation::{HWND, LPARAM, WPARAM},
		UI::{
			Controls::EM_UNDO,
			WindowsAndMessaging::{PostMessageW, SendMessageW, WM_KEYDOWN},
		},
	};

	use super::*;

	#[test]
	#[ignore = "Requires a Windows desktop; opens an isolated native picker without configured accounts"]
	fn native_picker_live_updates_keys_positions_and_undo() {
		let failure = Arc::new(Mutex::new(None::<String>));
		let failure_ui = failure.clone();
		wxdragon::main(move |_| {
			let frame = Frame::builder().with_title("Fedra autocomplete UI test").build();
			let alive = Arc::new(AtomicBool::new(true));
			let waker = crate::ui_wake::UiWaker::with_event(frame, alive.clone(), crate::ID_AUTOCOMPLETE_WAKE);
			let wake_handler = waker.clone();
			frame.bind_with_id_internal(EventType::MENU, crate::ID_AUTOCOMPLETE_WAKE, move |_| {
				wake_handler.reset();
				dispatch_update();
			});
			let dir = std::env::temp_dir().join(format!("fedra-native-picker-{}", std::process::id()));
			let service = crate::autocomplete::Service::start(
				dir.join("autocomplete-cache.json"),
				["a".into()].into_iter().collect(),
				waker,
			);
			service.select_for_ui_test("a");
			let session = service.session("a".into());
			let alice = Entry {
				id: "alice".into(),
				unavailable: false,
				address: "@alice@example.org".into(),
				display_name: "Alison Example".into(),
				revision: 0,
			};
			session.interaction(alice.clone());
			let deadline = Instant::now() + Duration::from_secs(3);
			while session.snapshot().entries.is_empty() && Instant::now() < deadline {
				std::thread::sleep(Duration::from_millis(5));
			}
			let parent = Dialog::builder(&frame, "Composer test").build();
			let body = TextCtrl::builder(&parent).with_style(TextCtrlStyle::MultiLine).build();
			let run = || -> anyhow::Result<()> {
				let original = "😀 hello\n@al!";
				body.set_value(original);
				body.set_insertion_point_end();
				anyhow::ensure!(
					usize::try_from(body.get_insertion_point())?
						== text::byte_to_native(original, original.len(), true),
					"Native multiline/UTF-16 adapter disagrees with wxMSW"
				);
				let caret = original.find('!').unwrap();
				let edit = text::prepare(original, caret, caret..caret);
				let (replacement, _) = edit.replacement(original, &alice.address);
				undoable_replace(
					body,
					text::byte_to_native(original, edit.range.start, true),
					text::byte_to_native(original, edit.range.end, true),
					&replacement,
				);
				anyhow::ensure!(
					body.get_value() == "😀 hello\n@alice@example.org!",
					"Native insertion changed unrelated text"
				);
				unsafe {
					SendMessageW(HWND(body.get_handle()), EM_UNDO, None, None);
				}
				anyhow::ensure!(body.get_value() == original, "One Undo did not restore the original text");

				let timer_keepalive: Rc<RefCell<Option<Timer<Dialog>>>> = Rc::new(RefCell::new(None));
				let timers = timer_keepalive.clone();
				let errors = failure_ui.clone();
				let session_updates = session.clone();
				let alice_updates = alice.clone();
				TEST_HOOK.with(|slot| {
					*slot.borrow_mut() = Some(Rc::new(move |dialog, filter, users, ok| {
						let timer = Timer::new(&dialog);
						let phase = Cell::new(0);
						let started = Instant::now();
						let errors = errors.clone();
						let updates = session_updates.clone();
						let alice = alice_updates.clone();
						timer.on_tick(move |_| {
							let step =
								|| -> anyhow::Result<()> {
									anyhow::ensure!(
										started.elapsed() < Duration::from_secs(5),
										"Picker timed out at phase {} (rows {}, first {})",
										phase.get(),
										users.get_item_count(),
										users.get_item_text(0, 0)
									);
									match phase.get() {
										0 => {
											anyhow::ensure!(
												users.get_item_count() == 1
													&& users.get_first_selected_item() == 0 && ok.is_enabled(),
												"Initial result selection"
											);
											anyhow::ensure!(users.has_focus(), "Seeded mention must focus Users");
											filter.set_focus();
											filter.set_value("does-not-match");
											phase.set(1);
										}
										1 => {
											anyhow::ensure!(
												users.get_item_count() == 1
													&& users.get_item_text(0, 0) == "No matches"
													&& users.get_first_selected_item() == 0 && !ok.is_enabled(),
												"No matches row must be selected and OK disabled"
											);
											unsafe {
												PostMessageW(
													Some(HWND(filter.get_handle())),
													WM_KEYDOWN,
													WPARAM(13),
													LPARAM(0),
												)?;
											}
											phase.set(2);
										}
										2 => {
											filter.set_value("");
											updates.relationship("alice".into(), crate::autocomplete::Change::Unfollow);
											phase.set(3);
										}
										3 if users.get_item_text(0, 0) == "No matches" => {
											anyhow::ensure!(filter.has_focus(), "Background removal changed focus");
											updates.interaction(alice.clone());
											phase.set(4);
										}
										4 if users.get_item_text(0, 0).contains("@alice") => {
											updates.interaction(Entry {
												id: "aaron".into(),
												unavailable: false,
												address: "@aaron@example.org".into(),
												display_name: String::new(),
												revision: 0,
											});
											phase.set(5);
										}
										5 if users.get_item_count() == 2 => {
											anyhow::ensure!(
												users.get_first_selected_item() == 1 && filter.has_focus(),
												"Background additions must preserve selected identity and focus"
											);
											unsafe {
												PostMessageW(
													Some(HWND(filter.get_handle())),
													WM_KEYDOWN,
													WPARAM(27),
													LPARAM(0),
												)?;
											}
											phase.set(6);
										}
										_ => (),
									}
									Ok(())
								};
							if let Err(error) = step() {
								*errors.lock().unwrap() = Some(error.to_string());
								dialog.end_modal(ID_CANCEL);
							}
						});
						timer.start(30, false);
						*timers.borrow_mut() = Some(timer);
					}))
				});
				// This prefix matches the display name only, exercising indexed virtual rows.
				let result = choose(parent, &session, "@alis");
				if let Some(timer) = timer_keepalive.borrow_mut().take() {
					timer.stop();
				}
				TEST_HOOK.with(|slot| slot.borrow_mut().take());
				anyhow::ensure!(result.is_none(), "No matches Enter or Escape accepted an account");
				// Exercise the complete body adapter and acceptance path, including one-step Undo.
				for key in [13, 27] {
					let timers = timer_keepalive.clone();
					TEST_HOOK.with(|slot| {
						*slot.borrow_mut() = Some(Rc::new(move |dialog, filter, users, _| {
							let timer = Timer::new(&dialog);
							timer.on_tick(move |_| {
								let target = if key == 13 { users.get_handle() } else { filter.get_handle() };
								unsafe {
									let _ = PostMessageW(Some(HWND(target)), WM_KEYDOWN, WPARAM(key), LPARAM(0));
								}
							});
							timer.start(30, true);
							*timers.borrow_mut() = Some(timer);
						}))
					});
					body.set_insertion_point(i64::try_from(text::byte_to_native(original, caret, true))?);
					let selection = body.get_selection();
					insert(parent, body, &session);
					if let Some(timer) = timer_keepalive.borrow_mut().take() {
						timer.stop();
					}
					TEST_HOOK.with(|slot| slot.borrow_mut().take());
					if key == 13 {
						let expected = "😀 hello\n@alice@example.org!";
						anyhow::ensure!(
							body.get_value() == expected,
							"Picker acceptance failed to replace the mention"
						);
						anyhow::ensure!(
							usize::try_from(body.get_insertion_point())?
								== text::byte_to_native(expected, expected.len() - 1, true),
							"Caret must precede following punctuation"
						);
						unsafe {
							SendMessageW(HWND(body.get_handle()), EM_UNDO, None, None);
						}
					} else {
						anyhow::ensure!(body.get_selection() == selection, "Cancel changed selection");
					}
					anyhow::ensure!(body.get_value() == original, "Cancel or Undo changed the original body");
				}
				Ok(())
			};
			if let Err(error) = run() {
				*failure_ui.lock().unwrap() = Some(error.to_string());
			}
			service.shutdown();
			let deadline = Instant::now() + Duration::from_secs(3);
			while service.shutdown_result().is_none() && Instant::now() < deadline {
				std::thread::sleep(Duration::from_millis(5));
			}
			alive.store(false, Ordering::SeqCst);
			parent.destroy();
			frame.destroy();
			let _ = std::fs::remove_file(dir.join("autocomplete-cache.json"));
			let _ = std::fs::remove_dir(dir);
		})
		.unwrap();
		assert!(failure.lock().unwrap().is_none(), "Native UI check failed: {:?}", failure.lock().unwrap());
	}
}

#[cfg(not(windows))]
fn undoable_replace(body: TextCtrl, from: usize, to: usize, text: &str) {
	body.replace(i64::try_from(from).unwrap_or(i64::MAX), i64::try_from(to).unwrap_or(i64::MAX), text);
}
