//! Spell checking for the text boxes of Fedra's dialogs, using Windows' own spell checker.
//!
//! Mistakes are underlined with a squiggle drawn over the edit control, F7 moves to the next one,
//! and right-clicking one offers fixes. Ported from whatSnap's message box. Each box is watched on
//! its own, keyed by its window handle, so any number can be checked at once.

// Win32 packs coordinates and character indices into message words and hands back raw
// pointers, so the cast and raw-pointer lints would only flag the FFI itself.
#![allow(
	clippy::cast_possible_truncation,
	clippy::cast_possible_wrap,
	clippy::cast_sign_loss,
	clippy::cast_lossless,
	clippy::borrow_as_ptr,
	clippy::ref_as_ptr,
	clippy::trivially_copy_pass_by_ref,
	clippy::option_option,
	clippy::needless_pass_by_value,
	reason = "Win32 FFI: packed message words, coordinates and raw pointers"
)]

use std::{
	cell::{Cell, RefCell},
	collections::HashMap,
};

use windows::{
	Win32::{
		Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
		Globalization::{
			CORRECTIVE_ACTION, CORRECTIVE_ACTION_DELETE, CORRECTIVE_ACTION_REPLACE, GetLocaleInfoEx,
			GetUserDefaultLocaleName, ISpellChecker, ISpellCheckerFactory, LOCALE_SLOCALIZEDDISPLAYNAME,
			SpellCheckerFactory,
		},
		Graphics::Gdi::{
			CreatePen, DeleteObject, GetDC, GetTextExtentPoint32W, GetTextMetricsW, HFONT, HGDIOBJ, IntersectClipRect,
			PS_SOLID, Polyline, ReleaseDC, SelectObject, TEXTMETRICW,
		},
		System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IEnumString},
		UI::{
			Controls::{
				EM_CHARFROMPOS, EM_GETRECT, EM_GETSEL, EM_POSFROMCHAR, EM_REPLACESEL, EM_SCROLLCARET, EM_SETSEL,
			},
			Input::KeyboardAndMouse::VK_F7,
			Shell::{DefSubclassProc, SetWindowSubclass},
			WindowsAndMessaging::{
				GetWindowTextLengthW, GetWindowTextW, HideCaret, SendMessageW, ShowCaret, WM_CHAR, WM_CLEAR,
				WM_CONTEXTMENU, WM_COPY, WM_CUT, WM_GETFONT, WM_HSCROLL, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS,
				WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_PASTE, WM_SETFOCUS, WM_SETTEXT,
				WM_SIZE, WM_UNDO, WM_VSCROLL,
			},
		},
	},
	core::{PCWSTR, PWSTR},
};
use wxdragon::prelude::*;

/// Whether this platform has a spell checker to offer the options dialog.
pub const AVAILABLE: bool = true;

const MAX_CHECKED: usize = 20_000;
const MAX_SUGGESTIONS: usize = 7;
const SUBCLASS_ID: usize = 0x5350_454C;

const MENU_ID_SUGGESTION: i32 = 7600;
const MENU_ID_DELETE_REPEAT: i32 = 7620;
const MENU_ID_ADD: i32 = 7621;
const MENU_ID_IGNORE: i32 = 7622;
const MENU_ID_UNDO: i32 = 7623;
const MENU_ID_CUT: i32 = 7624;
const MENU_ID_COPY: i32 = 7625;
const MENU_ID_PASTE: i32 = 7626;
const MENU_ID_DELETE: i32 = 7627;
const MENU_ID_SELECT_ALL: i32 = 7628;

#[derive(Debug, Clone, PartialEq)]
struct Mistake {
	start: u32,
	len: u32,
	repeated: bool,
	replacement: Option<String>,
}

/// What the checker keeps for one watched box.
struct Entry {
	ctrl: TextCtrl,
	announcer: StaticText,
	mistakes: Vec<Mistake>,
	last_len: usize,
}

thread_local! {
	static ENABLED: Cell<bool> = const { Cell::new(false) };
	static ANNOUNCE: Cell<bool> = const { Cell::new(false) };
	static LANGUAGE: RefCell<String> = const { RefCell::new(String::new()) };
	static CHECKER: RefCell<Option<Option<ISpellChecker>>> = const { RefCell::new(None) };
	static FIELDS: RefCell<HashMap<isize, Entry>> = RefCell::new(HashMap::new());
}

fn wide(s: &str) -> Vec<u16> {
	s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn take_pwstr(p: PWSTR) -> Option<String> {
	if p.is_null() {
		return None;
	}
	let s = unsafe { p.to_string() }.ok();
	unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
	s
}

unsafe fn strings(list: IEnumString, max: usize) -> Vec<String> {
	let mut out = Vec::new();
	while out.len() < max {
		let mut item = [PWSTR::null()];
		let mut got = 0u32;
		let hr = unsafe { list.Next(&mut item, Some(&mut got)) };
		if hr.is_err() || got == 0 {
			break;
		}
		if let Some(s) = unsafe { take_pwstr(item[0]) } {
			out.push(s);
		}
	}
	out
}

fn factory() -> Option<ISpellCheckerFactory> {
	unsafe { CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER) }.ok()
}

/// Every language the system spell checker has, as (tag, display name), sorted by name.
pub fn languages() -> Vec<(String, String)> {
	let Some(factory) = factory() else { return Vec::new() };
	let Ok(list) = (unsafe { factory.SupportedLanguages() }) else { return Vec::new() };
	let mut out: Vec<(String, String)> = unsafe { strings(list, 200) }
		.into_iter()
		.map(|tag| {
			let name = language_name(&tag).unwrap_or_else(|| tag.clone());
			(tag, name)
		})
		.collect();
	out.sort_by(|a, b| a.1.cmp(&b.1));
	out
}

fn language_name(tag: &str) -> Option<String> {
	let tag_w = wide(tag);
	let mut buf = [0u16; 128];
	let len = unsafe { GetLocaleInfoEx(PCWSTR(tag_w.as_ptr()), LOCALE_SLOCALIZEDDISPLAYNAME, Some(&mut buf)) };
	(len > 1).then(|| String::from_utf16_lossy(&buf[..len as usize - 1]))
}

fn pick_language(chosen: &str, supported: &[String]) -> Option<String> {
	if !chosen.is_empty() && supported.iter().any(|t| t.eq_ignore_ascii_case(chosen)) {
		return Some(chosen.to_string());
	}
	let mut buf = [0u16; 85];
	let len = unsafe { GetUserDefaultLocaleName(&mut buf) };
	let user = if len > 1 { String::from_utf16_lossy(&buf[..len as usize - 1]) } else { String::new() };
	best_match(&user, supported)
}

fn best_match(user: &str, supported: &[String]) -> Option<String> {
	if let Some(t) = supported.iter().find(|t| t.eq_ignore_ascii_case(user)) {
		return Some(t.clone());
	}
	let family = user.split('-').next().unwrap_or("");
	if !family.is_empty()
		&& let Some(t) = supported.iter().find(|t| t.split('-').next().is_some_and(|f| f.eq_ignore_ascii_case(family)))
	{
		return Some(t.clone());
	}
	supported.iter().find(|t| t.eq_ignore_ascii_case("en-US")).or_else(|| supported.first()).cloned()
}

fn with_checker<R>(f: impl FnOnce(&ISpellChecker) -> R) -> Option<R> {
	CHECKER.with(|c| {
		let mut c = c.borrow_mut();
		if c.is_none() {
			let made = (|| {
				let factory = factory()?;
				let list = unsafe { factory.SupportedLanguages() }.ok()?;
				let supported = unsafe { strings(list, 200) };
				let tag = LANGUAGE.with(|l| pick_language(&l.borrow(), &supported))?;
				let tag_w = wide(&tag);
				unsafe { factory.CreateSpellChecker(PCWSTR(tag_w.as_ptr())) }.ok()
			})();
			*c = Some(made);
		}
		c.as_ref().and_then(|c| c.as_ref()).map(f)
	})
}

/// Applies the settings. Called at startup and whenever the options change.
pub fn configure(enabled: bool, announce: bool, language: &str) {
	ENABLED.with(|e| e.set(enabled));
	ANNOUNCE.with(|a| a.set(announce));
	let changed = LANGUAGE.with(|l| {
		let mut l = l.borrow_mut();
		let changed = *l != language;
		*l = language.to_string();
		changed
	});
	if changed || !enabled {
		CHECKER.with(|c| c.borrow_mut().take());
	}
	let handles: Vec<HWND> = FIELDS.with(|fields| fields.borrow().values().map(|e| hwnd(&e.ctrl)).collect());
	for handle in handles {
		recheck(handle);
	}
}

pub fn is_enabled() -> bool {
	ENABLED.with(Cell::get)
}

/// Whether a word typed in a box should also be announced when it's misspelled.
pub fn announce_enabled() -> bool {
	ANNOUNCE.with(Cell::get)
}

/// A hidden label that says spelling messages through the screen reader. Give one to [`install`].
pub fn announcer(parent: &impl WxWidget) -> StaticText {
	let label = StaticText::builder(parent).with_label("").with_size(Size::new(0, 0)).build();
	label.show(false);
	live_region::set_live_region(&label);
	label
}

fn hwnd(field: &TextCtrl) -> HWND {
	HWND(field.get_handle().cast())
}

fn key(handle: HWND) -> isize {
	handle.0 as isize
}

/// Runs `f` on the entry for a watched box. Never send Win32 messages from inside `f`: they can
/// re-enter the subclass, which borrows the same map.
fn with_entry<R>(handle: HWND, f: impl FnOnce(&mut Entry) -> R) -> Option<R> {
	FIELDS.with(|fields| fields.borrow_mut().get_mut(&key(handle)).map(f))
}

fn ctrl_of(handle: HWND) -> Option<TextCtrl> {
	with_entry(handle, |e| e.ctrl)
}

fn announce_via(handle: HWND, say: &str) {
	if let Some(announcer) = with_entry(handle, |e| e.announcer) {
		live_region::announce_with_priority(announcer, say, live_region::Priority::Medium);
	}
}

fn text(handle: HWND) -> Vec<u16> {
	unsafe {
		let len = GetWindowTextLengthW(handle).max(0) as usize;
		let mut buf = vec![0u16; len + 1];
		let got = GetWindowTextW(handle, &mut buf).max(0) as usize;
		buf.truncate(got);
		buf
	}
}

fn selection(handle: HWND) -> (u32, u32) {
	let (mut start, mut end) = (0u32, 0u32);
	unsafe {
		SendMessageW(
			handle,
			EM_GETSEL,
			Some(WPARAM(&mut start as *mut u32 as usize)),
			Some(LPARAM(&mut end as *mut u32 as isize)),
		);
	}
	(start, end)
}

fn in_skipped_token(text: &[u16], start: usize, len: usize) -> bool {
	let is_space = |c: u16| char::from_u32(c as u32).is_some_and(char::is_whitespace);
	let from = text[..start].iter().rposition(|&c| is_space(c)).map_or(0, |p| p + 1);
	let to = text[start + len..].iter().position(|&c| is_space(c)).map_or(text.len(), |p| start + len + p);
	let token = String::from_utf16_lossy(&text[from..to]);
	token.starts_with('@')
		|| token.contains("://")
		|| token.to_ascii_lowercase().starts_with("www.")
		|| (token.contains('@') && token.contains('.'))
}

fn check(text: &[u16]) -> Vec<Mistake> {
	if text.is_empty() || text.len() > MAX_CHECKED {
		return Vec::new();
	}
	let mut z = text.to_vec();
	z.push(0);
	with_checker(|checker| {
		let mut out = Vec::new();
		let Ok(errors) = (unsafe { checker.Check(PCWSTR(z.as_ptr())) }) else { return out };
		loop {
			let mut error = None;
			if unsafe { errors.Next(&mut error) }.0 != 0 {
				break;
			}
			let Some(error) = error else { break };
			let (Ok(start), Ok(len)) = (unsafe { error.StartIndex() }, unsafe { error.Length() }) else { continue };
			let action: CORRECTIVE_ACTION = unsafe { error.CorrectiveAction() }.unwrap_or_default();
			if len == 0 || (start + len) as usize > text.len() || in_skipped_token(text, start as usize, len as usize) {
				continue;
			}
			let replacement = if action == CORRECTIVE_ACTION_REPLACE {
				unsafe { error.Replacement() }.ok().and_then(|p| unsafe { take_pwstr(p) })
			} else {
				None
			};
			out.push(Mistake { start, len, repeated: action == CORRECTIVE_ACTION_DELETE, replacement });
		}
		out
	})
	.unwrap_or_default()
}

/// Rechecks one box and redraws its squiggles. Returns the box's text.
fn recheck(handle: HWND) -> Vec<u16> {
	let all = text(handle);
	let mistakes = if is_enabled() { check(&all) } else { Vec::new() };
	with_entry(handle, |e| e.mistakes = mistakes);
	unsafe { draw_squiggles(handle) };
	all
}

fn is_word_end(c: u16) -> bool {
	char::from_u32(c as u32).is_some_and(|c| c.is_whitespace() || (c.is_ascii_punctuation() && c != '\'' && c != '-'))
}

fn just_finished(mistakes: &[Mistake], text: &[u16], caret: u32) -> Option<String> {
	let caret = caret as usize;
	if caret < 2 || caret > text.len() || !is_word_end(text[caret - 1]) {
		return None;
	}
	let word_end = caret - 1;
	mistakes
		.iter()
		.find(|m| (m.start + m.len) as usize == word_end)
		.map(|m| String::from_utf16_lossy(&text[m.start as usize..word_end]))
}

/// Starts checking `field`, announcing through `announcer`. Call [`detach`] once the dialog
/// holding the box has closed.
pub fn install(field: &TextCtrl, announcer: StaticText) {
	let handle = hwnd(field);
	FIELDS.with(|fields| {
		fields.borrow_mut().insert(key(handle), Entry { ctrl: *field, announcer, mistakes: Vec::new(), last_len: 0 });
	});
	unsafe {
		let _ = SetWindowSubclass(handle, Some(edit_proc), SUBCLASS_ID, 0);
	}
	recheck(handle);
}

/// Stops checking `field`, so nothing reads it once its dialog is closing.
pub fn detach(field: &TextCtrl) {
	let handle = hwnd(field);
	FIELDS.with(|fields| {
		fields.borrow_mut().remove(&key(handle));
	});
}

/// Rechecks `field` now, without announcing. For text that was set before it was watched.
pub fn refresh(field: &TextCtrl) {
	recheck(hwnd(field));
}

/// Rechecks `field` after its text changed. When a word has just been finished wrongly and
/// announcing is on, says so.
pub fn text_changed(field: &TextCtrl) {
	let handle = hwnd(field);
	let all = recheck(handle);
	let Some((grew, announcer)) = with_entry(handle, |e| {
		let grew = all.len() > e.last_len;
		e.last_len = all.len();
		(grew, e.announcer)
	}) else {
		return;
	};
	if !announce_enabled() || !is_enabled() || !grew || !field.has_focus() {
		return;
	}
	let (start, end) = selection(handle);
	if start != end {
		return;
	}
	let word = with_entry(handle, |e| just_finished(&e.mistakes, &all, end)).flatten();
	if let Some(word) = word {
		live_region::announce_with_priority(announcer, &format!("Misspelled: {word}"), live_region::Priority::Medium);
	}
}

unsafe extern "system" fn edit_proc(
	handle: HWND,
	msg: u32,
	wparam: WPARAM,
	lparam: LPARAM,
	_id: usize,
	_data: usize,
) -> LRESULT {
	if let Some(field) = ctrl_of(handle) {
		if msg == WM_CONTEXTMENU {
			let (x, y) = ((lparam.0 & 0xFFFF) as i16 as i32, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32);
			let screen = (x != -1 || y != -1).then(|| Point::new(x, y));
			if context_menu(&field, screen) {
				return LRESULT(0);
			}
		}
		// F7 is handled by the box that has focus, so it works in every watched box.
		if msg == WM_KEYDOWN && wparam.0 == usize::from(VK_F7.0) {
			next_error(&field);
			return LRESULT(0);
		}
	}
	let result = unsafe { DefSubclassProc(handle, msg, wparam, lparam) };
	let redraw = match msg {
		WM_PAINT | WM_CHAR | WM_KEYDOWN | WM_KEYUP | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEWHEEL | WM_VSCROLL
		| WM_HSCROLL | WM_SETFOCUS | WM_KILLFOCUS | WM_SETTEXT | WM_CUT | WM_PASTE | WM_CLEAR | WM_UNDO | WM_SIZE => true,
		WM_MOUSEMOVE => wparam.0 & 0x0001 != 0,
		_ => msg == EM_SETSEL || msg == EM_REPLACESEL || msg == WM_COPY,
	};
	if redraw && is_enabled() {
		unsafe { draw_squiggles(handle) };
	}
	result
}

fn pos_from_char(handle: HWND, index: u32) -> Option<(i32, i32)> {
	let r = unsafe { SendMessageW(handle, EM_POSFROMCHAR, Some(WPARAM(index as usize)), None) }.0;
	if r == -1 {
		return None;
	}
	Some(((r & 0xFFFF) as i16 as i32, ((r >> 16) & 0xFFFF) as i16 as i32))
}

unsafe fn draw_squiggles(handle: HWND) {
	let mistakes = with_entry(handle, |e| e.mistakes.clone()).unwrap_or_default();
	if mistakes.is_empty() {
		return;
	}
	let all = text(handle);
	unsafe {
		let mut rect = RECT::default();
		SendMessageW(handle, EM_GETRECT, None, Some(LPARAM(&mut rect as *mut RECT as isize)));
		let dc = GetDC(Some(handle));
		if dc.is_invalid() {
			return;
		}
		let font = HFONT(SendMessageW(handle, WM_GETFONT, None, None).0 as *mut _);
		let old_font = (!font.is_invalid()).then(|| SelectObject(dc, HGDIOBJ(font.0)));
		let mut tm = TEXTMETRICW::default();
		let _ = GetTextMetricsW(dc, &mut tm);
		let pen = CreatePen(PS_SOLID, 1, windows::Win32::Foundation::COLORREF(0x0035_39E5));
		let old_pen = SelectObject(dc, HGDIOBJ(pen.0));
		IntersectClipRect(dc, rect.left, rect.top, rect.right, rect.bottom);
		let _ = HideCaret(Some(handle));
		for m in &mistakes {
			let (start, end) = (m.start as usize, (m.start + m.len) as usize);
			if end > all.len() {
				continue;
			}
			let Some((x, y)) = pos_from_char(handle, m.start) else { continue };
			if y < rect.top - tm.tmHeight || y > rect.bottom {
				continue;
			}
			let mut last = end;
			while last > start + 1 && pos_from_char(handle, last as u32 - 1).is_some_and(|(_, ly)| ly != y) {
				last -= 1;
			}
			let mut size = SIZE::default();
			let _ = GetTextExtentPoint32W(dc, &all[start..last], &mut size);
			let base = y + tm.tmAscent + 2;
			let mut points = Vec::with_capacity(size.cx as usize / 2 + 2);
			let mut px = x;
			let mut up = false;
			while px <= x + size.cx {
				points.push(POINT { x: px, y: if up { base } else { base + 2 } });
				up = !up;
				px += 2;
			}
			if points.len() > 1 {
				let _ = Polyline(dc, &points);
			}
		}
		let _ = ShowCaret(Some(handle));
		SelectObject(dc, old_pen);
		let _ = DeleteObject(HGDIOBJ(pen.0));
		if let Some(old) = old_font {
			SelectObject(dc, old);
		}
		ReleaseDC(Some(handle), dc);
	}
}

fn mistake_at(handle: HWND, index: u32) -> Option<Mistake> {
	with_entry(handle, |e| e.mistakes.iter().find(|m| m.start <= index && index <= m.start + m.len).cloned()).flatten()
}

fn suggestions(word: &str) -> Vec<String> {
	let word_w = wide(word);
	with_checker(|c| {
		unsafe { c.Suggest(PCWSTR(word_w.as_ptr())) }
			.map(|list| unsafe { strings(list, MAX_SUGGESTIONS) })
			.unwrap_or_default()
	})
	.unwrap_or_default()
}

fn replace(handle: HWND, from: u32, to: u32, with: &str) {
	let w = wide(with);
	unsafe {
		SendMessageW(handle, EM_SETSEL, Some(WPARAM(from as usize)), Some(LPARAM(to as isize)));
		SendMessageW(handle, EM_REPLACESEL, Some(WPARAM(1)), Some(LPARAM(w.as_ptr() as isize)));
	}
}

fn context_menu(field: &TextCtrl, screen: Option<Point>) -> bool {
	if !is_enabled() {
		return false;
	}
	let handle = hwnd(field);
	let index = screen.map_or_else(
		|| selection(handle).0,
		|p| {
			let c = field.screen_to_client(p);
			let lp = ((c.y as u32 & 0xFFFF) << 16) | (c.x as u32 & 0xFFFF);
			(unsafe { SendMessageW(handle, EM_CHARFROMPOS, None, Some(LPARAM(lp as isize))) }.0 & 0xFFFF) as u32
		},
	);
	let Some(mistake) = mistake_at(handle, index) else { return false };
	show_fixes(field, mistake, screen);
	true
}

fn show_fixes(field: &TextCtrl, mistake: Mistake, screen: Option<Point>) {
	let handle = hwnd(field);
	let all = text(handle);
	let (from, to) = (mistake.start, mistake.start + mistake.len);
	let word = String::from_utf16_lossy(&all[from as usize..to as usize]);
	let mut fixes: Vec<String> = Vec::new();
	if !mistake.repeated {
		fixes.extend(mistake.replacement.clone());
		for s in suggestions(&word) {
			if !fixes.contains(&s) {
				fixes.push(s);
			}
		}
		fixes.truncate(MAX_SUGGESTIONS);
	}
	let mut builder = Menu::builder();
	if mistake.repeated {
		builder = builder.append_item(MENU_ID_DELETE_REPEAT, "&Delete Repeated Word", "Delete the word said twice");
	} else if fixes.is_empty() {
		builder = builder.append_item(MENU_ID_SUGGESTION, "(No spelling suggestions)", "");
	}
	for (i, fix) in fixes.iter().enumerate() {
		builder = builder.append_item(MENU_ID_SUGGESTION + i as i32, fix, "Use this spelling");
	}
	builder = builder.append_separator();
	if !mistake.repeated {
		builder = builder
			.append_item(MENU_ID_ADD, &format!("&Add \"{word}\" to Dictionary"), "Never mark this word again")
			.append_item(MENU_ID_IGNORE, &format!("&Ignore \"{word}\""), "Stop marking this word until Fedra restarts");
	}
	let mut menu = builder
		.append_separator()
		.append_item(MENU_ID_UNDO, "&Undo", "")
		.append_item(MENU_ID_CUT, "Cu&t", "")
		.append_item(MENU_ID_COPY, "&Copy", "")
		.append_item(MENU_ID_PASTE, "&Paste", "")
		.append_item(MENU_ID_DELETE, "D&elete", "")
		.append_item(MENU_ID_SELECT_ALL, "Select A&ll", "")
		.build();
	if fixes.is_empty() && !mistake.repeated {
		menu.enable_item(MENU_ID_SUGGESTION, false);
	}
	menu.on_selected(move |event| {
		let id = event.get_id();
		let send = |msg: u32| unsafe {
			SendMessageW(handle, msg, None, None);
		};
		match id {
			MENU_ID_DELETE_REPEAT => {
				let all = text(handle);
				let mut start = from as usize;
				while start > 0 && all[start - 1] == b' ' as u16 {
					start -= 1;
				}
				replace(handle, start as u32, to, "");
			}
			MENU_ID_ADD => {
				let w = wide(&word);
				with_checker(|c| unsafe { c.Add(PCWSTR(w.as_ptr())) });
				recheck(handle);
			}
			MENU_ID_IGNORE => {
				let w = wide(&word);
				with_checker(|c| unsafe { c.Ignore(PCWSTR(w.as_ptr())) });
				recheck(handle);
			}
			MENU_ID_UNDO => send(WM_UNDO),
			MENU_ID_CUT => send(WM_CUT),
			MENU_ID_COPY => send(WM_COPY),
			MENU_ID_PASTE => send(WM_PASTE),
			MENU_ID_DELETE => send(WM_CLEAR),
			MENU_ID_SELECT_ALL => unsafe {
				SendMessageW(handle, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
			},
			_ => {
				if let Some(fix) = (id - MENU_ID_SUGGESTION).try_into().ok().and_then(|i: usize| fixes.get(i)) {
					replace(handle, from, to, fix);
				}
			}
		}
	});
	let at = screen.or_else(|| pos_from_char(handle, from).map(|(x, y)| field.client_to_screen(Point::new(x, y + 16))));
	field.popup_menu(&mut menu, at);
	menu.destroy_menu();
}

/// Selects the next misspelled word after the caret in `field` (wrapping to the first) and offers
/// its fixes. Says so when there's nothing to select or spell checking is off. F7 runs this in
/// every watched box.
pub fn next_error(field: &TextCtrl) {
	let handle = hwnd(field);
	let say = if is_enabled() {
		recheck(handle);
		let (_, caret) = selection(handle);
		let next = with_entry(handle, |e| {
			e.mistakes.iter().find(|m| m.start >= caret).or_else(|| e.mistakes.first()).cloned()
		})
		.flatten();
		match next {
			Some(mistake) => {
				if !field.has_focus() {
					field.set_focus();
				}
				unsafe {
					SendMessageW(
						handle,
						EM_SETSEL,
						Some(WPARAM(mistake.start as usize)),
						Some(LPARAM((mistake.start + mistake.len) as isize)),
					);
					SendMessageW(handle, EM_SCROLLCARET, None, None);
				}
				show_fixes(field, mistake, None);
				None
			}
			None => Some("No spelling errors"),
		}
	} else {
		Some("Spell checking is off. Turn it on in Options, on the General tab.")
	};
	if let Some(say) = say {
		announce_via(handle, say);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn w(s: &str) -> Vec<u16> {
		s.encode_utf16().collect()
	}

	#[test]
	fn links_mentions_and_emails_are_not_checked() {
		let t = w("see https://exampel.com and @Aryn or me@mial.com but wrod");
		let at = |needle: &str| String::from_utf16_lossy(&t).find(needle).unwrap();
		assert!(in_skipped_token(&t, at("exampel"), 7));
		assert!(in_skipped_token(&t, at("Aryn"), 4));
		assert!(in_skipped_token(&t, at("mial"), 4));
		assert!(!in_skipped_token(&t, at("wrod"), 4));
	}

	#[test]
	fn a_word_is_said_once_its_end_is_typed() {
		let mistakes = [Mistake { start: 3, len: 4, repeated: false, replacement: None }];
		let t = w("hi wrod ");
		assert_eq!(just_finished(&mistakes, &t, 8).as_deref(), Some("wrod"));
		let t = w("hi wrod,");
		assert_eq!(just_finished(&mistakes, &t, 8).as_deref(), Some("wrod"));
		assert_eq!(just_finished(&mistakes, &w("hi wrod"), 7), None);
		assert_eq!(just_finished(&mistakes, &w("hi wrod'"), 8), None);
	}

	#[test]
	fn the_language_falls_back_to_its_family_then_us_english() {
		let supported: Vec<String> = ["en-GB", "en-US", "fr-FR"].into_iter().map(str::to_string).collect();
		assert_eq!(best_match("fr-FR", &supported).as_deref(), Some("fr-FR"));
		assert_eq!(best_match("en-IN", &supported).as_deref(), Some("en-GB"));
		assert_eq!(best_match("hi-IN", &supported).as_deref(), Some("en-US"));
		assert_eq!(best_match("", &[]), None);
	}
}
