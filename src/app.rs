//! Input handling and the tick that paces cards onto the table.
//!
//! The rules live in [`crate::game`]. This layer only decides *when* things
//! happen — how fast the opening deal is revealed, how long the dealer pauses
//! between draws — and translates keystrokes into game actions.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::game::{Action, BET_STEP, Game, Phase};

/// Gap between cards during the opening deal.
const DEAL_INTERVAL: Duration = Duration::from_millis(150);
/// Gap between the dealer's draws, so you can watch them go bust.
const DEALER_INTERVAL: Duration = Duration::from_millis(450);
/// Cards in the opening deal: two each.
const OPENING_CARDS: usize = 4;

pub struct App {
    pub game: Game,
    pub should_quit: bool,
    /// Whether to show the running count. On by default — the whole reason to
    /// keep one is to be able to watch it.
    pub show_count: bool,
    /// How many cards of the opening deal have landed on the table.
    dealt: usize,
    last_step: Instant,
}

impl App {
    pub fn new() -> Self {
        Self::with_game(Game::new())
    }

    pub fn with_game(game: Game) -> Self {
        Self {
            game,
            should_quit: false,
            show_count: true,
            dealt: 0,
            last_step: Instant::now(),
        }
    }

    /// How many of the dealer's cards to draw. Everything is on the table
    /// except during the opening deal, which is revealed a card at a time.
    pub fn dealer_visible(&self) -> usize {
        match self.game.phase {
            Phase::Dealing => self.dealt / 2,
            _ => usize::MAX,
        }
    }

    /// As above, for one of the player's hands. Only the first hand exists
    /// during the opening deal, so the others are always fully visible.
    pub fn player_visible(&self, index: usize) -> usize {
        match self.game.phase {
            Phase::Dealing if index == 0 => self.dealt.div_ceil(2),
            Phase::Dealing => 0,
            _ => usize::MAX,
        }
    }

    /// Advance whatever is currently animating.
    pub fn tick(&mut self) {
        match self.game.phase {
            Phase::Dealing => {
                if self.last_step.elapsed() >= DEAL_INTERVAL {
                    self.last_step = Instant::now();
                    if self.dealt < OPENING_CARDS {
                        self.dealt += 1;
                    } else {
                        self.game.finish_dealing();
                    }
                }
            }
            Phase::Dealer if self.last_step.elapsed() >= DEALER_INTERVAL => {
                self.last_step = Instant::now();
                self.game.dealer_step();
            }
            _ => {}
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Char('c' | 'C'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.should_quit = true;
            return;
        }
        if matches!(key.code, KeyCode::Char('q' | 'Q')) {
            self.should_quit = true;
            return;
        }
        // The count can be hidden at any point, including mid-hand.
        if matches!(key.code, KeyCode::Char('c' | 'C')) {
            self.show_count = !self.show_count;
            return;
        }

        match self.game.phase {
            Phase::Betting => self.on_betting_key(key.code),
            // Any key skips the rest of the deal for the impatient.
            Phase::Dealing => {
                self.dealt = OPENING_CARDS;
                self.game.finish_dealing();
            }
            Phase::Insurance => match key.code {
                KeyCode::Char('y' | 'Y') => self.game.take_insurance(true),
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.game.take_insurance(false),
                _ => {}
            },
            Phase::Player => match key.code {
                KeyCode::Char('h' | 'H') => self.game.act(Action::Hit),
                KeyCode::Char('s' | 'S') => self.game.act(Action::Stand),
                KeyCode::Char('d' | 'D') => self.game.act(Action::Double),
                KeyCode::Char('p' | 'P') => self.game.act(Action::Split),
                KeyCode::Char('r' | 'R') => self.game.act(Action::Surrender),
                _ => {}
            },
            Phase::Dealer => {}
            Phase::Settled => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ' | 'd' | 'D')) {
                    self.game.next_round();
                }
            }
        }
    }

    fn on_betting_key(&mut self, code: KeyCode) {
        let step = i64::from(BET_STEP);
        match code {
            KeyCode::Left | KeyCode::Char('-' | '_') => self.game.adjust_bet(-step),
            KeyCode::Right | KeyCode::Char('+' | '=') => self.game.adjust_bet(step),
            KeyCode::Down => self.game.adjust_bet(-step * 5),
            KeyCode::Up => self.game.adjust_bet(step * 5),
            KeyCode::Char('1') => self.game.set_bet(5),
            KeyCode::Char('2') => self.game.set_bet(25),
            KeyCode::Char('3') => self.game.set_bet(100),
            KeyCode::Char('4') => {
                let all = self.game.bankroll;
                self.game.set_bet(all);
            }
            KeyCode::Enter | KeyCode::Char(' ' | 'd' | 'D') => self.start_deal(),
            _ => {}
        }
    }

    fn start_deal(&mut self) {
        if !self.game.can_deal() {
            return;
        }
        self.game.deal();
        self.dealt = 0;
        self.last_step = Instant::now();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app() -> App {
        App::with_game(Game::seeded(11))
    }

    #[test]
    fn q_quits() {
        let mut a = app();
        a.on_key(press(KeyCode::Char('q')));
        assert!(a.should_quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut a = app();
        a.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(a.should_quit);
    }

    #[test]
    fn arrows_change_the_bet() {
        let mut a = app();
        let start = a.game.bet;
        a.on_key(press(KeyCode::Right));
        assert_eq!(a.game.bet, start + BET_STEP);
        a.on_key(press(KeyCode::Left));
        assert_eq!(a.game.bet, start);
    }

    #[test]
    fn enter_deals_and_starts_the_reveal() {
        let mut a = app();
        a.on_key(press(KeyCode::Enter));
        assert_eq!(a.game.phase, Phase::Dealing);
        assert_eq!(a.dealt, 0, "cards land one at a time, not all at once");
        assert_eq!(a.dealer_visible(), 0);
        assert_eq!(a.player_visible(0), 0);
    }

    /// The opening deal alternates player, dealer, player, dealer.
    #[test]
    fn reveal_order_alternates() {
        let mut a = app();
        a.on_key(press(KeyCode::Enter));
        let seen: Vec<_> = (0..=OPENING_CARDS)
            .map(|n| {
                a.dealt = n;
                (a.player_visible(0), a.dealer_visible())
            })
            .collect();
        assert_eq!(seen, vec![(0, 0), (1, 0), (1, 1), (2, 1), (2, 2)]);
    }

    #[test]
    fn a_keypress_skips_the_deal_animation() {
        let mut a = app();
        a.on_key(press(KeyCode::Enter));
        a.on_key(press(KeyCode::Char('x')));
        assert_ne!(a.game.phase, Phase::Dealing, "deal should have completed");
    }

    #[test]
    fn everything_is_visible_once_the_deal_is_done() {
        let mut a = app();
        a.on_key(press(KeyCode::Enter));
        a.on_key(press(KeyCode::Char('x')));
        assert_eq!(a.dealer_visible(), usize::MAX);
        assert_eq!(a.player_visible(0), usize::MAX);
    }

    #[test]
    fn player_keys_reach_the_game() {
        let mut a = app();
        a.on_key(press(KeyCode::Enter));
        a.on_key(press(KeyCode::Char('x')));
        if a.game.phase == Phase::Insurance {
            a.on_key(press(KeyCode::Char('n')));
        }
        if a.game.phase == Phase::Player {
            let before = a.game.hands[0].hand.cards.len();
            a.on_key(press(KeyCode::Char('h')));
            assert!(a.game.hands[0].hand.cards.len() > before, "H should hit");
        }
    }

    /// Play a whole round on a timer, the way the real loop does.
    #[test]
    fn a_round_runs_to_settlement_through_the_tick() {
        let mut a = app();
        a.on_key(press(KeyCode::Enter));
        a.on_key(press(KeyCode::Char('x')));
        if a.game.phase == Phase::Insurance {
            a.on_key(press(KeyCode::Char('n')));
        }
        while a.game.phase == Phase::Player {
            a.on_key(press(KeyCode::Char('s')));
        }
        // The dealer draws on a timer; wind the clock forward instead of sleeping.
        let mut guard = 0;
        while a.game.phase == Phase::Dealer && guard < 40 {
            a.last_step = Instant::now() - DEALER_INTERVAL;
            a.tick();
            guard += 1;
        }
        assert_eq!(a.game.phase, Phase::Settled);

        a.on_key(press(KeyCode::Enter));
        assert_eq!(a.game.phase, Phase::Betting, "Enter starts the next round");
    }

    #[test]
    fn c_toggles_the_count() {
        let mut a = app();
        assert!(a.show_count, "the count is on by default");
        a.on_key(press(KeyCode::Char('c')));
        assert!(!a.show_count);
        a.on_key(press(KeyCode::Char('C')));
        assert!(a.show_count);
    }

    /// A plain `c` hides the count; only Ctrl-C quits.
    #[test]
    fn c_does_not_quit() {
        let mut a = app();
        a.on_key(press(KeyCode::Char('c')));
        assert!(!a.should_quit);
    }

    #[test]
    fn r_surrenders() {
        let mut a = app();
        a.on_key(press(KeyCode::Enter));
        a.on_key(press(KeyCode::Char('x')));
        if a.game.phase == Phase::Insurance {
            a.on_key(press(KeyCode::Char('n')));
        }
        if a.game.phase == Phase::Player && a.game.can(Action::Surrender) {
            a.on_key(press(KeyCode::Char('r')));
            assert_eq!(a.game.phase, Phase::Settled);
            assert!(a.game.hands[0].is_surrendered());
        }
    }

    #[test]
    fn key_events_carry_a_press_kind() {
        // Guards the main loop's filter: crossterm also emits Release events on
        // some terminals, and acting on both would double every keystroke.
        let e = press(KeyCode::Char('h'));
        assert_eq!(e.kind, KeyEventKind::Press);
    }
}
