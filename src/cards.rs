//! The card model: suits, ranks, decks and hand scoring.
//!
//! Nothing in this module knows that a terminal exists. That is deliberate — the
//! rules of blackjack are worth testing on their own, and the UI is just one way
//! of looking at them.

use rand::Rng;
use rand::seq::SliceRandom;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
}

impl Suit {
    pub const ALL: [Self; 4] = [Self::Spades, Self::Hearts, Self::Diamonds, Self::Clubs];

    /// The pip character. These live in the Miscellaneous Symbols block and are
    /// well supported basically everywhere, unlike the playing-card block.
    pub const fn glyph(self) -> char {
        match self {
            Self::Spades => '♠',
            Self::Hearts => '♥',
            Self::Diamonds => '♦',
            Self::Clubs => '♣',
        }
    }

    pub const fn is_red(self) -> bool {
        matches!(self, Self::Hearts | Self::Diamonds)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rank {
    Ace,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
}

impl Rank {
    pub const ALL: [Self; 13] = [
        Self::Ace,
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
        Self::Seven,
        Self::Eight,
        Self::Nine,
        Self::Ten,
        Self::Jack,
        Self::Queen,
        Self::King,
    ];

    /// What goes in the card's corners. "10" is the only two-character label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ace => "A",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "10",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
        }
    }

    pub const fn is_face(self) -> bool {
        matches!(self, Self::Jack | Self::Queen | Self::King)
    }

    /// Aces count as 1 here; [`Hand::value`] promotes one to 11 when it can.
    pub const fn base_value(self) -> u8 {
        match self {
            Self::Ace => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
            Self::Eight => 8,
            Self::Nine => 9,
            Self::Ten | Self::Jack | Self::Queen | Self::King => 10,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Card {
    pub rank: Rank,
    pub suit: Suit,
}

impl Card {
    pub const fn new(rank: Rank, suit: Suit) -> Self {
        Self { rank, suit }
    }
}

/// A shoe of one or more 52-card decks.
#[derive(Clone, Debug)]
pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    pub fn new(deck_count: usize) -> Self {
        let mut cards = Vec::with_capacity(52 * deck_count);
        for _ in 0..deck_count {
            for suit in Suit::ALL {
                for rank in Rank::ALL {
                    cards.push(Card::new(rank, suit));
                }
            }
        }
        Self { cards }
    }

    pub fn shuffle<R: Rng + ?Sized>(&mut self, rng: &mut R) {
        self.cards.shuffle(rng);
    }

    pub fn draw(&mut self) -> Option<Card> {
        self.cards.pop()
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }
}

/// The value of a hand. `soft` means an ace is currently counted as 11, so the
/// hand can absorb one more card without busting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HandValue {
    pub total: u8,
    pub soft: bool,
}

#[derive(Clone, Default, Debug)]
pub struct Hand {
    pub cards: Vec<Card>,
}

impl Hand {
    pub fn push(&mut self, card: Card) {
        self.cards.push(card);
    }

    /// Count every ace as 1, then promote a single ace to 11 if that still fits.
    /// Promoting more than one would always bust, so one is the most we ever try.
    pub fn value(&self) -> HandValue {
        let total: u8 = self.cards.iter().map(|c| c.rank.base_value()).sum();
        let has_ace = self.cards.iter().any(|c| c.rank == Rank::Ace);
        if has_ace && total + 10 <= 21 {
            HandValue {
                total: total + 10,
                soft: true,
            }
        } else {
            HandValue { total, soft: false }
        }
    }

    pub fn is_bust(&self) -> bool {
        self.value().total > 21
    }

    pub fn is_blackjack(&self) -> bool {
        self.cards.len() == 2 && self.value().total == 21
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hand(ranks: &[Rank]) -> Hand {
        let mut h = Hand::default();
        for &r in ranks {
            h.push(Card::new(r, Suit::Spades));
        }
        h
    }

    #[test]
    fn fresh_deck_has_no_duplicates() {
        let deck = Deck::new(1);
        assert_eq!(deck.len(), 52);
        let mut seen = deck.cards.clone();
        seen.sort_by_key(|c| (c.suit as u8, c.rank as u8));
        seen.dedup();
        assert_eq!(seen.len(), 52);
    }

    #[test]
    fn shuffling_preserves_the_multiset() {
        let mut deck = Deck::new(2);
        let before = deck.len();
        deck.shuffle(&mut rand::rng());
        assert_eq!(deck.len(), before);
    }

    #[test]
    fn ace_is_soft_until_it_would_bust() {
        assert_eq!(
            hand(&[Rank::Ace, Rank::Six]).value(),
            HandValue {
                total: 17,
                soft: true
            }
        );
        // The same hand plus a ten: 11 no longer fits, so the ace drops to 1.
        assert_eq!(
            hand(&[Rank::Ace, Rank::Six, Rank::Ten]).value(),
            HandValue {
                total: 17,
                soft: false
            }
        );
    }

    #[test]
    fn multiple_aces_promote_at_most_one() {
        assert_eq!(
            hand(&[Rank::Ace, Rank::Ace]).value(),
            HandValue {
                total: 12,
                soft: true
            }
        );
        assert_eq!(
            hand(&[Rank::Ace, Rank::Ace, Rank::Nine]).value(),
            HandValue {
                total: 21,
                soft: true
            }
        );
        assert_eq!(
            hand(&[Rank::Ace, Rank::Ace, Rank::Ace, Rank::Ace]).value(),
            HandValue {
                total: 14,
                soft: true
            }
        );
    }

    #[test]
    fn blackjack_is_two_cards_only() {
        assert!(hand(&[Rank::Ace, Rank::King]).is_blackjack());
        assert!(!hand(&[Rank::Seven, Rank::Seven, Rank::Seven]).is_blackjack());
    }

    #[test]
    fn busting_is_hard_over_21() {
        assert!(hand(&[Rank::King, Rank::Queen, Rank::Jack]).is_bust());
        assert!(!hand(&[Rank::Ace, Rank::King]).is_bust());
    }
}
