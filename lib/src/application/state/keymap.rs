use super::AppMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Up,
    Down,
    Left,
    Right,
    Top,
    Bottom,
    Drill,
    Back,
    ToggleHelp,
    Search,
    Query,
    Edit,
    Regenerate,
    ToggleGraph,
    ToggleImpact,
    ToggleProjectView,
    QueueOnboarding,
    CancelOnboarding,
    RetryOnboarding,
    SwitchRepository,
    ShowHistory,
    Export,
    NextPane,
    PrevPane,
    PageUp,
    PageDown,
    Accept,
    Cancel,
    Input(char),
    Backspace,
    Noop,
}

pub fn map_key_to_action(
    key: crossterm::event::KeyEvent,
    pending_g: bool,
    mode: AppMode,
) -> (KeyAction, bool) {
    use crossterm::event::{KeyCode, KeyModifiers};

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return (KeyAction::Cancel, false);
    }

    let action = match key.code {
        KeyCode::Char('k') | KeyCode::Up => KeyAction::Up,
        KeyCode::Char('j') | KeyCode::Down => KeyAction::Down,
        KeyCode::Char('h') | KeyCode::Left => KeyAction::Left,
        KeyCode::Char('l') | KeyCode::Right => KeyAction::Right,
        KeyCode::Tab => KeyAction::NextPane,
        KeyCode::BackTab => KeyAction::PrevPane,
        KeyCode::PageUp => KeyAction::PageUp,
        KeyCode::PageDown => KeyAction::PageDown,
        KeyCode::Enter => KeyAction::Accept,
        KeyCode::Esc => KeyAction::Cancel,
        KeyCode::Backspace => KeyAction::Backspace,
        KeyCode::Char('q') => KeyAction::Cancel,
        KeyCode::Char('?') => KeyAction::ToggleHelp,
        KeyCode::Char('/') => KeyAction::Search,
        KeyCode::Char(':') => KeyAction::Query,
        KeyCode::Char('e') => KeyAction::Edit,
        KeyCode::Char('r') => KeyAction::Regenerate,
        KeyCode::Char('v') => KeyAction::ToggleGraph,
        KeyCode::Char('i') => KeyAction::ToggleImpact,
        KeyCode::Char('P') if matches!(mode, AppMode::Normal | AppMode::Project) => {
            KeyAction::ToggleProjectView
        }
        KeyCode::Char('o') if mode == AppMode::Project => KeyAction::QueueOnboarding,
        KeyCode::Char('c') if mode == AppMode::Project => KeyAction::CancelOnboarding,
        KeyCode::Char('R') if mode == AppMode::Project => KeyAction::RetryOnboarding,
        KeyCode::Char('s') if mode == AppMode::Project => KeyAction::SwitchRepository,
        KeyCode::Char('H') => KeyAction::ShowHistory,
        KeyCode::Char('X') => KeyAction::Export,
        KeyCode::Char('g') => {
            if pending_g {
                return (KeyAction::Top, false);
            }
            return (KeyAction::Noop, true);
        }
        KeyCode::Char('G') => KeyAction::Bottom,
        KeyCode::Char(c) => KeyAction::Input(c),
        _ => KeyAction::Noop,
    };

    (action, false)
}
