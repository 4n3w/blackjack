//! Everything that knows about the terminal.

pub mod card;
pub mod chip;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;
use tui_big_text::{BigText, PixelSize};

use crate::app::App;
use crate::cards::Hand;
use crate::game::{Action, Game, Outcome, Phase, PlayerHand};
use card::{CARD_HEIGHT, CARD_WIDTH, CardWidget, fan_stride, fan_width};
use chip::{CHIP_HEIGHT, SPOT_HEIGHT};

/// Rows the big-text banner needs.
const BANNER_ROWS: u16 = 5;
/// A label plus the cards themselves.
const DEALER_ROWS: u16 = CARD_HEIGHT + 1;
/// As above, plus a row underneath for the active-hand marker.
const PLAYER_ROWS: u16 = CARD_HEIGHT + 2;
/// Below this the table simply will not fit.
const MIN_ROWS: u16 = DEALER_ROWS + PLAYER_ROWS + 2;
/// Show the banner only when there is room to spare for it.
const BANNER_ROWS_NEEDED: u16 = MIN_ROWS + BANNER_ROWS;
/// Left and right margin for text.
const PAD: u16 = 3;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::new().bg(theme::FELT)), area);

    if area.height < MIN_ROWS || area.width < CARD_WIDTH + PAD * 2 {
        draw_too_small(frame, area);
        return;
    }

    // The banner is the first thing to go when the window is short.
    let banner_rows = if area.height >= BANNER_ROWS_NEEDED {
        BANNER_ROWS
    } else {
        0
    };
    let [banner, dealer, gap, player, status, footer] = Layout::vertical([
        Constraint::Length(banner_rows),
        Constraint::Length(DEALER_ROWS),
        Constraint::Min(0),
        Constraint::Length(PLAYER_ROWS),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    if banner_rows > 0 {
        draw_banner(frame, banner, &app.game);
    }
    draw_dealer(frame, dealer, app);
    draw_betting_spot(frame, gap, &app.game);
    draw_player(frame, player, app);
    draw_status(frame, status, app);
    draw_footer(frame, footer, &app.game);
}

/// The bet, as chips on the felt between the dealer and the player.
///
/// This lives in whatever slack the layout has left over, so it takes the
/// painted outline when there is room and falls back to bare chips — then to
/// nothing at all — as the table gets shorter.
fn draw_betting_spot(frame: &mut Frame, area: Rect, game: &Game) {
    let amount = game.wagered();
    if amount == 0 || area.height < CHIP_HEIGHT {
        return;
    }
    let outlined = area.height >= SPOT_HEIGHT;
    let height = if outlined { SPOT_HEIGHT } else { CHIP_HEIGHT };
    // Centre it in the gap rather than letting it ride against the dealer.
    let y = area.y + (area.height - height) / 2;
    frame.render_widget(
        chip::BettingSpot::new(amount, outlined),
        Rect::new(area.x, y, area.width, height),
    );
}

fn draw_too_small(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled(
            "Make the window taller — ",
            Style::new().fg(theme::TEXT_DIM),
        ),
        Span::styled(
            format!("{MIN_ROWS} rows needed"),
            Style::new().fg(theme::TRIM),
        ),
    ]);
    let mid = Rect::new(area.x, area.y + area.height / 2, area.width, 1);
    frame.render_widget(text.centered(), mid);
}

/// The big-text line across the top: the game's name, or the result of the
/// round once the bets are paid.
fn draw_banner(frame: &mut Frame, area: Rect, game: &Game) {
    let (text, colour) = if game.phase == Phase::Settled {
        let word = game.banner();
        let colour = match word {
            "WIN" | "BLACKJACK" => theme::WIN,
            "PUSH" => theme::TEXT_DIM,
            _ => theme::LOSE,
        };
        (word, colour)
    } else {
        ("BLACKJACK", theme::TRIM)
    };

    let big = BigText::builder()
        .pixel_size(PixelSize::Quadrant)
        .style(Style::new().fg(colour).bg(theme::FELT))
        .lines(vec![Line::from(text)])
        .centered()
        .build();
    frame.render_widget(big, area);
}

fn draw_dealer(frame: &mut Frame, area: Rect, app: &App) {
    let game = &app.game;
    let [label_area, cards_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(CARD_HEIGHT)]).areas(area);

    let mut spans = vec![Span::styled(
        "DEALER",
        Style::new().fg(theme::TRIM).add_modifier(Modifier::BOLD),
    )];
    if let Some(total) = game.dealer_shown_value() {
        let text = if !game.hole_revealed {
            format!("  showing {total}")
        } else if game.dealer.is_blackjack() {
            String::from("  BLACKJACK")
        } else if game.dealer.is_bust() {
            format!("  {total} — BUST")
        } else {
            format!("  {total}")
        };
        let colour = if game.dealer.is_bust() {
            theme::LOSE
        } else {
            theme::TEXT
        };
        spans.push(Span::styled(text, Style::new().fg(colour)));
    }
    frame.render_widget(Line::from(spans), label_area.inner(Margin::new(PAD, 0)));

    // The dealer's second card is the hole card until the player is done.
    let hole = if game.hole_revealed { None } else { Some(1) };
    draw_fan(frame, cards_area, &game.dealer, hole, app.dealer_visible());
}

/// The player's side of the table. After a split there is more than one hand,
/// so the row is divided into a column per hand.
fn draw_player(frame: &mut Frame, area: Rect, app: &App) {
    let game = &app.game;
    let [label_area, cards_area, mark_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(CARD_HEIGHT),
        Constraint::Length(1),
    ])
    .areas(area);

    if game.hands.is_empty() {
        let text = Line::from(Span::styled(
            "YOU",
            Style::new().fg(theme::TRIM).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(text, label_area.inner(Margin::new(PAD, 0)));
        return;
    }

    let split = vec![Constraint::Ratio(1, game.hands.len() as u32); game.hands.len()];
    let columns = Layout::horizontal(split.clone()).split(cards_area);
    let labels = Layout::horizontal(split.clone()).split(label_area);
    let marks = Layout::horizontal(split).split(mark_area);

    for (i, h) in game.hands.iter().enumerate() {
        let active = game.phase == Phase::Player && i == game.active;
        draw_hand_label(frame, labels[i], game, h, i, active);
        draw_fan(frame, columns[i], &h.hand, None, app.player_visible(i));

        // Rule under the hand being played. With a single hand there is
        // nothing to disambiguate, so it would only be clutter.
        if active && game.hands.len() > 1 {
            let fan = fan_layout(columns[i], h.hand.cards.len() as u16);
            let rule = "─".repeat(fan.width as usize);
            frame.render_widget(
                Line::from(Span::styled(rule, Style::new().fg(theme::TRIM))),
                Rect::new(fan.left, marks[i].y, fan.width, 1),
            );
        }
    }
}

fn draw_hand_label(
    frame: &mut Frame,
    area: Rect,
    game: &Game,
    h: &PlayerHand,
    index: usize,
    active: bool,
) {
    let single = game.hands.len() == 1;
    let name = if single {
        String::from("YOU")
    } else {
        format!("HAND {}", index + 1)
    };

    let mut spans = Vec::new();
    // A caret marks whose turn it is once there is more than one hand.
    if active && !single {
        spans.push(Span::styled("▸ ", Style::new().fg(theme::TRIM)));
    }
    spans.push(Span::styled(
        name,
        Style::new()
            .fg(if active { theme::TRIM } else { theme::TEXT_DIM })
            .add_modifier(Modifier::BOLD),
    ));

    if !h.hand.cards.is_empty() {
        let v = h.hand.value();
        let total = if h.hand.is_bust() {
            format!("  {}", v.total)
        } else if v.soft {
            format!("  soft {}", v.total)
        } else {
            format!("  {}", v.total)
        };
        spans.push(Span::styled(total, Style::new().fg(theme::TEXT)));
    }
    if h.doubled {
        spans.push(Span::styled("  ×2", Style::new().fg(theme::TEXT_DIM)));
    }
    if let Some(outcome) = h.outcome {
        spans.push(Span::styled(
            format!("  {}", outcome.label()),
            Style::new()
                .fg(outcome_colour(outcome))
                .add_modifier(Modifier::BOLD),
        ));
    }

    let line = if single {
        Line::from(spans)
    } else {
        Line::from(spans).centered()
    };
    let inset = if single { PAD } else { 0 };
    frame.render_widget(line, area.inner(Margin::new(inset, 0)));
}

const fn outcome_colour(outcome: Outcome) -> Color {
    if outcome.is_win() {
        theme::WIN
    } else if matches!(outcome, Outcome::Push) {
        theme::TEXT_DIM
    } else {
        theme::LOSE
    }
}

/// Where a fan of `n` cards sits inside `area`, once it has been tightened to
/// fit and centred.
struct Fan {
    left: u16,
    stride: u16,
    width: u16,
}

fn fan_layout(area: Rect, n: u16) -> Fan {
    let stride = fan_stride(area.width, n);
    let width = fan_width(stride, n).min(area.width);
    Fan {
        // Centre on the finished hand, so cards do not shuffle sideways as
        // the rest of the deal lands.
        left: area.x + area.width.saturating_sub(width) / 2,
        stride,
        width,
    }
}

/// Draw a hand as an overlapping fan, centred in `area`.
///
/// Ratatui composites into one buffer in call order, so drawing left to right
/// with a stride narrower than a card gives the overlap for free — later cards
/// simply cover the ones already there. No z-ordering required.
fn draw_fan(frame: &mut Frame, area: Rect, hand: &Hand, face_down: Option<usize>, visible: usize) {
    let n = hand.cards.len() as u16;
    if n == 0 {
        return;
    }
    let fan = fan_layout(area, n);

    let shown = visible.min(hand.cards.len());
    for (i, &c) in hand.cards.iter().take(shown).enumerate() {
        let x = fan.left + fan.stride * i as u16;
        if x + CARD_WIDTH > area.right() {
            break;
        }
        let rect = Rect::new(x, area.y, CARD_WIDTH, CARD_HEIGHT);
        let widget = if face_down == Some(i) {
            CardWidget::face_down(c)
        } else {
            CardWidget::face_up(c)
        };
        frame.render_widget(widget, rect);
    }
}

/// What just happened on the left, and the state of the shoe on the right.
fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let game = &app.game;
    let text = if game.message.is_empty() {
        match game.phase {
            Phase::Player => String::from("Your move."),
            Phase::Dealer => String::from("Dealer draws…"),
            _ => String::new(),
        }
    } else {
        game.message.clone()
    };

    let mut right = vec![
        Span::styled(
            format!("${}", game.bankroll),
            Style::new().fg(theme::TRIM).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  shoe {}", game.shoe_len()),
            Style::new().fg(theme::TEXT_DIM),
        ),
    ];
    if app.show_count {
        let rc = game.running_count();
        // A positive count is the one worth betting into, so colour it like a
        // win and the negative like a loss.
        let colour = match rc {
            0 => theme::TEXT_DIM,
            n if n > 0 => theme::WIN,
            _ => theme::LOSE,
        };
        right.push(Span::styled(
            format!("  count {rc:+}"),
            Style::new().fg(colour),
        ));
        right.push(Span::styled(
            format!(" ({:+.1} true)", game.true_count()),
            Style::new().fg(theme::TEXT_DIM),
        ));
    }
    let right_width = right.iter().map(|s| s.content.len() as u16).sum::<u16>() + 2;

    let inner = area.inner(Margin::new(PAD, 0));
    let [msg_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(inner);
    frame.render_widget(
        Line::from(Span::styled(text, Style::new().fg(theme::TEXT))),
        msg_area,
    );
    frame.render_widget(Line::from(right).right_aligned(), right_area);
}

fn draw_footer(frame: &mut Frame, area: Rect, game: &Game) {
    let mut keys: Vec<Span> = Vec::new();
    match game.phase {
        Phase::Betting => {
            if game.is_broke() {
                keys.push(Span::styled("Out of chips. ", Style::new().fg(theme::LOSE)));
            }
            key(&mut keys, "←/→", "bet", true);
            key(&mut keys, "1/2/3/4", "chips", true);
            key(&mut keys, "ENTER", "deal", game.can_deal());
        }
        Phase::Dealing => keys.push(Span::styled("dealing…", Style::new().fg(theme::TEXT_DIM))),
        Phase::Insurance => {
            key(&mut keys, "Y", "insure", true);
            key(&mut keys, "N", "no thanks", true);
        }
        Phase::Player => {
            key(&mut keys, "H", "hit", game.can(Action::Hit));
            key(&mut keys, "S", "stand", game.can(Action::Stand));
            key(&mut keys, "D", "double", game.can(Action::Double));
            key(&mut keys, "P", "split", game.can(Action::Split));
            key(&mut keys, "R", "fold", game.can(Action::Surrender));
        }
        Phase::Dealer => keys.push(Span::styled(
            "dealer draws…",
            Style::new().fg(theme::TEXT_DIM),
        )),
        Phase::Settled => key(&mut keys, "ENTER", "next hand", true),
    }
    key(&mut keys, "C", "count", true);
    key(&mut keys, "Q", "quit", true);

    // The bankroll and shoe live on the status line above, so the footer is
    // the key list and nothing else.
    frame.render_widget(Line::from(keys), area.inner(Margin::new(PAD, 0)));
}

/// Push a `KEY label` pair, dimmed when the action is not available.
fn key(spans: &mut Vec<Span<'static>>, k: &str, label: &str, enabled: bool) {
    let (key_style, label_style) = if enabled {
        (
            Style::new().fg(theme::TRIM).add_modifier(Modifier::BOLD),
            Style::new().fg(theme::TEXT_DIM),
        )
    } else {
        let off = Style::new().fg(theme::DISABLED);
        (off, off)
    };
    spans.push(Span::styled(k.to_string(), key_style));
    spans.push(Span::styled(format!(" {label}  "), label_style));
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::cards::{Card, Rank, Suit};
    use crate::game::{Game, PlayerHand};

    const S: Suit = Suit::Spades;
    const H: Suit = Suit::Hearts;
    const D: Suit = Suit::Diamonds;
    const C: Suit = Suit::Clubs;

    fn hand(cards: &[(Rank, Suit)]) -> Hand {
        let mut h = Hand::default();
        for &(r, s) in cards {
            h.push(Card::new(r, s));
        }
        h
    }

    fn player(cards: &[(Rank, Suit)], bet: u32) -> PlayerHand {
        let mut p = PlayerHand::new(bet);
        p.hand = hand(cards);
        p
    }

    /// Render the whole table at a fixed size. This catches layout regressions
    /// — a fan drifting off centre, a hand clipped out of its column — that no
    /// amount of per-card testing would notice.
    fn table(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        terminal.backend().to_string()
    }

    fn staged() -> Game {
        Game::staged(
            hand(&[(Rank::Ten, D), (Rank::Four, C)]),
            vec![player(&[(Rank::Ace, S), (Rank::Seven, H)], 25)],
            Phase::Player,
        )
    }

    #[test]
    fn betting_table() {
        let app = App::with_game(Game::seeded(1));
        insta::assert_snapshot!(table(&app, 74, 32));
    }

    #[test]
    fn player_turn_with_hole_card_down() {
        let app = App::with_game(staged());
        insta::assert_snapshot!(table(&app, 74, 32));
    }

    #[test]
    fn split_shows_a_column_per_hand() {
        let mut g = Game::staged(
            hand(&[(Rank::Ten, D), (Rank::Four, C)]),
            vec![
                player(&[(Rank::Eight, S), (Rank::Three, D)], 25),
                player(&[(Rank::Eight, H), (Rank::King, C)], 25),
            ],
            Phase::Player,
        );
        g.active = 1;
        insta::assert_snapshot!(table(&App::with_game(g), 74, 32));
    }

    /// Four hands at 74 columns is the worst case the fan has to survive.
    #[test]
    fn four_split_hands_still_fit() {
        let g = Game::staged(
            hand(&[(Rank::Ten, D), (Rank::Four, C)]),
            (0..4)
                .map(|_| player(&[(Rank::Eight, S), (Rank::Three, D), (Rank::Two, C)], 25))
                .collect(),
            Phase::Player,
        );
        insta::assert_snapshot!(table(&App::with_game(g), 74, 32));
    }

    #[test]
    fn settled_table_shows_the_result_banner() {
        let mut g = staged();
        g.dealer = hand(&[(Rank::Ten, D), (Rank::Four, C), (Rank::Ten, S)]);
        g.hole_revealed = true;
        g.settle();
        insta::assert_snapshot!(table(&App::with_game(g), 74, 32));
    }

    /// A tall table has room for the painted outline around the chips.
    #[test]
    fn tall_window_paints_the_betting_spot() {
        let app = App::with_game(staged());
        insta::assert_snapshot!(table(&app, 74, 36));
    }

    /// A mixed wager should stack out into several chips.
    #[test]
    fn a_mixed_wager_stacks_several_chips() {
        let g = Game::staged(
            hand(&[(Rank::Ten, D), (Rank::Four, C)]),
            vec![player(&[(Rank::Ace, S), (Rank::Seven, H)], 135)],
            Phase::Player,
        );
        insta::assert_snapshot!(table(&App::with_game(g), 74, 36));
    }

    #[test]
    fn surrendered_hand_is_labelled_and_bannered() {
        let mut g = Game::staged(
            hand(&[(Rank::Ten, D), (Rank::Four, C)]),
            vec![player(&[(Rank::Ten, S), (Rank::Six, H)], 25)],
            Phase::Player,
        );
        g.act(Action::Surrender);
        insta::assert_snapshot!(table(&App::with_game(g), 74, 36));
    }

    /// Hiding the count must take the readout away but leave the key that
    /// brings it back.
    #[test]
    fn count_can_be_hidden() {
        let mut app = App::with_game(staged());
        app.show_count = true;
        assert!(table(&app, 74, 32).contains("true)"));

        app.show_count = false;
        let hidden = table(&app, 74, 32);
        assert!(!hidden.contains("true)"), "the readout should be gone");
        assert!(
            hidden.contains("C count"),
            "the toggle should still be shown"
        );
    }

    #[test]
    fn short_window_drops_the_banner() {
        let app = App::with_game(staged());
        insta::assert_snapshot!(table(&app, 74, 23));
    }

    #[test]
    fn a_window_too_small_says_so_instead_of_panicking() {
        let app = App::with_game(staged());
        insta::assert_snapshot!(table(&app, 40, 12));
    }

    /// Every phase must render without panicking at a range of sizes.
    #[test]
    fn all_phases_render_at_any_size() {
        for phase in [
            Phase::Betting,
            Phase::Dealing,
            Phase::Insurance,
            Phase::Player,
            Phase::Dealer,
            Phase::Settled,
        ] {
            let mut g = staged();
            g.phase = phase;
            let app = App::with_game(g);
            for (w, h) in [(10, 4), (20, 10), (40, 24), (80, 24), (200, 60)] {
                let _ = table(&app, w, h);
            }
        }
    }
}
