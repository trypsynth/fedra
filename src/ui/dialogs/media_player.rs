use std::{cell::Cell, rc::Rc};

use wxdragon::{prelude::*, widgets::media_ctrl::SeekMode};

pub fn show_media_player(_parent: &dyn WxWidget, url: String, _access_token: Option<String>) {
	let frame = Frame::builder().with_title("Media Player").with_size(Size::new(800, 600)).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	let media_ctrl = wxdragon::widgets::MediaCtrl::builder(&frame).build();
	media_ctrl.show_player_controls(wxdragon::widgets::media_ctrl::MediaCtrlPlayerControls::None);
	sizer.add(&media_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 10);
	frame.set_sizer(sizer, true);
	let is_playing = Rc::new(Cell::new(false));
	const ID_PLAY_PAUSE: i32 = 10001;
	const ID_SEEK_BACK: i32 = 10002;
	const ID_SEEK_FWD: i32 = 10003;
	const ID_VOL_UP: i32 = 10004;
	const ID_VOL_DOWN: i32 = 10005;
	const ID_CLOSE: i32 = 10006;
	let menu = Menu::builder()
		.append_item(ID_PLAY_PAUSE, "Play/Pause\tSpace", "Play or pause the media")
		.append_item(ID_SEEK_BACK, "Seek Backward\tLeft", "Seek backward 10 seconds")
		.append_item(ID_SEEK_FWD, "Seek Forward\tRight", "Seek forward 10 seconds")
		.append_item(ID_VOL_UP, "Volume Up\tUp", "Increase volume")
		.append_item(ID_VOL_DOWN, "Volume Down\tDown", "Decrease volume")
		.append_separator()
		.append_item(ID_CLOSE, "Close\tEscape", "Close media player")
		.build();
	let menu_bar = MenuBar::builder().append(menu, "&Playback").build();
	frame.set_menu_bar(menu_bar);
	frame.on_menu_selected({
		let mc = media_ctrl.clone();
		let is_playing = is_playing.clone();
		let frm = frame.clone();
		move |event| match event.get_id() {
			ID_PLAY_PAUSE => {
				if is_playing.get() {
					mc.pause();
					is_playing.set(false);
				} else {
					mc.play();
					is_playing.set(true);
				}
			}
			ID_SEEK_BACK => {
				mc.seek(-10000, SeekMode::FromCurrent);
			}
			ID_SEEK_FWD => {
				mc.seek(10000, SeekMode::FromCurrent);
			}
			ID_VOL_UP => {
				let v = (mc.get_volume() + 0.1).min(1.0);
				mc.set_volume(v);
			}
			ID_VOL_DOWN => {
				let v = (mc.get_volume() - 0.1).max(0.0);
				mc.set_volume(v);
			}
			ID_CLOSE => {
				frm.close(true);
			}
			_ => {}
		}
	});
	media_ctrl.load_uri(&url);
	media_ctrl.set_focus();
	frame.show(true);
}
