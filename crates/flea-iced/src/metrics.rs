// The window's box metrics and the few fixed intervals the shell reads, out of main.rs whole.
use std::time::Duration;

// ui/Theme.qml's measured box metrics at Omarchy's default text size: a 37 px row, read off the IPC
// seam as Theme.rowHeight, around a bodySmall of 12 px drawn at a 1.8 line box.
pub(crate) const TEXT_PX: f32 = 13.0;
pub(crate) const ROW_H: f32 = 37.0;
pub(crate) const PAD_X: f32 = 14.0;
// Theme.iconSize, the row's own text line box: rowHeight less twice rowPaddingY, 23 px here.
pub(crate) const ICON_SLOT: f32 = 23.0;
// Theme.markSize, bodySmall * 1.45; Glyph.qml draws the mark at the smaller of this and the slot.
pub(crate) const MARK: f32 = 12.0 * 1.45;
// Decoded at twice the slot so a 1.25 or 2x output scale is not drawn soft; still about 9 kB a row
// where the 256 px cache PNG would be 256 kB, the saving Image.sourceSize bought in QML.
pub(crate) const THUMB_PX: u32 = 46;
// Decoded thumbnails kept beyond the pane's own 240-row map, oldest out first.
pub(crate) const DECODED_CAP: usize = 480;
pub(crate) const SIZE_W: f32 = 90.0;
pub(crate) const DATE_W: f32 = 140.0;
// Theme.column's other widths at this size: Mode's nine characters, Kind's 120 px base at bodySmall
// 12 scaled to 13, and the name's floor of twenty characters, in the face the rows draw with.
pub(crate) const MODE_W: f32 = 80.0;
pub(crate) const KIND_W: f32 = 130.0;
pub(crate) const NAME_MIN: f32 = 160.0;
// Theme.column.items: six digits, a million children, which no directory here has.
pub(crate) const ITEMS_W: f32 = 52.0;
// A pane's listing scrollable, one id per side, so a scroll lands on the pane it was meant for.
pub(crate) fn list_id(side: u8) -> iced::widget::Id {
    iced::widget::Id::from(format!("listing-{side}"))
}
pub(crate) const RENAME_ID: &str = "rename";
// The second press of a double click lands inside this, iced's own mouse::click interval.
pub(crate) const DOUBLE_TAP: Duration = Duration::from_millis(300);
// Longer than the backend's own 25 s drain limit; see ui/Backend.qml quitDeadline.
pub(crate) const QUIT_DEADLINE: Duration = Duration::from_secs(30);
