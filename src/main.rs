#![windows_subsystem = "windows"]

use wxdragon::prelude::*;

fn main() {
	let _ = wxdragon::main(|_| {
		let frame = Frame::builder().with_title("Fedra").with_size(Size::new(800, 600)).build();
		let sizer = BoxSizer::builder(Orientation::Vertical).build();
		let button = Button::builder(&frame).with_label("Click me").build();
		sizer.add(&button, 1, SizerFlag::AlignCenterHorizontal | SizerFlag::AlignCenterVertical, 0);
		frame.set_sizer(sizer, true);
		frame.show(true);
		frame.centre();
	});
}
