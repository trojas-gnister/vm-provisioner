use std::time::Duration;

use ratatui::crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use super::app::App;
use super::handler;
use super::ui;
use vm_provisioner::error::Result;

const TICK_RATE: Duration = Duration::from_millis(250);
const REFRESH_INTERVAL: Duration = Duration::from_secs(3);

pub fn run_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    while app.running {
        terminal.draw(|frame| ui::render(frame, app))?;

        if event::poll(TICK_RATE)? {
            if let Event::Key(key) = event::read()? {
                handler::handle_key(app, key);
            }
        }

        if app.last_refresh.elapsed() >= REFRESH_INTERVAL {
            app.refresh_vm_list();
        }
    }
    Ok(())
}
