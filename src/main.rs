mod app;
mod store;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, Screen};

fn main() -> io::Result<()> {
    let mut app = App::new();

    terminal::enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let tick_rate = Duration::from_millis(200);

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if event::poll(tick_rate)?
            && let Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) = event::read()?
        {
            match app.screen {
                Screen::Setup => match code {
                    KeyCode::Up | KeyCode::Char('k') => app.prev_field(),
                    KeyCode::Down | KeyCode::Char('j') => app.next_field(),
                    KeyCode::Left | KeyCode::Char('h') => app.decrement_field(),
                    KeyCode::Right | KeyCode::Char('l') => app.increment_field(),
                    KeyCode::Enter => app.start_timer(),
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    _ => {}
                },
                Screen::TaskInput => match code {
                    KeyCode::Enter => app.submit_task(),
                    KeyCode::Esc => app.skip_task_input(),
                    KeyCode::Backspace => app.task_input_backspace(),
                    KeyCode::Char(c) => app.task_input_char(c),
                    _ => {}
                },
                Screen::Timer => match code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Char(' ') => app.toggle_pause(),
                    KeyCode::Char('s') | KeyCode::Char('n') => app.skip_phase(),
                    KeyCode::Char('d') => app.screen = Screen::DailyLog,
                    _ => {}
                },
                Screen::DailyLog => match code {
                    KeyCode::Esc | KeyCode::Backspace => app.screen = Screen::Timer,
                    KeyCode::Char('q') => app.should_quit = true,
                    _ => {}
                },
            }
        }

        if app.screen == Screen::Timer {
            app.tick();
        }

        if app.should_quit {
            app.save_current_work_if_needed();
            break;
        }
    }

    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    if matches!(app.screen, Screen::Timer | Screen::DailyLog | Screen::TaskInput) {
        let completed = app.completed_work_sessions();
        println!(
            "👋 Done! Completed {} work session{}. Great job!",
            completed,
            if completed == 1 { "" } else { "s" },
        );
    }
    Ok(())
}
