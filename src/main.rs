//! A blackjack table, drawn in the terminal.

mod app;
mod cards;
mod game;
mod ui;

use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::Terminal;
use ratatui::backend::Backend;

use app::App;

/// Roughly 60fps. This doubles as the input poll timeout, so a keypress is
/// never more than a frame away from being handled.
const FRAME: Duration = Duration::from_millis(16);

/// Where key presses come from.
///
/// Behind a trait so the loop can be driven by a scripted sequence in tests.
/// There is no other way to exercise the thing end to end without a live
/// terminal, and the run-out-of-chips path in particular only happens after a
/// whole round has been played.
trait Input {
    /// Wait up to `timeout` for a key press, if one is coming.
    fn poll_key(&mut self, timeout: Duration) -> Result<Option<KeyEvent>>;
}

/// The real one: crossterm's event queue.
struct TerminalInput;

impl Input for TerminalInput {
    fn poll_key(&mut self, timeout: Duration) -> Result<Option<KeyEvent>> {
        // Releases and repeats are filtered out here: some terminals send
        // them, and acting on both edges would double every keystroke.
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            return Ok(Some(key));
        }
        Ok(None)
    }
}

fn main() -> Result<()> {
    // color-eyre's hooks go in first. `ratatui::init` wraps whatever panic hook
    // is already installed, so the terminal is restored *before* the pretty
    // report prints — otherwise the backtrace lands in the alternate screen and
    // vanishes when it is torn down.
    color_eyre::install()?;
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut TerminalInput, App::new());
    ratatui::restore();

    // Printed after the alternate screen is torn down, so the parting word
    // survives in the shell's scrollback rather than going with the table.
    if let Ok(Some(farewell)) = &result {
        println!("{farewell}");
    }
    result.map(|_| ())
}

fn run<B, I>(terminal: &mut Terminal<B>, input: &mut I, mut app: App) -> Result<Option<String>>
where
    B: Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
    I: Input,
{
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &app))?;
        if let Some(key) = input.poll_key(FRAME)? {
            app.on_key(key);
        }
        app.tick();
    }
    Ok(app.farewell)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::game::Game;

    /// A canned sequence of key presses. Once it runs dry it presses `q`, so a
    /// test can never wedge the loop open.
    struct ScriptedInput {
        keys: Vec<KeyCode>,
        at: usize,
    }

    impl ScriptedInput {
        fn new(keys: Vec<KeyCode>) -> Self {
            Self { keys, at: 0 }
        }
    }

    impl Input for ScriptedInput {
        fn poll_key(&mut self, _timeout: Duration) -> Result<Option<KeyEvent>> {
            let code = self
                .keys
                .get(self.at)
                .copied()
                .unwrap_or(KeyCode::Char('q'));
            self.at += 1;
            Ok(Some(KeyEvent::new(code, KeyModifiers::NONE)))
        }
    }

    fn play_seed(seed: u64, keys: Vec<KeyCode>) -> Option<String> {
        let mut terminal = Terminal::new(TestBackend::new(74, 32)).unwrap();
        let app = App::with_game(Game::seeded(seed)).without_delays();
        run(&mut terminal, &mut ScriptedInput::new(keys), app).unwrap()
    }

    fn play(keys: Vec<KeyCode>) -> Option<String> {
        play_seed(3, keys)
    }

    /// One round of betting everything and drawing until it is gone.
    ///
    /// Hitting ten times busts from any two cards — unless it lands on exactly
    /// 21, which stands and can then *win*. So the cycle is repeated rather
    /// than trusted once; the loop stops itself the moment the money runs out
    /// and the leftover keys go nowhere.
    fn bust_cycle() -> Vec<KeyCode> {
        let mut keys = vec![
            KeyCode::Char('4'), // all in
            KeyCode::Enter,     // deal
            KeyCode::Char('x'), // skip the reveal
            KeyCode::Char('n'), // no insurance
        ];
        keys.extend(std::iter::repeat_n(KeyCode::Char('h'), 10));
        keys.push(KeyCode::Enter); // collect, and on to the next
        keys
    }

    /// The whole point: bet the lot, lose it, and get shown the door — with
    /// the message coming back out of the loop so `main` can print it.
    #[test]
    fn busting_out_ends_the_session_with_a_word_from_the_house() {
        let keys: Vec<KeyCode> = std::iter::repeat_with(bust_cycle)
            .take(12)
            .flatten()
            .collect();
        let farewell = play(keys).expect("the house should have said something");
        assert!(
            farewell.contains("Stay out of my casino, Lebowski."),
            "got: {farewell}"
        );
        assert!(farewell.contains("out of chips"), "got: {farewell}");
    }

    /// It must happen whatever the shoe does, not just for one lucky seed.
    #[test]
    fn the_house_always_wins_eventually() {
        for seed in 0..50 {
            let keys: Vec<KeyCode> = std::iter::repeat_with(bust_cycle)
                .take(40)
                .flatten()
                .collect();
            assert!(
                play_seed(seed, keys).is_some(),
                "seed {seed} still had chips after 40 all-in rounds"
            );
        }
    }

    /// Quitting with money still on the table says nothing at all.
    #[test]
    fn walking_away_solvent_is_a_quiet_exit() {
        assert_eq!(play(vec![KeyCode::Char('q')]), None);
    }

    /// A losing round that leaves something in the purse is not the end.
    #[test]
    fn a_small_loss_keeps_the_table_open() {
        let mut keys = vec![
            KeyCode::Char('1'), // the table minimum
            KeyCode::Enter,
            KeyCode::Char('x'),
            KeyCode::Char('n'),
        ];
        keys.extend(std::iter::repeat_n(KeyCode::Char('h'), 10));
        keys.push(KeyCode::Enter);
        keys.push(KeyCode::Char('q'));
        assert_eq!(play(keys), None, "still solvent, so no send-off");
    }
}
