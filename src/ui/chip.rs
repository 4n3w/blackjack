//! Gambling chips and the spot on the felt where they go.
//!
//! A chip is drawn as a five-cell disc: a full-width middle row carrying the
//! denomination, capped above and below by three cells of half-block, which
//! rounds the corners off enough to read as round rather than square.
//!
//! ```text
//!  ▄▄▄
//! █ 25█
//!  ▀▀▀
//! ```

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::ui::theme;

pub const CHIP_WIDTH: u16 = 5;
pub const CHIP_HEIGHT: u16 = 3;
/// Felt left between one stack and the next.
const CHIP_GAP: u16 = 1;
/// Rows the betting spot's outline adds around the chips.
pub const SPOT_HEIGHT: u16 = CHIP_HEIGHT + 2;
/// Cells the outline adds around the chips.
const SPOT_PAD: u16 = 6;

/// A chip denomination, in the usual casino colours.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Denomination {
    pub value: u32,
    face: Color,
    ink: Color,
}

/// Largest first, which is also the order a stack is broken down in.
pub const DENOMINATIONS: [Denomination; 5] = [
    Denomination {
        value: 500,
        face: Color::Rgb(98, 54, 138),
        ink: Color::Rgb(240, 236, 245),
    },
    Denomination {
        value: 100,
        face: Color::Rgb(32, 32, 36),
        ink: Color::Rgb(236, 236, 232),
    },
    Denomination {
        value: 25,
        face: Color::Rgb(32, 120, 62),
        ink: Color::Rgb(238, 246, 238),
    },
    Denomination {
        value: 5,
        face: Color::Rgb(196, 48, 43),
        ink: Color::Rgb(250, 240, 238),
    },
    Denomination {
        value: 1,
        face: Color::Rgb(240, 240, 235),
        ink: Color::Rgb(36, 36, 40),
    },
];

/// A pile of one denomination, the way a dealer would stack it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stack {
    pub denomination: Denomination,
    pub count: u32,
}

impl Stack {
    /// A stack of more than one is labelled with how many, since only the top
    /// chip of a real pile is visible anyway.
    fn multiplier(self) -> Option<String> {
        (self.count > 1).then(|| format!("×{}", self.count))
    }

    fn width(self) -> u16 {
        // The multiplier sits in the felt to the right of the chip.
        CHIP_WIDTH
            + self
                .multiplier()
                .map_or(0, |m| 1 + m.chars().count() as u16)
    }
}

/// Break an amount into stacks of chips, largest denomination first.
///
/// Greedy, which for these denominations is also the fewest chips. Grouping
/// equal chips keeps the spot a bounded width — there are only five
/// denominations, so a bet can never need more than five stacks however
/// large it gets.
pub fn breakdown(mut amount: u32) -> Vec<Stack> {
    let mut stacks = Vec::new();
    for denomination in DENOMINATIONS {
        let count = amount / denomination.value;
        if count > 0 {
            amount -= count * denomination.value;
            stacks.push(Stack {
                denomination,
                count,
            });
        }
    }
    stacks
}

/// The width a row of stacks occupies, gaps included.
fn stacks_width(stacks: &[Stack]) -> u16 {
    if stacks.is_empty() {
        return 0;
    }
    let chips: u16 = stacks.iter().map(|s| s.width()).sum();
    chips + CHIP_GAP * (stacks.len() as u16 - 1)
}

/// A single chip.
#[derive(Clone, Copy, Debug)]
pub struct Chip {
    denomination: Denomination,
}

impl Chip {
    pub const fn new(denomination: Denomination) -> Self {
        Self { denomination }
    }
}

impl Widget for Chip {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < CHIP_WIDTH || area.height < CHIP_HEIGHT {
            return;
        }
        let face = self.denomination.face;
        let cap = Style::new().fg(face).bg(theme::FELT);
        let body = Style::new()
            .fg(self.denomination.ink)
            .bg(face)
            .add_modifier(Modifier::BOLD);

        // The caps are three cells of half-block, inset by one, which knocks
        // the corners off the disc.
        for dx in 1..CHIP_WIDTH - 1 {
            set(buf, area.x + dx, area.y, '▄', cap);
            set(buf, area.x + dx, area.y + 2, '▀', cap);
        }

        let label = self.denomination.value.to_string();
        let pad = (CHIP_WIDTH as usize).saturating_sub(label.len()) / 2;
        let mut face_row = " ".repeat(pad);
        face_row.push_str(&label);
        while face_row.len() < CHIP_WIDTH as usize {
            face_row.push(' ');
        }
        buf.set_string(area.x, area.y + 1, &face_row, body);
    }
}

/// The outline painted on the felt that the bet sits in.
///
/// A flattened diamond: wide and shallow, the way a real betting spot is, so
/// it can hold a row of chips without eating the table's vertical space.
#[derive(Clone, Copy, Debug)]
pub struct BettingSpot {
    amount: u32,
    /// Draw the outline, or just the chips when the table is short of rows.
    outlined: bool,
}

impl BettingSpot {
    pub const fn new(amount: u32, outlined: bool) -> Self {
        Self { amount, outlined }
    }

    /// Total width, so the caller can centre it.
    pub fn width(self) -> u16 {
        let chips = stacks_width(&breakdown(self.amount));
        if self.outlined {
            chips + SPOT_PAD
        } else {
            chips
        }
    }
}

impl Widget for BettingSpot {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let stacks = breakdown(self.amount);
        if stacks.is_empty() || area.width < CHIP_WIDTH || area.height < CHIP_HEIGHT {
            return;
        }
        let chips_w = stacks_width(&stacks);
        let total = self.width().min(area.width);
        let left = area.x + area.width.saturating_sub(total) / 2;

        // With the outline the chips sit one row down, inside it.
        let (chip_y, mut x) = if self.outlined && area.height >= SPOT_HEIGHT {
            self.render_outline(
                Rect::new(left, area.y, total, SPOT_HEIGHT),
                buf,
                self.amount,
            );
            (area.y + 1, left + SPOT_PAD / 2)
        } else {
            (area.y, area.x + area.width.saturating_sub(chips_w) / 2)
        };

        for stack in stacks {
            if x + CHIP_WIDTH > area.right() {
                break;
            }
            Chip::new(stack.denomination)
                .render(Rect::new(x, chip_y, CHIP_WIDTH, CHIP_HEIGHT), buf);
            if let Some(m) = stack.multiplier() {
                buf.set_string(
                    x + CHIP_WIDTH + 1,
                    chip_y + 1,
                    &m,
                    Style::new().fg(theme::TEXT_DIM).bg(theme::FELT),
                );
            }
            x += stack.width() + CHIP_GAP;
        }
    }
}

impl BettingSpot {
    /// The diamond itself. Five rows: a flat top and bottom joined by four
    /// diagonals, with the amount engraved along the top edge.
    fn render_outline(self, area: Rect, buf: &mut Buffer, amount: u32) {
        let style = Style::new().fg(theme::SPOT).bg(theme::FELT);
        let t = area.width;
        if t < SPOT_PAD + 1 {
            return;
        }
        let (top, bottom) = (area.y, area.y + SPOT_HEIGHT - 1);

        // Widest row, level with the middle of the chips.
        set(buf, area.x, area.y + 2, '▕', style);
        set(buf, area.x + t - 1, area.y + 2, '▏', style);

        // The four diagonals, one cell in from the widest row.
        set(buf, area.x + 1, area.y + 1, '╱', style);
        set(buf, area.x + t - 2, area.y + 1, '╲', style);
        set(buf, area.x + 1, area.y + 3, '╲', style);
        set(buf, area.x + t - 2, area.y + 3, '╱', style);

        // Flat top and bottom edges, two cells in.
        set(buf, area.x + 2, top, '╱', style);
        set(buf, area.x + t - 3, top, '╲', style);
        set(buf, area.x + 2, bottom, '╲', style);
        set(buf, area.x + t - 3, bottom, '╱', style);

        let edge = t.saturating_sub(SPOT_PAD);
        for dx in 0..edge {
            set(buf, area.x + 3 + dx, top, '▔', style);
            set(buf, area.x + 3 + dx, bottom, '▁', style);
        }

        // The wager, painted along the top edge like a table limit sign.
        let label = format!("${amount}");
        if (label.len() as u16) + 2 <= edge {
            let x = area.x + 3 + (edge - label.len() as u16) / 2;
            buf.set_string(x, top, &label, style.add_modifier(Modifier::BOLD));
        }
    }
}

/// Write one character, ignoring positions outside the buffer.
fn set(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch).set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn render<W: Widget>(w: W, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(w, frame.area()))
            .unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn a_single_chip() {
        insta::assert_snapshot!(render(Chip::new(DENOMINATIONS[2]), CHIP_WIDTH, CHIP_HEIGHT));
    }

    #[test]
    fn a_hundred_dollar_chip_is_three_digits_wide() {
        insta::assert_snapshot!(render(Chip::new(DENOMINATIONS[1]), CHIP_WIDTH, CHIP_HEIGHT));
    }

    #[test]
    fn betting_spot_with_one_chip() {
        insta::assert_snapshot!(render(BettingSpot::new(25, true), 30, SPOT_HEIGHT));
    }

    #[test]
    fn betting_spot_with_a_mixed_stack() {
        insta::assert_snapshot!(render(BettingSpot::new(135, true), 40, SPOT_HEIGHT));
    }

    /// A big wager groups into tall stacks rather than a wide sprawl.
    #[test]
    fn a_large_wager_stacks_rather_than_sprawls() {
        insta::assert_snapshot!(render(BettingSpot::new(4999, true), 74, SPOT_HEIGHT));
    }

    #[test]
    fn bare_chips_when_there_is_no_room_for_the_outline() {
        insta::assert_snapshot!(render(BettingSpot::new(135, false), 40, CHIP_HEIGHT));
    }

    /// The greedy split must come to exactly the amount asked for.
    #[test]
    fn breakdown_is_exact() {
        for amount in 0..2000u32 {
            let total: u32 = breakdown(amount)
                .iter()
                .map(|s| s.denomination.value * s.count)
                .sum();
            assert_eq!(total, amount, "{amount} did not break down exactly");
        }
    }

    /// One stack per denomination, so the spot has a hard width ceiling no
    /// matter how much is riding on it.
    #[test]
    fn stacks_are_grouped_and_bounded() {
        for amount in [5u32, 135, 495, 4_999, 1_000_000] {
            let stacks = breakdown(amount);
            assert!(
                stacks.len() <= DENOMINATIONS.len(),
                "{amount} split too far"
            );
            let mut seen: Vec<u32> = stacks.iter().map(|s| s.denomination.value).collect();
            let before = seen.len();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), before, "{amount} repeated a denomination");
        }
    }

    /// However big the wager, the spot must still fit across a normal table.
    #[test]
    fn even_an_absurd_bet_fits_on_the_table() {
        for amount in [500u32, 5_000, 99_999, u32::MAX] {
            let w = BettingSpot::new(amount, true).width();
            assert!(w <= 74, "${amount} wanted {w} cells");
        }
    }

    /// It must also be the shortest stack, or the dealer is paying out in
    /// singles when they could hand over one chip.
    #[test]
    fn breakdown_uses_the_fewest_chips() {
        // With these denominations greedy is optimal; check it against a
        // brute-force minimum for a range of amounts.
        const LIMIT: usize = 600;
        let values: Vec<usize> = DENOMINATIONS.iter().map(|d| d.value as usize).collect();

        // best[n] = fewest chips that make n, built up from zero.
        let mut best: Vec<usize> = Vec::with_capacity(LIMIT);
        best.push(0);
        for amount in 1..LIMIT {
            let fewest = values
                .iter()
                .filter(|&&v| v <= amount)
                .map(|&v| best[amount - v] + 1)
                .min()
                .unwrap_or(usize::MAX);
            best.push(fewest);
        }

        for (amount, &fewest) in best.iter().enumerate().skip(1) {
            let chips: u32 = breakdown(amount as u32).iter().map(|s| s.count).sum();
            assert_eq!(
                chips as usize, fewest,
                "{amount} took more chips than it needed"
            );
        }
    }

    #[test]
    fn no_chips_for_no_bet() {
        assert!(breakdown(0).is_empty());
        // Every cell must still be blank: no outline hanging in mid-air.
        let mut terminal = Terminal::new(TestBackend::new(20, SPOT_HEIGHT)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(BettingSpot::new(0, true), frame.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        assert!(
            buf.content().iter().all(|c| c.symbol() == " "),
            "an empty spot should draw nothing at all"
        );
    }

    #[test]
    fn a_cramped_area_does_not_panic() {
        for (w, h) in [(1, 1), (4, 2), (5, 3), (11, 5), (80, 5)] {
            let _ = render(BettingSpot::new(135, true), w, h);
        }
    }
}
