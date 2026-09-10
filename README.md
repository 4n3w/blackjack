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
| your turn | `H` hit, `S` stand, `D` double, `P` split, `R` surrender |
| settled | `ENTER` next hand |
| any | `C` show/hide the count, `Q` or `Ctrl-C` quit |

Run out of chips — that is, drop below the table minimum — and the house ends
the session for you, with a parting word printed to the shell once the
terminal has been handed back. Those lines end in `\r\n` rather than `\n`, and
open with a blank line: leaving the alternate screen drops the cursor back
wherever the shell left it, and a terminal still shaking off raw mode treats a
bare newline as "down one row" without returning to the first column, which
walks the message diagonally down the screen.

## House rules

Six decks, dealer stands on all 17s, blackjack pays 3:2, double on any two
cards including after a split, split up to four hands, split aces get one card
each, insurance offered when the dealer shows an ace, and late surrender on
the first two cards of an unsplit hand for half the bet back. All of it is in
the constants at the top of `src/game.rs` — `DEALER_HITS_SOFT_17` flips the
dealer to H17, for instance.

## The count

A Hi-Lo running count is kept as you play, shown on the status line next to
the bankroll, with the true count (running ÷ decks left) beside it. `C` hides
it if you would rather play blind.

The subtlety is *when* a card counts. It is at the moment you could see it,
not when it leaves the shoe: the hole card sits face down for most of a round
and must not move the count until it is turned over, and the opening deal is
counted only once the UI has finished sliding those four cards out. A property
test plays a hundred seeded rounds and asserts the count always equals the
Hi-Lo sum of exactly the cards on the table. A fresh shoe resets it.

## Chips

The wager is shown as chips in a betting spot painted on the felt between the
dealer and the player — a flattened diamond, the way a real one is shaped, so
it holds a row of chips without eating the table's vertical space.

Chips use the usual casino colours ($1 white, $5 red, $25 green, $100 black,
$500 purple) and the amount is broken down greedily, which for these
denominations is also the fewest chips — there is a test that checks that
against a brute-force minimum. Equal chips are grouped into one stack with a
`×N` beside it, so five denominations is the most the spot can ever be wide,
however large the bet.

## Layout

| file | |
| --- | --- |
| `src/cards.rs` | Suits, ranks, the shoe, hand scoring. Knows nothing about terminals. |
| `src/game.rs` | The rules, as a phase machine. Knows nothing about terminals *or* time. |
| `src/app.rs` | Key handling and the tick that paces cards onto the table. |
| `src/ui/card.rs` | The card widget: borders, corner indices, the pip-layout table. |
| `src/ui/chip.rs` | Chips, the greedy breakdown, and the betting spot. |
| `src/ui/mod.rs` | The table: banner, dealer row, betting spot, player columns, footer. |
| `src/ui/theme.rs` | The palette. |

`game.rs` has no `ratatui` import and no clock. A round is driven by calling
`deal`, then `act` once per decision, then `dealer_step` until it stops, then
`settle` — the UI only decides how fast that happens. That is what makes the
awkward parts (soft aces, split aces, whether a two-card 21 counts as a
natural, insurance settling independently of the main bet) testable without a
terminal.

The main loop takes key presses through an `Input` trait and is generic over
the backend, so tests drive the real loop with scripted keys against ratatui's
`TestBackend` rather than a live terminal. `App::without_delays` zeroes the
deal and dealer pacing for those, which is what lets fifty seeded sessions be
played all the way to bankruptcy in under a second.

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

Three payouts round down to the dollar: a 3:2 natural on an odd bet,
insurance at half an odd bet, and a surrendered odd bet. Real tables round to
the nearest chip too.

## Layout budget

The table gives up detail as the window shrinks, in this order: the banner
goes first, then the betting spot loses its painted outline, then the chips
go entirely. Below 22 rows it says what it needs instead of drawing a broken
table. Fans also tighten their overlap rather than clipping, so four split
hands still fit across 74 columns.

## Not yet built

Resplitting aces, even-money on a natural against an ace, or any persistence
of the bankroll between runs. Sliding the chips into the spot would be the
obvious next bit of animation — the tick loop already paces the deal and the
dealer's draws.
