use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::application::state::{map_key_to_action, AppMode, AppState, KeyAction};
use crate::error::Result;

mod render;
use render::render;

pub fn run_tui(state: &mut AppState) -> Result<()> {
    if std::env::var("CANOPY_TUI_TEST_MODE").as_deref() == Ok("1") {
        state.poll_project_events();
        state.poll_inference_events();
        return Ok(());
    }

    enable_raw_mode().map_err(|source| crate::error::CanopyError::io("terminal", source))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|source| crate::error::CanopyError::io("terminal", source))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|source| crate::error::CanopyError::io("terminal", source))?;

    let result = loop {
        state.poll_inference_events();
        state.poll_project_events();
        terminal
            .draw(|frame| render(frame, state))
            .map_err(|source| crate::error::CanopyError::io("terminal", source))?;

        if state.should_quit {
            break Ok(());
        }

        if event::poll(Duration::from_millis(80))
            .map_err(|source| crate::error::CanopyError::io("terminal", source))?
        {
            if let Event::Key(key) =
                event::read().map_err(|source| crate::error::CanopyError::io("terminal", source))?
            {
                let (mut action, pending_g) = map_key_to_action(key, state.pending_g, state.mode);
                state.pending_g = pending_g;

                if key.code == KeyCode::Enter {
                    action = match state.mode {
                        AppMode::Normal => KeyAction::Drill,
                        _ => KeyAction::Accept,
                    };
                }
                if key.code == KeyCode::Esc {
                    action = match state.mode {
                        AppMode::Normal => KeyAction::Back,
                        _ => KeyAction::Cancel,
                    };
                }
                if key.code == KeyCode::Backspace && matches!(state.mode, AppMode::Normal) {
                    action = KeyAction::Back;
                }

                if matches!(action, KeyAction::Noop) {
                    continue;
                }

                if let Err(err) = state.apply(action) {
                    state.status_line = format!("Error: {err}");
                }
            }
        }
    };

    disable_raw_mode().map_err(|source| crate::error::CanopyError::io("terminal", source))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|source| crate::error::CanopyError::io("terminal", source))?;
    terminal
        .show_cursor()
        .map_err(|source| crate::error::CanopyError::io("terminal", source))?;

    result
}
