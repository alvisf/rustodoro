use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::store::{self, DayStats};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Setup,
    TaskInput,
    Timer,
    DailyLog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Work,
    Break,
    LongBreak,
}

impl Phase {
    pub fn icon(self) -> &'static str {
        match self {
            Phase::Work => "🍅",
            Phase::Break => "☕",
            Phase::LongBreak => "🌴",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Phase::Work => "WORK",
            Phase::Break => "BREAK",
            Phase::LongBreak => "LONG BREAK",
        }
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.icon(), self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub session: u32,
    pub phase: Phase,
    pub elapsed_secs: u64,
    pub total_secs: u64,
    pub outcome: Outcome,
    pub task: String,
    pub start_time: String,
    pub end_time: String,
}

pub const SETUP_FIELD_COUNT: usize = 4;

pub struct App {
    pub work_secs: u64,
    pub break_secs: u64,
    pub long_break_secs: u64,
    pub sessions_before_long: u32,
    pub phase: Phase,
    pub session: u32,
    pub paused: bool,
    pub should_quit: bool,
    phase_start: Instant,
    pause_accumulated: Duration,
    pause_start: Option<Instant>,
    pub history: Vec<HistoryEntry>,
    pub screen: Screen,
    pub selected_field: usize,
    pub daily_stats: BTreeMap<String, DayStats>,
    persist: bool,
    pub current_task: String,
    pub task_input_buffer: String,
    phase_start_wall: String,
    overtime_notified: bool,
}

impl App {
    pub fn new() -> Self {
        let cfg = store::load_config();
        let mut app = Self::with_config_secs(
            cfg.work_secs,
            cfg.break_secs,
            cfg.long_break_secs,
            cfg.sessions_before_long,
        );
        app.persist = true;
        app.daily_stats = store::load_daily_stats();
        app
    }

    #[cfg(test)]
    pub fn with_config(
        work_mins: u64,
        break_mins: u64,
        long_break_mins: u64,
        sessions_before_long: u32,
    ) -> Self {
        Self::with_config_secs(
            work_mins * 60,
            break_mins * 60,
            long_break_mins * 60,
            sessions_before_long,
        )
    }

    fn with_config_secs(
        work_secs: u64,
        break_secs: u64,
        long_break_secs: u64,
        sessions_before_long: u32,
    ) -> Self {
        Self {
            work_secs,
            break_secs,
            long_break_secs,
            sessions_before_long: sessions_before_long.max(1),
            phase: Phase::Work,
            session: 1,
            paused: false,
            should_quit: false,
            phase_start: Instant::now(),
            pause_accumulated: Duration::ZERO,
            pause_start: None,
            history: Vec::new(),
            screen: Screen::Setup,
            selected_field: 0,
            daily_stats: BTreeMap::new(),
            persist: false,
            current_task: String::new(),
            task_input_buffer: String::new(),
            phase_start_wall: String::new(),
            overtime_notified: false,
        }
    }

    // -- Setup screen --

    pub fn next_field(&mut self) {
        self.selected_field = (self.selected_field + 1).min(SETUP_FIELD_COUNT - 1);
    }

    pub fn prev_field(&mut self) {
        self.selected_field = self.selected_field.saturating_sub(1);
    }

    pub fn increment_field(&mut self) {
        match self.selected_field {
            0 => self.work_secs = (self.work_secs + 60).min(120 * 60),
            1 => self.break_secs = (self.break_secs + 60).min(60 * 60),
            2 => self.long_break_secs = (self.long_break_secs + 60).min(60 * 60),
            3 => self.sessions_before_long = (self.sessions_before_long + 1).min(10),
            _ => {}
        }
    }

    pub fn decrement_field(&mut self) {
        match self.selected_field {
            0 => self.work_secs = self.work_secs.saturating_sub(60).max(60),
            1 => self.break_secs = self.break_secs.saturating_sub(60).max(60),
            2 => self.long_break_secs = self.long_break_secs.saturating_sub(60).max(60),
            3 => self.sessions_before_long = self.sessions_before_long.saturating_sub(1).max(1),
            _ => {}
        }
    }

    pub fn start_timer(&mut self) {
        if self.persist {
            store::save_config(
                self.work_secs,
                self.break_secs,
                self.long_break_secs,
                self.sessions_before_long,
            )
            .ok();
        }
        self.task_input_buffer.clear();
        self.screen = Screen::TaskInput;
    }

    // -- Task input --

    pub fn task_input_char(&mut self, c: char) {
        self.task_input_buffer.push(c);
    }

    pub fn task_input_backspace(&mut self) {
        self.task_input_buffer.pop();
    }

    pub fn submit_task(&mut self) {
        self.current_task = self.task_input_buffer.trim().to_string();
        self.task_input_buffer.clear();
        self.begin_work_phase();
    }

    pub fn skip_task_input(&mut self) {
        self.current_task = String::new();
        self.task_input_buffer.clear();
        self.begin_work_phase();
    }

    fn begin_work_phase(&mut self) {
        self.screen = Screen::Timer;
        self.phase_start = Instant::now();
        self.pause_accumulated = Duration::ZERO;
        self.pause_start = None;
        self.paused = false;
        self.phase_start_wall = store::local_time_str();
        self.overtime_notified = false;
    }

    // -- Timer --

    pub fn phase_total_secs(&self) -> u64 {
        match self.phase {
            Phase::Work => self.work_secs,
            Phase::Break => self.break_secs,
            Phase::LongBreak => self.long_break_secs,
        }
    }

    pub fn elapsed_secs(&self) -> u64 {
        let pause_extra = self
            .pause_start
            .map(|ps| ps.elapsed())
            .unwrap_or(Duration::ZERO);
        let total_paused = self.pause_accumulated + pause_extra;
        let raw = self
            .phase_start
            .elapsed()
            .saturating_sub(total_paused)
            .as_secs();
        if self.phase == Phase::Work {
            raw
        } else {
            raw.min(self.phase_total_secs())
        }
    }

    pub fn remaining_secs(&self) -> u64 {
        self.phase_total_secs().saturating_sub(self.elapsed_secs())
    }

    pub fn overtime_secs(&self) -> u64 {
        self.elapsed_secs().saturating_sub(self.phase_total_secs())
    }

    pub fn is_overtime(&self) -> bool {
        self.phase == Phase::Work && self.elapsed_secs() >= self.phase_total_secs()
    }

    pub fn progress(&self) -> f64 {
        let total = self.phase_total_secs() as f64;
        if total == 0.0 {
            return 1.0;
        }
        (self.elapsed_secs() as f64 / total).min(1.0)
    }

    pub fn tick(&mut self) {
        if !self.paused && self.remaining_secs() == 0 && self.phase != Phase::Work {
            self.finish_phase(Outcome::Completed);
        }

        if self.persist && self.phase == Phase::Work
            && self.is_overtime() && !self.overtime_notified
        {
            self.overtime_notified = true;
            store::send_notification("⏰ Time's up!", "Take a break when you're ready");
        }
    }

    pub fn toggle_pause(&mut self) {
        if self.paused {
            if let Some(ps) = self.pause_start.take() {
                self.pause_accumulated += ps.elapsed();
            }
            self.paused = false;
        } else {
            self.pause_start = Some(Instant::now());
            self.paused = true;
        }
    }

    pub fn skip_phase(&mut self) {
        self.finish_phase(Outcome::Skipped);
    }

    pub fn confirm_break(&mut self) {
        if self.phase != Phase::Work {
            return;
        }

        let elapsed = self.elapsed_secs();
        let total = self.phase_total_secs();
        let end_wall = store::local_time_str();
        let overtime = elapsed.saturating_sub(total);

        if elapsed > 0 {
            self.record_work(elapsed, &end_wall, Outcome::Completed);
        }

        self.history.push(HistoryEntry {
            session: self.session,
            phase: Phase::Work,
            elapsed_secs: elapsed,
            total_secs: total,
            outcome: Outcome::Completed,
            task: self.current_task.clone(),
            start_time: self.phase_start_wall.clone(),
            end_time: end_wall,
        });

        let break_dur = self.break_secs.max(1);
        let breaks_to_skip = (overtime / break_dur) as u32;

        if breaks_to_skip == 0 {
            self.advance_phase();
            self.reset_timer();
            if self.persist {
                let msg = match self.phase {
                    Phase::LongBreak => format!(
                        "Great work! Relax for {} min",
                        self.long_break_secs / 60,
                    ),
                    Phase::Break => format!(
                        "Relax for {} min",
                        self.break_secs / 60,
                    ),
                    _ => String::new(),
                };
                let title = match self.phase {
                    Phase::LongBreak => "🌴 Long break!",
                    Phase::Break => "☕ Break time!",
                    _ => "",
                };
                if !title.is_empty() {
                    store::send_notification(title, &msg);
                }
            }
        } else {
            self.session += breaks_to_skip;
            self.reset_timer();
            if self.screen == Screen::Timer {
                self.task_input_buffer.clear();
                self.screen = Screen::TaskInput;
            }
        }
    }

    fn finish_phase(&mut self, outcome: Outcome) {
        let elapsed = self.elapsed_secs();
        let total = self.phase_total_secs();
        let end_wall = store::local_time_str();
        let prev_phase = self.phase;

        if self.phase == Phase::Work && elapsed > 0 {
            self.record_work(elapsed, &end_wall, outcome);
        }

        let task = if self.phase == Phase::Work {
            self.current_task.clone()
        } else {
            String::new()
        };

        self.history.push(HistoryEntry {
            session: self.session,
            phase: self.phase,
            elapsed_secs: elapsed,
            total_secs: total,
            outcome,
            task,
            start_time: self.phase_start_wall.clone(),
            end_time: end_wall,
        });

        self.advance_phase();
        self.reset_timer();

        if self.persist {
            match (prev_phase, self.phase) {
                (Phase::Break | Phase::LongBreak, Phase::Work) => {
                    store::send_notification(
                        "🍅 Break's over!",
                        "Time to get back to work",
                    );
                }
                (Phase::Work, Phase::Break) => {
                    store::send_notification("☕ Break time!", &format!("Relax for {} min", self.break_secs / 60));
                }
                (Phase::Work, Phase::LongBreak) => {
                    store::send_notification("🌴 Long break!", &format!("Great work! Relax for {} min", self.long_break_secs / 60));
                }
                _ => {}
            }
        }

        if self.phase == Phase::Work && self.screen == Screen::Timer {
            self.task_input_buffer.clear();
            self.screen = Screen::TaskInput;
        }
    }

    fn advance_phase(&mut self) {
        match self.phase {
            Phase::Work => {
                if self.session.is_multiple_of(self.sessions_before_long) {
                    self.phase = Phase::LongBreak;
                } else {
                    self.phase = Phase::Break;
                }
            }
            Phase::Break | Phase::LongBreak => {
                self.session += 1;
                self.phase = Phase::Work;
            }
        }
    }

    fn reset_timer(&mut self) {
        self.phase_start = Instant::now();
        self.pause_accumulated = Duration::ZERO;
        self.pause_start = None;
        self.paused = false;
        self.phase_start_wall = store::local_time_str();
    }

    pub fn sessions_in_cycle(&self) -> u32 {
        ((self.session - 1) % self.sessions_before_long) + 1
    }

    pub fn completed_work_sessions(&self) -> usize {
        self.history
            .iter()
            .filter(|e| e.phase == Phase::Work && e.outcome == Outcome::Completed)
            .count()
    }

    fn record_work(&mut self, secs: u64, end_wall: &str, outcome: Outcome) {
        let today = store::local_date_str();
        if self.persist {
            store::save_work_entry_md(
                &today,
                &self.phase_start_wall,
                end_wall,
                secs,
                &self.current_task,
                outcome == Outcome::Completed,
            )
            .ok();
        }
        let entry = self.daily_stats.entry(today).or_default();
        entry.work_secs += secs;
        entry.sessions += 1;
    }

    pub fn save_current_work_if_needed(&mut self) {
        if self.phase == Phase::Work
            && matches!(self.screen, Screen::Timer | Screen::DailyLog)
        {
            let elapsed = self.elapsed_secs();
            if elapsed > 0 {
                let end_wall = store::local_time_str();
                self.record_work(elapsed, &end_wall, Outcome::Skipped);
            }
        }
    }

    pub fn today_work_secs(&self) -> u64 {
        let today = store::local_date_str();
        let saved = self.daily_stats.get(&today).map_or(0, |s| s.work_secs);
        let current = if self.phase == Phase::Work && self.screen == Screen::Timer {
            self.elapsed_secs()
        } else {
            0
        };
        saved + current
    }

    pub fn today_sessions(&self) -> u32 {
        let today = store::local_date_str();
        self.daily_stats.get(&today).map_or(0, |s| s.sessions)
    }
}

pub fn format_duration(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- format_duration --

    #[test]
    fn test_format_duration_zero() {
        assert_eq!(format_duration(0), "00:00");
    }

    #[test]
    fn test_format_duration_seconds_only() {
        assert_eq!(format_duration(45), "00:45");
    }

    #[test]
    fn test_format_duration_minutes_and_seconds() {
        assert_eq!(format_duration(754), "12:34");
    }

    #[test]
    fn test_format_duration_exact_minutes() {
        assert_eq!(format_duration(1500), "25:00");
    }

    // -- App construction --

    #[test]
    fn test_new_app_defaults() {
        let app = App::new();
        assert_eq!(app.work_secs, 1500);
        assert_eq!(app.break_secs, 300);
        assert_eq!(app.long_break_secs, 900);
        assert_eq!(app.sessions_before_long, 4);
        assert_eq!(app.session, 1);
        assert_eq!(app.phase, Phase::Work);
        assert_eq!(app.screen, Screen::Setup);
        assert_eq!(app.selected_field, 0);
        assert!(app.current_task.is_empty());
        assert!(app.task_input_buffer.is_empty());
        assert!(!app.paused);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_with_config_clamps_sessions() {
        let app = App::with_config(25, 5, 15, 0);
        assert_eq!(app.sessions_before_long, 1);
    }

    #[test]
    fn test_phase_display() {
        assert_eq!(format!("{}", Phase::Work), "🍅 WORK");
        assert_eq!(format!("{}", Phase::Break), "☕ BREAK");
        assert_eq!(format!("{}", Phase::LongBreak), "🌴 LONG BREAK");
    }

    // -- Setup navigation --

    #[test]
    fn test_next_field_clamps_at_max() {
        let mut app = App::new();
        for _ in 0..10 {
            app.next_field();
        }
        assert_eq!(app.selected_field, SETUP_FIELD_COUNT - 1);
    }

    #[test]
    fn test_prev_field_stops_at_zero() {
        let mut app = App::new();
        app.prev_field();
        assert_eq!(app.selected_field, 0);
    }

    #[test]
    fn test_increment_work() {
        let mut app = App::new();
        app.selected_field = 0;
        assert_eq!(app.work_secs, 25 * 60);
        app.increment_field();
        assert_eq!(app.work_secs, 26 * 60);
    }

    #[test]
    fn test_decrement_work_minimum() {
        let mut app = App::with_config(1, 5, 15, 4);
        app.selected_field = 0;
        app.decrement_field();
        assert_eq!(app.work_secs, 60);
    }

    #[test]
    fn test_increment_sessions_maximum() {
        let mut app = App::with_config(25, 5, 15, 10);
        app.selected_field = 3;
        app.increment_field();
        assert_eq!(app.sessions_before_long, 10);
    }

    #[test]
    fn test_start_timer_goes_to_task_input() {
        let mut app = App::new();
        assert_eq!(app.screen, Screen::Setup);
        app.start_timer();
        assert_eq!(app.screen, Screen::TaskInput);
    }

    // -- Task input --

    #[test]
    fn test_task_input_typing() {
        let mut app = App::new();
        app.start_timer();
        app.task_input_char('H');
        app.task_input_char('i');
        assert_eq!(app.task_input_buffer, "Hi");
        app.task_input_backspace();
        assert_eq!(app.task_input_buffer, "H");
    }

    #[test]
    fn test_submit_task_starts_timer() {
        let mut app = App::new();
        app.start_timer();
        app.task_input_char('X');
        app.submit_task();
        assert_eq!(app.screen, Screen::Timer);
        assert_eq!(app.current_task, "X");
        assert!(app.task_input_buffer.is_empty());
    }

    #[test]
    fn test_skip_task_input_starts_timer() {
        let mut app = App::new();
        app.start_timer();
        app.skip_task_input();
        assert_eq!(app.screen, Screen::Timer);
        assert!(app.current_task.is_empty());
    }

    // -- Timer logic --

    #[test]
    fn test_phase_total_secs() {
        let app = App::new();
        assert_eq!(app.phase_total_secs(), 1500);
    }

    #[test]
    fn test_work_does_not_auto_complete() {
        let mut app = App::with_config(0, 5, 15, 4);
        app.tick(); // Work remaining=0, but tick should NOT auto-finish
        assert_eq!(app.phase, Phase::Work);
        assert!(app.history.is_empty());
    }

    #[test]
    fn test_break_still_auto_completes() {
        let mut app = App::with_config(25, 0, 15, 4);
        app.skip_phase(); // Work -> Break (0 dur)
        app.tick(); // Break remaining=0 -> auto-completes
        assert_eq!(app.phase, Phase::Work);
        assert_eq!(app.session, 2);
    }

    #[test]
    fn test_skip_advances_phase() {
        let mut app = App::new();
        app.skip_phase();
        assert_eq!(app.phase, Phase::Break);
        assert_eq!(app.session, 1);

        app.skip_phase();
        assert_eq!(app.phase, Phase::Work);
        assert_eq!(app.session, 2);
    }

    #[test]
    fn test_skip_to_long_break() {
        let mut app = App::with_config(25, 5, 15, 2);

        app.skip_phase(); // Work -> Break
        assert_eq!(app.phase, Phase::Break);

        app.skip_phase(); // Break -> Work (session 2)
        assert_eq!(app.session, 2);

        app.skip_phase(); // Work -> LongBreak
        assert_eq!(app.phase, Phase::LongBreak);

        app.skip_phase(); // LongBreak -> Work (session 3)
        assert_eq!(app.phase, Phase::Work);
        assert_eq!(app.session, 3);
    }

    #[test]
    fn test_history_tracking() {
        let mut app = App::new();
        app.current_task = "Test task".to_string();
        app.skip_phase();

        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].phase, Phase::Work);
        assert_eq!(app.history[0].outcome, Outcome::Skipped);
        assert_eq!(app.history[0].session, 1);
        assert_eq!(app.history[0].task, "Test task");
    }

    #[test]
    fn test_toggle_pause() {
        let mut app = App::new();
        assert!(!app.paused);
        app.toggle_pause();
        assert!(app.paused);
        app.toggle_pause();
        assert!(!app.paused);
    }

    #[test]
    fn test_sessions_in_cycle() {
        let mut app = App::new();
        assert_eq!(app.sessions_in_cycle(), 1);

        app.skip_phase(); // Work -> Break
        app.skip_phase(); // Break -> Work (session 2)
        assert_eq!(app.sessions_in_cycle(), 2);
    }

    #[test]
    fn test_progress_zero_duration() {
        let app = App::with_config(0, 5, 15, 4);
        assert_eq!(app.progress(), 1.0);
    }

    // -- Overtime & confirm_break --

    #[test]
    fn test_is_overtime_zero_duration() {
        let app = App::with_config(0, 5, 15, 4);
        assert!(app.is_overtime());
    }

    #[test]
    fn test_not_overtime_during_countdown() {
        let app = App::with_config(25, 5, 15, 4);
        assert!(!app.is_overtime());
        assert_eq!(app.overtime_secs(), 0);
    }

    #[test]
    fn test_confirm_break_goes_to_break() {
        let mut app = App::with_config(0, 5, 15, 4);
        app.confirm_break();
        assert_eq!(app.phase, Phase::Break);
        assert_eq!(app.session, 1);
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].outcome, Outcome::Completed);
    }

    #[test]
    fn test_confirm_break_noop_during_break() {
        let mut app = App::with_config(25, 5, 15, 4);
        app.skip_phase(); // Work -> Break
        let session = app.session;
        app.confirm_break(); // Should do nothing
        assert_eq!(app.phase, Phase::Break);
        assert_eq!(app.session, session);
    }

    #[test]
    fn test_completed_work_sessions() {
        let mut app = App::with_config(0, 0, 0, 4);
        app.confirm_break(); // Work -> Break
        app.tick(); // Break (0 dur) completes -> Work
        app.confirm_break(); // Work -> Break
        assert_eq!(app.completed_work_sessions(), 2);
    }
}
