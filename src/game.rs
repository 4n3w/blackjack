//! The rules of blackjack, as a state machine.
//!
//! Nothing here knows about terminals or time. A round is driven by calling
//! [`Game::deal`], then [`Game::act`] once per player decision, then stepping
//! [`Game::dealer_step`] until it stops, then [`Game::settle`]. The UI decides
//! how fast that happens; the rules do not care.
//!
//! House rules implemented here: six decks, dealer stands on all 17s, blackjack
//! pays 3:2, double on any two cards including after a split, split up to four
//! hands, split aces get one card each and cannot be resplit, insurance offered
//! when the dealer shows an ace.

use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::cards::{Card, Deck, Hand, Rank};

/// Decks in the shoe.
const SHOE_DECKS: usize = 6;
/// Reshuffle between rounds once the shoe drops below this. Six decks dealt to
/// about 80% penetration, which is a normal shoe game.
const RESHUFFLE_AT: usize = 62;
/// The most hands a player can reach by splitting, i.e. three resplits.
pub const MAX_HANDS: usize = 4;
/// Dealer hits soft 17? `false` is the S17 rule, better for the player.
const DEALER_HITS_SOFT_17: bool = false;

pub const STARTING_BANKROLL: u32 = 500;
pub const MIN_BET: u32 = 5;
pub const BET_STEP: u32 = 5;

/// How a single hand finished.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// A natural: 21 on the first two cards. Pays 3:2.
    Blackjack,
    Win,
    Push,
    Lose,
    Bust,
}

impl Outcome {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Blackjack => "BLACKJACK",
            Self::Win => "WIN",
            Self::Push => "PUSH",
            Self::Lose => "LOSE",
            Self::Bust => "BUST",
        }
    }

    pub const fn is_win(self) -> bool {
        matches!(self, Self::Blackjack | Self::Win)
    }
}

/// One of the player's hands. There is more than one only after a split.
#[derive(Clone, Debug)]
pub struct PlayerHand {
    pub hand: Hand,
    pub bet: u32,
    pub doubled: bool,
    /// This hand came out of a split, so a two-card 21 is just 21, not a
    /// natural, and it does not get paid 3:2.
    pub from_split: bool,
    /// No more decisions to make on this hand.
    pub done: bool,
    pub outcome: Option<Outcome>,
}

impl PlayerHand {
    pub fn new(bet: u32) -> Self {
        Self {
            hand: Hand::default(),
            bet,
            doubled: false,
            from_split: false,
            done: false,
            outcome: None,
        }
    }

    /// A natural pays 3:2, but only if it was dealt, not assembled from a split.
    pub fn is_natural(&self) -> bool {
        !self.from_split && self.hand.is_blackjack()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Choosing a wager.
    Betting,
    /// The opening four cards are being put on the table.
    Dealing,
    /// Dealer shows an ace; the insurance bet is on offer.
    Insurance,
    /// The player is acting on `active`.
    Player,
    /// The dealer is drawing.
    Dealer,
    /// Bets are paid; press on for the next round.
    Settled,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Hit,
    Stand,
    Double,
    Split,
}

pub struct Game {
    shoe: Deck,
    rng: StdRng,
    pub dealer: Hand,
    pub hands: Vec<PlayerHand>,
    /// Index into `hands` of the hand being played.
    pub active: usize,
    pub bankroll: u32,
    /// The wager for the next round, carried between rounds.
    pub bet: u32,
    pub phase: Phase,
    /// Amount staked on insurance this round; zero if declined or not offered.
    pub insurance: u32,
    pub hole_revealed: bool,
    /// Total put at risk this round, for reporting the net result.
    staked: u32,
    returned: u32,
    /// A short line describing what just happened, for the status bar.
    pub message: String,
}

impl Game {
    pub fn new() -> Self {
        Self::from_rng(StdRng::from_rng(&mut rand::rng()))
    }

    /// A game whose shoe shuffles the same way every time, for tests.
    #[cfg(test)]
    pub fn seeded(seed: u64) -> Self {
        Self::from_rng(StdRng::seed_from_u64(seed))
    }

    /// A game part-way through a round with known cards on the table.
    ///
    /// Tests and UI snapshots need exact hands, which the shoe will not oblige
    /// with. This takes the wagers out of the bankroll and records them, so the
    /// money still adds up when the round settles.
    #[cfg(test)]
    pub fn staged(dealer: Hand, hands: Vec<PlayerHand>, phase: Phase) -> Self {
        let mut g = Self::seeded(1);
        g.staked = hands.iter().map(|h| h.bet).sum();
        g.bankroll -= g.staked;
        g.bet = hands.first().map_or(MIN_BET, |h| h.bet);
        g.dealer = dealer;
        g.hands = hands;
        g.phase = phase;
        g.message = String::new();
        g
    }

    fn from_rng(mut rng: StdRng) -> Self {
        let mut shoe = Deck::new(SHOE_DECKS);
        shoe.shuffle(&mut rng);
        Self {
            shoe,
            rng,
            dealer: Hand::default(),
            hands: Vec::new(),
            active: 0,
            bankroll: STARTING_BANKROLL,
            bet: MIN_BET * 5,
            phase: Phase::Betting,
            insurance: 0,
            hole_revealed: false,
            staked: 0,
            returned: 0,
            message: String::from("Place your bet."),
        }
    }

    pub fn shoe_len(&self) -> usize {
        self.shoe.len()
    }

    /// The hand the player is currently acting on, if any.
    pub fn active_hand(&self) -> Option<&PlayerHand> {
        self.hands.get(self.active)
    }

    /// True once the player is out of money and cannot cover the minimum bet.
    pub fn is_broke(&self) -> bool {
        self.phase == Phase::Betting && self.bankroll < MIN_BET
    }

    // -- betting ----------------------------------------------------------

    /// Nudge the wager, clamped to the table minimum and what is in the purse.
    pub fn adjust_bet(&mut self, delta: i64) {
        if self.phase != Phase::Betting {
            return;
        }
        let ceiling = self.bankroll.max(MIN_BET);
        let raw = i64::from(self.bet) + delta;
        self.bet = raw.clamp(i64::from(MIN_BET), i64::from(ceiling)) as u32;
    }

    pub fn set_bet(&mut self, amount: u32) {
        if self.phase != Phase::Betting {
            return;
        }
        self.bet = amount.clamp(MIN_BET, self.bankroll.max(MIN_BET));
    }

    pub fn can_deal(&self) -> bool {
        self.phase == Phase::Betting && self.bet >= MIN_BET && self.bet <= self.bankroll
    }

    /// Put the opening four cards out. The cards are dealt immediately; the UI
    /// reveals them one at a time and then calls [`Game::finish_dealing`].
    pub fn deal(&mut self) {
        if !self.can_deal() {
            return;
        }
        // Only ever reshuffle between rounds, never mid-hand.
        if self.shoe.len() < RESHUFFLE_AT {
            self.shoe = Deck::new(SHOE_DECKS);
            self.shoe.shuffle(&mut self.rng);
        }

        self.dealer = Hand::default();
        self.hands = vec![PlayerHand::new(self.bet)];
        self.active = 0;
        self.insurance = 0;
        self.hole_revealed = false;
        self.staked = self.bet;
        self.returned = 0;
        self.bankroll -= self.bet;
        self.message.clear();

        for _ in 0..2 {
            let c = self.draw();
            self.hands[0].hand.push(c);
            let c = self.draw();
            self.dealer.push(c);
        }
        self.phase = Phase::Dealing;
    }

    /// Called once the opening deal has finished being revealed.
    pub fn finish_dealing(&mut self) {
        if self.phase != Phase::Dealing {
            return;
        }
        // Insurance is only offered against an ace, and only if the player can
        // cover half their bet.
        if self.dealer_upcard().map(|c| c.rank) == Some(Rank::Ace)
            && self.bankroll >= self.bet / 2
            && self.bet / 2 > 0
        {
            self.phase = Phase::Insurance;
            self.message = String::from("Dealer shows an ace. Insurance?");
        } else {
            self.check_naturals();
        }
    }

    pub fn take_insurance(&mut self, yes: bool) {
        if self.phase != Phase::Insurance {
            return;
        }
        if yes {
            let stake = self.bet / 2;
            self.insurance = stake;
            self.bankroll -= stake;
            self.staked += stake;
        }
        self.check_naturals();
    }

    /// The dealer peeks at the hole card when it could make a natural, and a
    /// player natural ends the hand too. Either way the round can be over
    /// before the player ever acts.
    fn check_naturals(&mut self) {
        self.message.clear();
        if self.dealer.is_blackjack() || self.hands[0].is_natural() {
            self.hole_revealed = true;
            self.settle();
        } else {
            self.phase = Phase::Player;
        }
    }

    // -- player turn ------------------------------------------------------

    pub fn can(&self, action: Action) -> bool {
        if self.phase != Phase::Player {
            return false;
        }
        let Some(h) = self.active_hand() else {
            return false;
        };
        if h.done {
            return false;
        }
        match action {
            Action::Hit | Action::Stand => true,
            Action::Double => h.hand.cards.len() == 2 && self.bankroll >= h.bet,
            Action::Split => {
                h.hand.cards.len() == 2
                    && self.hands.len() < MAX_HANDS
                    && self.bankroll >= h.bet
                    && h.hand.cards[0].rank.base_value() == h.hand.cards[1].rank.base_value()
            }
        }
    }

    pub fn act(&mut self, action: Action) {
        if !self.can(action) {
            return;
        }
        match action {
            Action::Hit => {
                let c = self.draw();
                self.hands[self.active].hand.push(c);
                self.finish_if_done();
            }
            Action::Stand => self.hands[self.active].done = true,
            Action::Double => {
                let extra = self.hands[self.active].bet;
                self.bankroll -= extra;
                self.staked += extra;
                self.hands[self.active].bet += extra;
                self.hands[self.active].doubled = true;
                let c = self.draw();
                self.hands[self.active].hand.push(c);
                // A double buys exactly one card.
                self.hands[self.active].done = true;
            }
            Action::Split => self.split(),
        }
        self.advance();
    }

    fn split(&mut self) {
        let bet = self.hands[self.active].bet;
        self.bankroll -= bet;
        self.staked += bet;

        let moved = self.hands[self.active].hand.cards.pop().expect("two cards");
        let mut new = PlayerHand::new(bet);
        new.from_split = true;
        new.hand.push(moved);

        self.hands[self.active].from_split = true;
        let c = self.draw();
        self.hands[self.active].hand.push(c);
        let c = self.draw();
        new.hand.push(c);

        self.hands.insert(self.active + 1, new);

        // Split aces get exactly one card each and are not played on.
        if self.hands[self.active].hand.cards[0].rank == Rank::Ace {
            self.hands[self.active].done = true;
            self.hands[self.active + 1].done = true;
        } else {
            self.finish_if_done();
            let next = self.active + 1;
            self.finish_hand_if_done(next);
        }
    }

    /// A hand that has busted or reached 21 has nothing left to decide.
    fn finish_if_done(&mut self) {
        let i = self.active;
        self.finish_hand_if_done(i);
    }

    fn finish_hand_if_done(&mut self, i: usize) {
        let h = &mut self.hands[i];
        if h.hand.is_bust() {
            h.done = true;
            h.outcome = Some(Outcome::Bust);
        } else if h.hand.value().total == 21 {
            h.done = true;
        }
    }

    /// Move to the next undecided hand, or hand over to the dealer.
    fn advance(&mut self) {
        if let Some(next) = (0..self.hands.len()).find(|&i| !self.hands[i].done) {
            self.active = next;
            return;
        }
        // Every hand is finished. The dealer only bothers to play if there is
        // still a live hand to beat.
        self.hole_revealed = true;
        if self.hands.iter().all(|h| h.hand.is_bust()) {
            self.settle();
        } else {
            self.phase = Phase::Dealer;
        }
    }

    // -- dealer turn ------------------------------------------------------

    /// Should the dealer take another card?
    fn dealer_hits(&self) -> bool {
        let v = self.dealer.value();
        v.total < 17 || (DEALER_HITS_SOFT_17 && v.total == 17 && v.soft)
    }

    /// Draw one dealer card if the rules call for it. Returns whether a card
    /// was drawn, so the UI can pace the draw and then settle.
    pub fn dealer_step(&mut self) -> bool {
        if self.phase != Phase::Dealer {
            return false;
        }
        if self.dealer_hits() {
            let c = self.draw();
            self.dealer.push(c);
            true
        } else {
            self.settle();
            false
        }
    }

    // -- settlement -------------------------------------------------------

    /// Score every hand, pay the bets, and write the summary line.
    pub fn settle(&mut self) {
        let dealer_bj = self.dealer.is_blackjack();
        let dealer_total = self.dealer.value().total;
        let dealer_bust = self.dealer.is_bust();

        // Insurance is settled first, and independently of the main bets: it
        // wins at 2:1 exactly when the dealer has a natural.
        if self.insurance > 0 && dealer_bj {
            self.returned += self.insurance * 3;
        }

        for h in &mut self.hands {
            let outcome = if h.hand.is_bust() {
                Outcome::Bust
            } else if h.is_natural() {
                if dealer_bj {
                    Outcome::Push
                } else {
                    Outcome::Blackjack
                }
            } else if dealer_bj {
                Outcome::Lose
            } else if dealer_bust {
                Outcome::Win
            } else {
                let total = h.hand.value().total;
                match total.cmp(&dealer_total) {
                    std::cmp::Ordering::Greater => Outcome::Win,
                    std::cmp::Ordering::Equal => Outcome::Push,
                    std::cmp::Ordering::Less => Outcome::Lose,
                }
            };
            h.outcome = Some(outcome);
            h.done = true;

            self.returned += match outcome {
                // 3:2, rounded down to the dollar the way a table rounds to
                // the nearest chip.
                Outcome::Blackjack => h.bet + h.bet * 3 / 2,
                Outcome::Win => h.bet * 2,
                Outcome::Push => h.bet,
                Outcome::Lose | Outcome::Bust => 0,
            };
        }

        self.bankroll += self.returned;
        self.phase = Phase::Settled;
        self.message = self.summary();
    }

    /// The net result of the round, in dollars.
    pub fn net(&self) -> i64 {
        i64::from(self.returned) - i64::from(self.staked)
    }

    /// The single word to put on the banner when a round ends.
    pub fn banner(&self) -> &'static str {
        if self.hands.iter().any(PlayerHand::is_natural) && self.net() > 0 {
            return "BLACKJACK";
        }
        match self.net() {
            n if n > 0 => "WIN",
            0 => "PUSH",
            _ if self.hands.iter().all(|h| h.hand.is_bust()) => "BUST",
            _ => "LOSE",
        }
    }

    fn summary(&self) -> String {
        let net = self.net();
        let money = match net {
            0 => String::from("You break even"),
            n if n > 0 => format!("You win ${n}"),
            n => format!("You lose ${}", -n),
        };
        let dealer = if self.dealer.is_blackjack() {
            String::from("Dealer has blackjack")
        } else if self.dealer.is_bust() {
            format!("Dealer busts with {}", self.dealer.value().total)
        } else {
            format!("Dealer stands on {}", self.dealer.value().total)
        };
        format!("{dealer}. {money}.")
    }

    /// Clear the table and go back to taking a bet.
    pub fn next_round(&mut self) {
        if self.phase != Phase::Settled {
            return;
        }
        self.hands.clear();
        self.dealer = Hand::default();
        self.active = 0;
        self.insurance = 0;
        self.hole_revealed = false;
        self.phase = Phase::Betting;
        // Do not carry a bet the player can no longer cover.
        if self.bet > self.bankroll {
            self.bet = self.bankroll.max(MIN_BET);
        }
        self.message = if self.bankroll < MIN_BET {
            String::from("You are out of chips.")
        } else {
            String::from("Place your bet.")
        };
    }

    // -- helpers ----------------------------------------------------------

    /// The dealer's face-up card.
    pub fn dealer_upcard(&self) -> Option<Card> {
        self.dealer.cards.first().copied()
    }

    /// What the dealer is showing, given the hole card may still be down.
    pub fn dealer_shown_value(&self) -> Option<u8> {
        if self.hole_revealed {
            (!self.dealer.cards.is_empty()).then(|| self.dealer.value().total)
        } else {
            self.dealer_upcard().map(|c| match c.rank {
                Rank::Ace => 11,
                r => r.base_value(),
            })
        }
    }

    /// Take a card, refilling the shoe in the unlikely event it runs dry
    /// mid-round.
    fn draw(&mut self) -> Card {
        if self.shoe.len() == 0 {
            self.shoe = Deck::new(SHOE_DECKS);
            self.shoe.shuffle(&mut self.rng);
        }
        self.shoe.draw().expect("shoe was just refilled")
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Suit;

    /// Drive a game with a stacked hand instead of the shoe, so the rules can
    /// be tested against exact cards.
    fn staged(player: &[(Rank, Suit)], dealer: &[(Rank, Suit)]) -> Game {
        let mut g = Game::seeded(1);
        g.bankroll = 1000;
        g.bet = 100;
        g.staked = 100;
        g.bankroll -= 100;
        let mut h = PlayerHand::new(100);
        for &(r, s) in player {
            h.hand.push(Card::new(r, s));
        }
        g.hands = vec![h];
        g.dealer = Hand::default();
        for &(r, s) in dealer {
            g.dealer.push(Card::new(r, s));
        }
        g.phase = Phase::Player;
        g
    }

    const S: Suit = Suit::Spades;
    const H: Suit = Suit::Hearts;

    #[test]
    fn natural_pays_three_to_two() {
        let mut g = staged(
            &[(Rank::Ace, S), (Rank::King, S)],
            &[(Rank::Nine, H), (Rank::Seven, H)],
        );
        g.settle();
        assert_eq!(g.hands[0].outcome, Some(Outcome::Blackjack));
        // $100 bet returns the stake plus $150.
        assert_eq!(g.net(), 150);
    }

    #[test]
    fn natural_against_natural_pushes() {
        let mut g = staged(
            &[(Rank::Ace, S), (Rank::King, S)],
            &[(Rank::Ace, H), (Rank::Queen, H)],
        );
        g.settle();
        assert_eq!(g.hands[0].outcome, Some(Outcome::Push));
        assert_eq!(g.net(), 0);
    }

    #[test]
    fn busting_loses_even_if_dealer_busts_too() {
        let mut g = staged(
            &[(Rank::King, S), (Rank::Queen, S), (Rank::Five, S)],
            &[(Rank::Ten, H), (Rank::Six, H), (Rank::Nine, H)],
        );
        g.settle();
        assert_eq!(g.hands[0].outcome, Some(Outcome::Bust));
        assert_eq!(g.net(), -100);
    }

    #[test]
    fn dealer_stands_on_soft_17() {
        // A-6 is a soft 17; under S17 the dealer must not draw.
        let mut g = staged(
            &[(Rank::Ten, S), (Rank::Eight, S)],
            &[(Rank::Ace, H), (Rank::Six, H)],
        );
        g.phase = Phase::Dealer;
        assert!(!g.dealer_step(), "dealer should stand on soft 17");
        assert_eq!(g.dealer.cards.len(), 2);
        assert_eq!(g.hands[0].outcome, Some(Outcome::Win));
    }

    #[test]
    fn dealer_draws_to_sixteen() {
        let mut g = staged(
            &[(Rank::Ten, S), (Rank::Eight, S)],
            &[(Rank::Ten, H), (Rank::Six, H)],
        );
        g.phase = Phase::Dealer;
        assert!(g.dealer_step(), "dealer must hit 16");
        assert_eq!(g.dealer.cards.len(), 3);
    }

    #[test]
    fn doubling_takes_one_card_and_doubles_the_bet() {
        let mut g = staged(
            &[(Rank::Six, S), (Rank::Five, S)],
            &[(Rank::Ten, H), (Rank::Seven, H)],
        );
        assert!(g.can(Action::Double));
        g.act(Action::Double);
        assert_eq!(g.hands[0].bet, 200);
        assert_eq!(g.hands[0].hand.cards.len(), 3);
        assert!(g.hands[0].done);
        assert!(g.hands[0].doubled);
    }

    #[test]
    fn cannot_double_after_hitting() {
        let mut g = staged(
            &[(Rank::Six, S), (Rank::Five, S)],
            &[(Rank::Ten, H), (Rank::Seven, H)],
        );
        g.act(Action::Hit);
        assert!(!g.can(Action::Double), "double is a two-card decision only");
    }

    #[test]
    fn splitting_makes_two_hands_and_stakes_a_second_bet() {
        let mut g = staged(
            &[(Rank::Eight, S), (Rank::Eight, H)],
            &[(Rank::Ten, H), (Rank::Seven, H)],
        );
        let purse = g.bankroll;
        assert!(g.can(Action::Split));
        g.act(Action::Split);
        assert_eq!(g.hands.len(), 2);
        assert_eq!(g.bankroll, purse - 100);
        for h in &g.hands {
            assert_eq!(h.hand.cards.len(), 2, "each split hand draws one card");
            assert!(h.from_split);
        }
    }

    #[test]
    fn tens_of_different_ranks_can_be_split() {
        let g = staged(
            &[(Rank::King, S), (Rank::Ten, H)],
            &[(Rank::Nine, H), (Rank::Seven, H)],
        );
        assert!(g.can(Action::Split), "any two ten-value cards split");
    }

    #[test]
    fn unequal_cards_cannot_be_split() {
        let g = staged(
            &[(Rank::Nine, S), (Rank::Eight, H)],
            &[(Rank::Nine, H), (Rank::Seven, H)],
        );
        assert!(!g.can(Action::Split));
    }

    #[test]
    fn split_aces_get_one_card_each_and_stop() {
        let mut g = staged(
            &[(Rank::Ace, S), (Rank::Ace, H)],
            &[(Rank::Ten, H), (Rank::Seven, H)],
        );
        g.act(Action::Split);
        assert_eq!(g.hands.len(), 2);
        assert!(
            g.hands.iter().all(|h| h.done),
            "split aces are not played on"
        );
        assert!(g.hands.iter().all(|h| h.hand.cards.len() == 2));
        // Twenty-one on a split hand is not a natural, so it never pays 3:2.
        assert!(g.hands.iter().all(|h| !h.is_natural()));
    }

    #[test]
    fn splitting_is_capped_at_four_hands() {
        let mut g = staged(
            &[(Rank::Eight, S), (Rank::Eight, H)],
            &[(Rank::Ten, H), (Rank::Seven, H)],
        );
        g.hands = (0..MAX_HANDS).map(|_| g.hands[0].clone()).collect();
        assert!(!g.can(Action::Split));
    }

    #[test]
    fn twenty_one_on_a_split_hand_beats_but_does_not_pay_bonus() {
        let mut g = staged(
            &[(Rank::Ace, S), (Rank::King, S)],
            &[(Rank::Ten, H), (Rank::Nine, H)],
        );
        g.hands[0].from_split = true;
        g.settle();
        assert_eq!(g.hands[0].outcome, Some(Outcome::Win));
        assert_eq!(g.net(), 100, "even money, not 3:2");
    }

    #[test]
    fn insurance_pays_two_to_one_against_a_dealer_natural() {
        let mut g = staged(
            &[(Rank::Ten, S), (Rank::Nine, S)],
            &[(Rank::Ace, H), (Rank::King, H)],
        );
        g.phase = Phase::Insurance;
        g.take_insurance(true);
        // Main bet loses $100, insurance stakes $50 and returns $150.
        assert_eq!(g.hands[0].outcome, Some(Outcome::Lose));
        assert_eq!(g.net(), 0, "insurance exactly covers the loss");
    }

    #[test]
    fn declined_insurance_stakes_nothing() {
        let mut g = staged(
            &[(Rank::Ten, S), (Rank::Nine, S)],
            &[(Rank::Ace, H), (Rank::Two, H)],
        );
        g.phase = Phase::Insurance;
        g.take_insurance(false);
        assert_eq!(g.insurance, 0);
        assert_eq!(g.phase, Phase::Player, "no natural, so play continues");
    }

    #[test]
    fn hole_card_stays_down_until_the_player_is_done() {
        let mut g = Game::seeded(7);
        g.deal();
        g.finish_dealing();
        if g.phase == Phase::Insurance {
            g.take_insurance(false);
        }
        if g.phase == Phase::Player {
            assert!(!g.hole_revealed);
            g.act(Action::Stand);
            assert!(g.hole_revealed, "standing turns the hole card over");
        }
    }

    #[test]
    fn a_full_round_conserves_money() {
        // Whatever happens, the bankroll only ever changes by the net result.
        for seed in 0..200 {
            let mut g = Game::seeded(seed);
            let before = g.bankroll;
            g.set_bet(25);
            g.deal();
            g.finish_dealing();
            if g.phase == Phase::Insurance {
                g.take_insurance(seed % 2 == 0);
            }
            while g.phase == Phase::Player {
                if g.can(Action::Hit) && g.active_hand().unwrap().hand.value().total < 17 {
                    g.act(Action::Hit);
                } else {
                    g.act(Action::Stand);
                }
            }
            while g.dealer_step() {}
            assert_eq!(g.phase, Phase::Settled, "seed {seed} did not settle");
            assert_eq!(
                i64::from(g.bankroll),
                i64::from(before) + g.net(),
                "seed {seed} leaked money"
            );
        }
    }

    #[test]
    fn bet_is_clamped_to_the_purse() {
        let mut g = Game::seeded(3);
        g.bankroll = 40;
        g.set_bet(1000);
        assert_eq!(g.bet, 40);
        g.adjust_bet(-1000);
        assert_eq!(g.bet, MIN_BET);
    }

    #[test]
    fn next_round_trims_a_bet_the_player_cannot_cover() {
        let mut g = Game::seeded(4);
        g.phase = Phase::Settled;
        g.bankroll = 15;
        g.bet = 100;
        g.next_round();
        assert_eq!(g.bet, 15);
        assert_eq!(g.phase, Phase::Betting);
    }
}
