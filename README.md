# blackjack

A blackjack table drawn in the terminal, with cards rendered as little bordered
windows — a rank in two corners, the suit's pip pattern in the middle, and hands
laid out as overlapping fans.

## Running

`rustup` is installed via Homebrew and is keg-only, so it needs to be on `PATH`:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"   # add to ~/.zshrc to make it stick
cargo run
```

The footer always shows the keys that are live in the current phase; unavailable
actions are dimmed rather than hidden.

| phase | keys |
| --- | --- |
| betting | `←`/`→` adjust bet, `1`/`2`/`3`/`4` for $5/$25/$100/all in, `ENTER` deal |
| dealing | any key skips the reveal |
| insurance | `Y` take it, `N` decline |
| your turn | `H` hit, `S` stand, `D` double, `P` split |
| settled | `ENTER` next hand |
| any | `Q` or `Ctrl-C` quit |

## House rules

Six decks, dealer stands on all 17s, blackjack pays 3:2, double on any two
cards including after a split, split up to four hands, split aces get one card
each, insurance offered when the dealer shows an ace. All of it is in the
constants at the top of `src/game.rs` — `DEALER_HITS_SOFT_17` flips the dealer
to H17, for instance.

## Layout

| file | |
| --- | --- |
| `src/cards.rs` | Suits, ranks, the shoe, hand scoring. Knows nothing about terminals. |
| `src/game.rs` | The rules, as a phase machine. Knows nothing about terminals *or* time. |
| `src/app.rs` | Key handling and the tick that paces cards onto the table. |
| `src/ui/card.rs` | The card widget: borders, corner indices, the pip-layout table. |
| `src/ui/mod.rs` | The table: banner, dealer row, player columns, footer. |
| `src/ui/theme.rs` | The palette. |

`game.rs` has no `ratatui` import and no clock. A round is driven by calling
`deal`, then `act` once per decision, then `dealer_step` until it stops, then
`settle` — the UI only decides how fast that happens. That is what makes the
awkward parts (soft aces, split aces, whether a two-card 21 counts as a
natural, insurance settling independently of the main bet) testable without a
terminal.

## Tests

```sh
cargo test
```

Card and full-table rendering are covered by [`insta`](https://insta.rs)
snapshots taken through ratatui's `TestBackend`, so a layout regression shows up
as a readable diff of the actual rendered grid. To review changes after
deliberately altering the drawing code:

```sh
cargo insta review     # cargo install cargo-insta
# or, to accept everything:
INSTA_UPDATE=always cargo test
```

## Notes on the drawing

Cards are 9×9 cells: a one-cell border leaves a 7×7 interior, which is one row
for each corner index and a 5-row pip field between them. Pip positions come
from a hand-formatted table in `card.rs` — there is no formula for how many pips
go where, only convention, so the table is written out to look like the cards it
draws.

Two things worth knowing if you extend the drawing:

- **Fans are free.** Ratatui composites into one buffer in call order, so
  painting cards left to right at a stride narrower than a card gives the
  overlap with no z-ordering.
- **`Block` does not clear what it covers.** It sets the *style* of its cells
  but leaves their contents, so a card laid over another lets the one underneath
  show through. `card::blank` wipes the area first.

The Unicode playing-card block (`🂡`–`🂮`) is deliberately unused: one glyph per
card means no control over size, patchy font coverage, and ambiguous cell width
that breaks layout. The suit glyphs `♠♥♦♣` are safe.

Two payouts round down to the dollar: a 3:2 natural on an odd bet, and
insurance at half an odd bet. Real tables round to the nearest chip too.

## Not yet built

Surrender, resplitting aces, a running count, or any persistence of the
bankroll between runs. Chip graphics for the bet would be the obvious next bit
of drawing — the tick loop already has room to slide them to the pot.
