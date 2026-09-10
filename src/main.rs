//! A blackjack table, drawn in the terminal.

mod app;
mod cards;
mod game;
mod ui;

use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyEventKind};

use app::App;

/// Roughly 60fps. This doubles as the input poll timeout, so a keypress is
/// never more than a frame away from being handled.
const FRAME: Duration = Duration::from_millis(16);

fn main() -> Result<()> {
    // color-eyre's hooks go in first. `ratatui::init` wraps whatever panic hook
    // is already installed, so the terminal is restored *before* the pretty
    // report prints — otherwise the backtrace lands in the alternate screen and
    // vanishes when it is torn down.
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();
    result
}

fn run(mut terminal: ratatui::DefaultTerminal) -> Result<()> {
    let mut app = App::new();
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &app))?;
        if event::poll(FRAME)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key);
        }
        app.tick();
    }
    Ok(())
}
