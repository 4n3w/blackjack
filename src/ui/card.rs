//! Drawing a single card as a small bordered window.
//!
//! This is the part that started life in Turbo C++: a rectangle, a border, a
//! rank in two corners and a pip pattern in the middle. The pip layouts are a
//! hardcoded table because that is genuinely how playing cards work — there is
//! no formula, just convention.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Widget};

use crate::cards::{Card, Rank};
use crate::ui::theme;

/// Cards are 9x9 cells. The border eats one cell on each side, leaving a 7x7
/// interior: one row for each corner index and a 5-row pip field between them.
pub const CARD_WIDTH: u16 = 9;
pub const CARD_HEIGHT: u16 = 9;

/// How far apart to place cards when fanning a hand. Enough to show the corner
/// index of the card underneath, which is the whole point of a fan.
pub const FAN_STRIDE: u16 = 4;
/// The tightest a fan may be squeezed: a border plus two cells, which is just
/// enough for a two-character index like `10`.
pub const MIN_FAN_STRIDE: u16 = 3;

/// The width a fan of `n` cards occupies at the given stride.
pub const fn fan_width(stride: u16, n: u16) -> u16 {
    if n == 0 {
        0
    } else {
        CARD_WIDTH + stride * (n - 1)
    }
}

/// Pick a stride that keeps `n` cards inside `available` cells.
///
/// A long hand, or four hands after splitting, will not fit at the usual
/// spacing. Tightening the overlap degrades far better than letting cards run
/// off the edge of their column.
pub fn fan_stride(available: u16, n: u16) -> u16 {
    if n <= 1 {
        return FAN_STRIDE;
    }
    let fit = available.saturating_sub(CARD_WIDTH) / (n - 1);
    fit.clamp(MIN_FAN_STRIDE, FAN_STRIDE)
}

/// Columns of the 3-wide pip grid.
const L: u8 = 0;
const C: u8 = 1;
const R: u8 = 2;

/// Pip positions as `(column, row)` on a 3x5 grid.
///
/// These follow the standard layouts you would find on a real deck: pips march
/// down the outer columns, and the centre column fills in the gaps. Face cards
/// get no pips — they are drawn as a monogram instead.
/// Kept hand-formatted: each line is one row of the grid, so the table reads
/// like the card it draws.
#[rustfmt::skip]
const fn pip_layout(rank: Rank) -> &'static [(u8, u8)] {
    match rank {
        Rank::Ace   => &[                          (C, 2)                        ],
        Rank::Two   => &[(C, 0),                                           (C, 4)],
        Rank::Three => &[(C, 0),                   (C, 2),                 (C, 4)],
        Rank::Four  => &[(L, 0), (R, 0),                           (L, 4), (R, 4)],
        Rank::Five  => &[(L, 0), (R, 0),           (C, 2),         (L, 4), (R, 4)],
        Rank::Six   => &[(L, 0), (R, 0),   (L, 2), (R, 2),         (L, 4), (R, 4)],
        Rank::Seven => &[(L, 0), (R, 0),   (C, 1), (L, 2), (R, 2), (L, 4), (R, 4)],
        Rank::Eight => &[(L, 0), (R, 0),   (C, 1), (L, 2), (R, 2),
                         (C, 3),                                   (L, 4), (R, 4)],
        Rank::Nine  => &[(L, 0), (R, 0),   (L, 1), (R, 1), (C, 2),
                         (L, 3), (R, 3),                           (L, 4), (R, 4)],
        Rank::Ten   => &[(L, 0), (R, 0),   (L, 1), (C, 1), (R, 1),
                         (L, 3), (C, 3), (R, 3),                   (L, 4), (R, 4)],
        Rank::Jack | Rank::Queen | Rank::King => &[],
    }
}

/// A card, face up or face down.
#[derive(Clone, Copy, Debug)]
pub struct CardWidget {
    card: Card,
    face_up: bool,
}

impl CardWidget {
    pub const fn face_up(card: Card) -> Self {
        Self {
            card,
            face_up: true,
        }
    }

    pub const fn face_down(card: Card) -> Self {
        Self {
            card,
            face_up: false,
        }
    }
}

impl Widget for CardWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.face_up {
            self.render_face(area, buf);
        } else {
            render_back(area, buf);
        }
    }
}

impl CardWidget {
    fn render_face(self, area: Rect, buf: &mut Buffer) {
        let ink = if self.card.suit.is_red() {
            theme::INK_RED
        } else {
            theme::INK
        };
        let style = Style::new().bg(theme::CARD_STOCK).fg(ink);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .style(style);
        let inner = block.inner(area);
        blank(buf, area, style);
        block.render(area, buf);

        if inner.is_empty() {
            return;
        }

        let label = self.card.rank.label();
        let glyph = self.card.suit.glyph();
        let bold = style.add_modifier(Modifier::BOLD);

        // Index in the top-left and, upside-down convention aside, bottom-right.
        buf.set_string(inner.x, inner.y, label, bold);
        let label_w = label.len() as u16;
        if inner.width >= label_w && inner.height >= 2 {
            let x = inner.right() - label_w;
            buf.set_string(x, inner.bottom() - 1, label, bold);
        }

        // A single small pip beside each index, the way real cards do it.
        if inner.width > label_w {
            set_char(buf, inner.x + label_w, inner.y, glyph, style);
            set_char(
                buf,
                inner.right() - label_w - 1,
                inner.bottom() - 1,
                glyph,
                style,
            );
        }

        if self.card.rank.is_face() {
            self.render_monogram(inner, buf, style);
        } else {
            self.render_pips(inner, buf, style);
        }
    }

    fn render_pips(self, inner: Rect, buf: &mut Buffer, style: Style) {
        let glyph = self.card.suit.glyph();
        // The three grid columns are inset by one cell, the way a real card
        // keeps a margin of white around its pips. It also matters for fanned
        // hands: it stops the outer pips from stacking directly under the
        // corner index in the narrow sliver of a covered card.
        let cols = [inner.x + 1, inner.x + inner.width / 2, inner.right() - 2];
        for &(col, row) in pip_layout(self.card.rank) {
            let x = cols[col as usize];
            let y = inner.y + 1 + u16::from(row);
            set_char(buf, x, y, glyph, style);
        }
    }

    /// Face cards get the rank letter framed by two pips.
    fn render_monogram(self, inner: Rect, buf: &mut Buffer, style: Style) {
        let glyph = self.card.suit.glyph();
        let cx = inner.x + inner.width / 2;
        let cy = inner.y + inner.height / 2;
        let bold = style.add_modifier(Modifier::BOLD);
        set_char(buf, cx, cy - 1, glyph, style);
        set_char(
            buf,
            cx,
            cy,
            self.card.rank.label().chars().next().unwrap_or('?'),
            bold,
        );
        set_char(buf, cx, cy + 1, glyph, style);
    }
}

fn render_back(area: Rect, buf: &mut Buffer) {
    let style = Style::new().bg(theme::BACK).fg(theme::BACK_PATTERN);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .style(style);
    let inner = block.inner(area);
    blank(buf, area, style);
    block.render(area, buf);

    for y in inner.top()..inner.bottom() {
        for x in inner.left()..inner.right() {
            // A woven diagonal, which reads as a card back at this size.
            let ch = if (x + y) % 2 == 0 { '╱' } else { '╲' };
            set_char(buf, x, y, ch, style);
        }
    }
}

/// Wipe an area to blank cells in the given style.
///
/// `Block` sets the *style* of every cell it covers but leaves their contents
/// alone, so without this a card laid over another in a fan would let the pips
/// and indices of the card underneath show through its face.
fn blank(buf: &mut Buffer, area: Rect, style: Style) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            set_char(buf, x, y, ' ', style);
        }
    }
}

/// Write one character, ignoring positions that fall outside the buffer.
fn set_char(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch).set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::cards::{Rank, Suit};

    /// Render one card on its own and hand the buffer to insta.
    fn render(widget: CardWidget) -> String {
        let mut terminal = Terminal::new(TestBackend::new(CARD_WIDTH, CARD_HEIGHT)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(widget, frame.area()))
            .unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn eight_of_diamonds() {
        insta::assert_snapshot!(render(CardWidget::face_up(Card::new(
            Rank::Eight,
            Suit::Diamonds
        ))));
    }

    #[test]
    fn ten_of_clubs() {
        insta::assert_snapshot!(render(CardWidget::face_up(Card::new(
            Rank::Ten,
            Suit::Clubs
        ))));
    }

    #[test]
    fn ace_of_spades() {
        insta::assert_snapshot!(render(CardWidget::face_up(Card::new(
            Rank::Ace,
            Suit::Spades
        ))));
    }

    #[test]
    fn king_of_hearts() {
        insta::assert_snapshot!(render(CardWidget::face_up(Card::new(
            Rank::King,
            Suit::Hearts
        ))));
    }

    #[test]
    fn hole_card() {
        insta::assert_snapshot!(render(CardWidget::face_down(Card::new(
            Rank::Two,
            Suit::Spades
        ))));
    }

    /// Every rank must draw exactly as many pips as its name promises.
    #[test]
    fn pip_counts_match_rank() {
        for rank in Rank::ALL {
            if rank.is_face() {
                assert!(pip_layout(rank).is_empty(), "{rank:?} should have no pips");
            } else {
                assert_eq!(
                    pip_layout(rank).len(),
                    usize::from(rank.base_value()),
                    "{rank:?} drew the wrong number of pips"
                );
            }
        }
    }

    /// Two pips must never land on the same cell.
    #[test]
    fn pip_positions_are_distinct() {
        for rank in Rank::ALL {
            let mut seen = pip_layout(rank).to_vec();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(
                seen.len(),
                pip_layout(rank).len(),
                "{rank:?} has overlapping pips"
            );
        }
    }
}
