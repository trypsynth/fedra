#![windows_subsystem = "windows"]

use wxdragon::prelude::*;

fn main() {
	let _ = wxdragon::main(|_| {
		let frame = Frame::builder().with_title("Fedra").with_size(Size::new(800, 600)).build();
		let panel = Panel::builder(&frame).build();
		let sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let timelines_label = StaticText::builder(&panel).with_label("Timelines").build();
		let timelines = ListBox::builder(&panel)
			.with_choices(vec![
				"Home".to_string(),
				"Local".to_string(),
				"Federated".to_string(),
			])
			.build();
		let timeline_content = ListBox::builder(&panel).build();
		let timelines_sizer = BoxSizer::builder(Orientation::Vertical).build();
		timelines_sizer.add(&timelines_label, 0, SizerFlag::All, 8);
		timelines_sizer.add(
			&timelines,
			1,
			SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
			8,
		);
		sizer.add_sizer(&timelines_sizer, 1, SizerFlag::Expand, 0);
		sizer.add(&timeline_content, 3, SizerFlag::Expand | SizerFlag::All, 8);
		panel.set_sizer(sizer, true);
		let frame_sizer = BoxSizer::builder(Orientation::Vertical).build();
		frame_sizer.add(&panel, 1, SizerFlag::Expand | SizerFlag::All, 0);
		frame.set_sizer(frame_sizer, true);
		frame.show(true);
		frame.centre();
	});
}
