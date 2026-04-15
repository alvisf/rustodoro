mod app;
mod store;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};

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
            if app.confirm_quit {
                match code {
                    KeyCode::Char('q') | KeyCode::Char('y') => app.should_quit = true,
                    KeyCode::Char('e') => app.confirm_quit_end_session(),
                    KeyCode::Esc | KeyCode::Char('n') => app.cancel_quit(),
                    _ => {}
                }
                continue;
            }

            match app.screen {
                Screen::Setup => match code {
                    KeyCode::Up | KeyCode::Char('k') => app.prev_field(),
                    KeyCode::Down | KeyCode::Char('j') => app.next_field(),
                    KeyCode::Left | KeyCode::Char('h') => app.decrement_field(),
                    KeyCode::Right | KeyCode::Char('l') => app.increment_field(),
                    KeyCode::Enter => app.start_timer(),
                    KeyCode::Esc => app.open_todo_list(true),
                    KeyCode::Char('q') => app.request_quit(),
                    _ => {}
                },
                Screen::TodoList => {
                    if app.todo_is_input_mode() {
                        match code {
                            KeyCode::Enter => app.todo_confirm_input(),
                            KeyCode::Esc => app.todo_cancel_input(),
                            KeyCode::Backspace => app.todo_input_backspace(),
                            KeyCode::Char(c) => app.todo_input_char(c),
                            _ => {}
                        }
                    } else {
                        match code {
                            KeyCode::Up | KeyCode::Char('k') => app.todo_up(),
                            KeyCode::Down | KeyCode::Char('j') => app.todo_down(),
                            KeyCode::Enter => app.todo_select(),
                            KeyCode::Char('a') => app.todo_start_add(),
                            KeyCode::Char('e') => app.todo_start_edit(),
                            KeyCode::Char('d') => app.todo_delete(),
                            KeyCode::Char(' ') => app.todo_toggle(),
                            KeyCode::Char('n') => app.todo_custom_task(),
                            KeyCode::Char('l') => app.open_daily_log(),
                            KeyCode::Char('b') => app.start_manual_break(),
                            KeyCode::Esc => app.todo_back(),
                            KeyCode::Char('q') => app.request_quit(),
                            _ => {}
                        }
                    }
                }
                Screen::TaskInput => match code {
                    KeyCode::Enter => app.submit_task(),
                    KeyCode::Esc => app.skip_task_input(),
                    KeyCode::Backspace => app.task_input_backspace(),
                    KeyCode::Char(c) => app.task_input_char(c),
                    _ => {}
                },
                Screen::Timer => match code {
                    _ if app.manual_break => match code {
                        KeyCode::Enter => app.end_manual_break(),
                        KeyCode::Char(' ') => app.toggle_pause(),
                        KeyCode::Char('l') => app.open_daily_log(),
                        KeyCode::Char('q') | KeyCode::Esc => app.request_quit(),
                        _ => {}
                    },
                    KeyCode::Char('q') | KeyCode::Esc => app.request_quit(),
                    KeyCode::Char(' ') => app.toggle_pause(),
                    KeyCode::Char('e') => app.end_task(),
                    KeyCode::Char('h') => app.help_others(),
                    KeyCode::Char('s') | KeyCode::Char('n') => app.skip_phase(),
                    KeyCode::Char('d') => app.distraction(),
                    KeyCode::Char('w') => app.shorten_work(),
                    KeyCode::Char('r') => app.rename_task(),
                    KeyCode::Char('l') => app.open_daily_log(),
                    KeyCode::Char('t') => app.open_todo_list(false),
                    KeyCode::Enter => app.confirm_break(),
                    _ => {}
                },
                Screen::NotesInput => match code {
                    KeyCode::Enter => app.submit_notes(),
                    KeyCode::Esc => app.skip_notes(),
                    KeyCode::Backspace => app.notes_input_backspace(),
                    KeyCode::Char(c) => app.notes_input_char(c),
                    _ => {}
                },
                Screen::DailyLog => match code {
                    KeyCode::Esc | KeyCode::Backspace => app.close_daily_log(),
                    KeyCode::Char('q') => app.request_quit(),
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

    if app.completed_work_sessions() > 0 {
        let completed = app.completed_work_sessions();
        println!(
            "👋 Done! Completed {} work session{}. Great job!",
            completed,
            if completed == 1 { "" } else { "s" },
        );
    }
    Ok(())
}
