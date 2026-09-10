//! The colour palette, in one place so the whole table stays consistent.

use ratatui::style::Color;

/// Table felt.
pub const FELT: Color = Color::Rgb(11, 79, 51);
/// Gold trim on the table edge and headings.
pub const TRIM: Color = Color::Rgb(212, 175, 55);

/// Card stock — warm off-white rather than pure white, which glares.
pub const CARD_STOCK: Color = Color::Rgb(246, 243, 235);
/// Black pips and indices.
pub const INK: Color = Color::Rgb(28, 28, 32);
/// Red pips and indices.
pub const INK_RED: Color = Color::Rgb(178, 34, 41);

/// The back of a card.
pub const BACK: Color = Color::Rgb(24, 42, 96);
pub const BACK_PATTERN: Color = Color::Rgb(58, 84, 158);

pub const TEXT: Color = Color::Rgb(232, 232, 228);
pub const TEXT_DIM: Color = Color::Rgb(150, 168, 156);

/// The outline painted on the felt where the bet goes.
pub const SPOT: Color = Color::Rgb(150, 128, 72);

/// A winning hand.
pub const WIN: Color = Color::Rgb(122, 201, 129);
/// A losing or busted hand.
pub const LOSE: Color = Color::Rgb(224, 108, 105);
/// An action that is not available right now.
pub const DISABLED: Color = Color::Rgb(74, 106, 87);
