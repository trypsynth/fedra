//! wxWidgets virtual key codes, as `get_key_code` reports them.

#[cfg(target_os = "macos")]
pub const BACKSPACE: i32 = 8;
pub const TAB: i32 = 9;
pub const RETURN: i32 = 13;
pub const ESCAPE: i32 = 27;
pub const SPACE: i32 = 32;
#[cfg(target_os = "macos")]
pub const DELETE: i32 = 127;
pub const END: i32 = 312;
pub const HOME: i32 = 313;
pub const LEFT: i32 = 314;
pub const UP: i32 = 315;
pub const RIGHT: i32 = 316;
pub const DOWN: i32 = 317;
#[cfg(target_os = "macos")]
pub const F1: i32 = 340;
pub const PAGE_UP: i32 = 366;
pub const PAGE_DOWN: i32 = 367;
