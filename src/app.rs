use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::store::{self, DayEntry, DayStats};

const SECONDS_PER_MINUTE: u64 = 60;
const MIN_WORK_SECS: u64 = SECONDS_PER_MINUTE;
const MAX_WORK_SECS: u64 = 120 * SECONDS_PER_MINUTE;
const MIN_BREAK_SECS: u64 = SECONDS_PER_MINUTE;
const MAX_BREAK_SECS: u64 = 60 * SECONDS_PER_MINUTE;
const MAX_SESSIONS: u32 = 10;
const DISTRACTION_SECS: u64 = 300;
const WRAP_UP_SECS: u64 = 300;
const SLEEP_THRESHOLD_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Onboarding,
    Setup,
    TaskInput,
    TodoList,
    NotesInput,
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
    Helping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoMode {
    Normal,
    Adding,
    Editing(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    Work,
    Break,
    LongBreak,
    SessionsBeforeLong,
}

impl SetupField {
    const ALL: [SetupField; 4] = [
        SetupField::Work,
        SetupField::Break,
        SetupField::LongBreak,
        SetupField::SessionsBeforeLong,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }

    pub fn next(self) -> Self {
        let i = self.index();
        Self::ALL[(i + 1).min(Self::ALL.len() - 1)]
    }

    pub fn prev(self) -> Self {
        let i = self.index();
        Self::ALL[i.saturating_sub(1)]
    }
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

pub struct TimerState {
    pub phase_start: Instant,
    pub pause_accumulated: Duration,
    pub pause_start: Option<Instant>,
    pub overtime_notified: bool,
    pub last_tick: Instant,
    pub phase_start_wall: String,
    pub phase_start_wall_12h: String,
}

impl TimerState {
    fn new() -> Self {
        Self {
            phase_start: Instant::now(),
            pause_accumulated: Duration::ZERO,
            pause_start: None,
            overtime_notified: false,
            last_tick: Instant::now(),
            phase_start_wall: String::new(),
            phase_start_wall_12h: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.phase_start = Instant::now();
        self.pause_accumulated = Duration::ZERO;
        self.pause_start = None;
        self.overtime_notified = false;
        self.last_tick = Instant::now();
        self.phase_start_wall = store::local_time_str();
        self.phase_start_wall_12h = store::local_time_12h();
    }
}

pub struct TodoState {
    pub items: Vec<store::TodoItem>,
    pub cursor: usize,
    pub mode: TodoMode,
    pub input_buffer: String,
    pub picking: bool,
}

impl TodoState {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            cursor: 0,
            mode: TodoMode::Normal,
            input_buffer: String::new(),
            picking: true,
        }
    }
}

pub struct PendingEntry {
    pub elapsed_secs: u64,
    pub end_wall_12h: String,
    pub is_helping: bool,
}

impl PendingEntry {
    fn new() -> Self {
        Self {
            elapsed_secs: 0,
            end_wall_12h: String::new(),
            is_helping: false,
        }
    }
}

pub struct DailyLogState {
    pub entries: BTreeMap<String, Vec<DayEntry>>,
    pub expanded: Vec<String>,
    pub cursor: usize,
    pub return_from: Screen,
}

impl DailyLogState {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            expanded: Vec::new(),
            cursor: 0,
            return_from: Screen::TodoList,
        }
    }
}

pub struct OnboardingState {
    pub input_buffer: String,
    pub error: Option<String>,
}

impl OnboardingState {
    fn new(default_dir: &std::path::Path) -> Self {
        Self {
            input_buffer: default_dir.display().to_string(),
            error: None,
        }
    }
}

pub struct App {
    /// User-configured default work phase duration (persisted).
    pub work_secs: u64,
    /// Current work phase's actual duration; starts equal to `work_secs` and
    /// may be reduced via `shorten_work` within a phase.
    current_work_secs: u64,
    pub break_secs: u64,
    pub long_break_secs: u64,
    pub sessions_before_long: u32,
    pub log_dir: std::path::PathBuf,
    pub phase: Phase,
    pub session: u32,
    pub paused: bool,
    pub should_quit: bool,
    pub confirm_quit: bool,
    pub timer: TimerState,
    pub history: Vec<HistoryEntry>,
    pub screen: Screen,
    pub selected_field: SetupField,
    pub daily_stats: BTreeMap<String, DayStats>,
    persist: bool,
    pub current_task: String,
    pub task_input_buffer: String,
    pub notes_input_buffer: String,
    pub renaming_task: bool,
    pub pending: PendingEntry,
    pub todo: TodoState,
    pub daily_log: DailyLogState,
    pub onboarding: OnboardingState,
    pub manual_break: bool,
    last_date: String,
}

impl App {
    pub fn new() -> Self {
        let is_first_run = !store::config_exists();
        let cfg = store::load_config();
        store::set_log_dir(cfg.log_dir.clone());

        let mut app = Self::with_config(&cfg);
        app.persist = true;
        if is_first_run {
            app.screen = Screen::Onboarding;
        } else {
            app.daily_stats = store::load_daily_stats();
            app.todo.items = store::load_todos();
        }
        app.last_date = store::local_date_str();
        app
    }

    #[cfg(test)]
    pub fn with_test_config(
        work_mins: u64,
        break_mins: u64,
        long_break_mins: u64,
        sessions_before_long: u32,
    ) -> Self {
        Self::with_config(&store::Config {
            work_secs: work_mins * 60,
            break_secs: break_mins * 60,
            long_break_secs: long_break_mins * 60,
            sessions_before_long,
            log_dir: store::default_log_dir(),
        })
    }

    fn with_config(cfg: &store::Config) -> Self {
        Self {
            work_secs: cfg.work_secs,
            current_work_secs: cfg.work_secs,
            break_secs: cfg.break_secs,
            long_break_secs: cfg.long_break_secs,
            sessions_before_long: cfg.sessions_before_long.max(1),
            log_dir: cfg.log_dir.clone(),
            phase: Phase::Work,
            session: 1,
            paused: false,
            should_quit: false,
            confirm_quit: false,
            timer: TimerState::new(),
            history: Vec::new(),
            screen: Screen::TodoList,
            selected_field: SetupField::Work,
            daily_stats: BTreeMap::new(),
            persist: false,
            current_task: String::new(),
            task_input_buffer: String::new(),
            notes_input_buffer: String::new(),
            renaming_task: false,
            pending: PendingEntry::new(),
            todo: TodoState::new(),
            daily_log: DailyLogState::new(),
            onboarding: OnboardingState::new(&cfg.log_dir),
            manual_break: false,
            last_date: String::new(),
        }
    }

    fn current_config(&self) -> store::Config {
        store::Config {
            work_secs: self.work_secs,
            break_secs: self.break_secs,
            long_break_secs: self.long_break_secs,
            sessions_before_long: self.sessions_before_long,
            log_dir: self.log_dir.clone(),
        }
    }

    // -- Onboarding screen --

    pub fn onboarding_input_char(&mut self, c: char) {
        self.onboarding.input_buffer.push(c);
        self.onboarding.error = None;
    }

    pub fn onboarding_input_backspace(&mut self) {
        self.onboarding.input_buffer.pop();
        self.onboarding.error = None;
    }

    pub fn onboarding_reset_to_default(&mut self) {
        self.onboarding.input_buffer = store::default_log_dir().display().to_string();
        self.onboarding.error = None;
    }

    /// Expands the input, creates the dir, persists the config, and transitions
    /// to TodoList. Sets `onboarding.error` and stays on Onboarding if anything fails.
    pub fn onboarding_confirm(&mut self) {
        let trimmed = self.onboarding.input_buffer.trim();
        if trimmed.is_empty() {
            self.onboarding.error = Some("Path cannot be empty".to_string());
            return;
        }

        let path = store::expand_home(trimmed);

        if let Err(err) = store::ensure_dir(&path) {
            self.onboarding.error = Some(format!("Could not create directory: {err}"));
            return;
        }

        self.log_dir = path.clone();
        store::set_log_dir(path);

        if self.persist
            && let Err(err) = store::save_config(&self.current_config())
        {
            self.onboarding.error = Some(format!("Could not save config: {err}"));
            return;
        }

        if self.persist {
            self.daily_stats = store::load_daily_stats();
            self.todo.items = store::load_todos();
        }

        self.screen = Screen::TodoList;
    }

    // -- Setup screen --

    pub fn next_field(&mut self) {
        self.selected_field = self.selected_field.next();
    }

    pub fn prev_field(&mut self) {
        self.selected_field = self.selected_field.prev();
    }

    pub fn increment_field(&mut self) {
        match self.selected_field {
            SetupField::Work => {
                self.work_secs = (self.work_secs + SECONDS_PER_MINUTE).min(MAX_WORK_SECS);
            }
            SetupField::Break => {
                self.break_secs = (self.break_secs + SECONDS_PER_MINUTE).min(MAX_BREAK_SECS);
            }
            SetupField::LongBreak => {
                self.long_break_secs =
                    (self.long_break_secs + SECONDS_PER_MINUTE).min(MAX_BREAK_SECS);
            }
            SetupField::SessionsBeforeLong => {
                self.sessions_before_long = (self.sessions_before_long + 1).min(MAX_SESSIONS);
            }
        }
    }

    pub fn decrement_field(&mut self) {
        match self.selected_field {
            SetupField::Work => {
                self.work_secs = self
                    .work_secs
                    .saturating_sub(SECONDS_PER_MINUTE)
                    .max(MIN_WORK_SECS);
            }
            SetupField::Break => {
                self.break_secs = self
                    .break_secs
                    .saturating_sub(SECONDS_PER_MINUTE)
                    .max(MIN_BREAK_SECS);
            }
            SetupField::LongBreak => {
                self.long_break_secs = self
                    .long_break_secs
                    .saturating_sub(SECONDS_PER_MINUTE)
                    .max(MIN_BREAK_SECS);
            }
            SetupField::SessionsBeforeLong => {
                self.sessions_before_long = self.sessions_before_long.saturating_sub(1).max(1);
            }
        }
    }

    pub fn start_timer(&mut self) {
        if self.persist {
            store::save_config(&self.current_config()).ok();
        }
        self.begin_work_phase();
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
        if self.renaming_task {
            self.renaming_task = false;
            self.screen = Screen::Timer;
        } else {
            self.screen = Screen::Setup;
        }
    }

    pub fn skip_task_input(&mut self) {
        self.task_input_buffer.clear();
        if self.renaming_task {
            self.renaming_task = false;
            self.screen = Screen::Timer;
        } else {
            self.return_to_task_picker();
        }
    }

    fn begin_work_phase(&mut self) {
        self.current_work_secs = self.work_secs;
        self.screen = Screen::Timer;
        self.timer.reset();
        self.paused = false;
    }

    // -- Timer --

    pub fn phase_total_secs(&self) -> u64 {
        match self.phase {
            Phase::Work => self.current_work_secs,
            Phase::Break => self.break_secs,
            Phase::LongBreak => self.long_break_secs,
        }
    }

    pub fn elapsed_secs(&self) -> u64 {
        let pause_extra = self
            .timer
            .pause_start
            .map(|ps| ps.elapsed())
            .unwrap_or(Duration::ZERO);
        let total_paused = self.timer.pause_accumulated + pause_extra;
        let raw = self
            .timer
            .phase_start
            .elapsed()
            .saturating_sub(total_paused)
            .as_secs();
        if self.phase == Phase::Work || self.manual_break {
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
        if self.manual_break {
            return 0.0;
        }
        let total = self.phase_total_secs() as f64;
        if total == 0.0 {
            return 1.0;
        }
        (self.elapsed_secs() as f64 / total).min(1.0)
    }

    pub fn tick(&mut self) {
        self.handle_sleep_gap();
        self.check_date_change();
        self.auto_complete_break();
        self.check_overtime_notification();
    }

    fn handle_sleep_gap(&mut self) {
        let gap = self.timer.last_tick.elapsed();
        self.timer.last_tick = Instant::now();

        if gap.as_secs() < SLEEP_THRESHOLD_SECS {
            return;
        }

        if self.is_in_active_work() && !self.paused {
            self.timer.pause_accumulated += gap.saturating_sub(Duration::from_secs(1));
            self.finish_phase(Outcome::Completed);
        }
    }

    fn check_date_change(&mut self) {
        if !self.persist {
            return;
        }
        let today = store::local_date_str();
        if !self.last_date.is_empty() && self.last_date != today {
            self.session = 1;
            self.history.clear();
            self.daily_stats = store::load_daily_stats();
            self.todo.items = store::load_todos();
        }
        self.last_date = today;
    }

    fn auto_complete_break(&mut self) {
        if !self.paused
            && self.remaining_secs() == 0
            && self.phase != Phase::Work
            && !self.manual_break
        {
            self.finish_phase(Outcome::Completed);
        }
    }

    fn check_overtime_notification(&mut self) {
        if self.persist
            && self.is_in_active_work()
            && self.is_overtime()
            && !self.timer.overtime_notified
        {
            self.timer.overtime_notified = true;
            store::send_notification("⏰ Time's up!", "Take a break when you're ready");
        }
    }

    pub fn toggle_pause(&mut self) {
        if self.paused {
            if let Some(ps) = self.timer.pause_start.take() {
                self.timer.pause_accumulated += ps.elapsed();
            }
            self.paused = false;
        } else {
            self.timer.pause_start = Some(Instant::now());
            self.paused = true;
        }
    }

    pub fn distraction(&mut self) {
        if !self.is_in_active_work() {
            return;
        }
        self.timer.pause_accumulated += Duration::from_secs(DISTRACTION_SECS);
    }

    pub fn shorten_work(&mut self) {
        if !self.is_in_active_work() {
            return;
        }
        self.current_work_secs = self
            .current_work_secs
            .saturating_sub(WRAP_UP_SECS)
            .max(MIN_WORK_SECS);
    }

    pub fn rename_task(&mut self) {
        if !self.is_in_active_work() {
            return;
        }
        self.renaming_task = true;
        self.task_input_buffer = self.current_task.clone();
        self.screen = Screen::TaskInput;
    }

    pub fn skip_phase(&mut self) {
        self.finish_phase(Outcome::Skipped);
    }

    pub fn confirm_break(&mut self) {
        if !self.is_in_active_work() {
            return;
        }

        let elapsed = self.elapsed_secs();

        self.record_and_log_work(elapsed, Outcome::Completed);
        self.advance_phase();
        self.reset_timer();
        self.notify_break_start();
    }

    fn finish_phase(&mut self, outcome: Outcome) {
        let elapsed = self.elapsed_secs();
        let prev_phase = self.phase;

        if self.phase == Phase::Work {
            self.record_and_log_work(elapsed, outcome);
        } else {
            self.log_phase_completion(elapsed, outcome);
        }

        self.advance_phase();
        self.reset_timer();

        match (prev_phase, self.phase) {
            (Phase::Break | Phase::LongBreak, Phase::Work) => self.notify_break_over(),
            (Phase::Work, _) => self.notify_break_start(),
            _ => {}
        }

        if self.phase == Phase::Work && self.screen == Screen::Timer {
            self.return_to_task_picker();
        }
    }

    fn advance_phase(&mut self) {
        match self.phase {
            Phase::Work => {
                if self.session % self.sessions_before_long == 0 {
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
        self.timer.reset();
        self.paused = false;
    }

    pub fn sessions_in_cycle(&self) -> u32 {
        ((self.session - 1) % self.sessions_before_long) + 1
    }

    pub fn completed_work_sessions(&self) -> usize {
        self.history
            .iter()
            .filter(|e| {
                e.phase == Phase::Work && matches!(e.outcome, Outcome::Completed | Outcome::Helping)
            })
            .count()
    }

    fn record_work(&mut self, secs: u64, end_wall: &str, outcome: Outcome) {
        let today = store::local_date_str();
        if self.persist {
            store::save_work_entry_md(&store::WorkEntry {
                date: &today,
                start_time: &self.timer.phase_start_wall_12h,
                end_time: end_wall,
                duration_secs: secs,
                task: &self.current_task,
                completed: outcome == Outcome::Completed,
                helping: false,
                notes: "",
            })
            .ok();
        }
        let entry = self.daily_stats.entry(today).or_default();
        entry.work_secs += secs;
        entry.sessions += 1;
    }

    fn record_and_log_work(&mut self, elapsed: u64, outcome: Outcome) {
        let end_wall = store::local_time_str();
        let end_wall_12h = store::local_time_12h();
        if elapsed > 0 {
            self.record_work(elapsed, &end_wall_12h, outcome);
        }
        self.history.push(HistoryEntry {
            session: self.session,
            phase: Phase::Work,
            elapsed_secs: elapsed,
            total_secs: self.phase_total_secs(),
            outcome,
            task: self.current_task.clone(),
            start_time: self.timer.phase_start_wall.clone(),
            end_time: end_wall,
        });
    }

    fn log_phase_completion(&mut self, elapsed: u64, outcome: Outcome) {
        self.history.push(HistoryEntry {
            session: self.session,
            phase: self.phase,
            elapsed_secs: elapsed,
            total_secs: self.phase_total_secs(),
            outcome,
            task: String::new(),
            start_time: self.timer.phase_start_wall.clone(),
            end_time: store::local_time_str(),
        });
    }

    fn notify_break_start(&self) {
        if !self.persist {
            return;
        }
        let (title, msg) = match self.phase {
            Phase::LongBreak => (
                "🌴 Long break!",
                format!(
                    "Great work! Relax for {} min",
                    self.long_break_secs / SECONDS_PER_MINUTE
                ),
            ),
            Phase::Break => (
                "☕ Break time!",
                format!("Relax for {} min", self.break_secs / SECONDS_PER_MINUTE),
            ),
            _ => return,
        };
        store::send_notification(title, &msg);
    }

    fn notify_break_over(&self) {
        if self.persist {
            store::send_notification("🍅 Break's over!", "Time to get back to work");
        }
    }

    fn is_in_active_work(&self) -> bool {
        self.phase == Phase::Work && !self.manual_break
    }

    // -- Todo list --

    pub fn return_to_task_picker(&mut self) {
        self.todo.picking = true;
        self.todo.cursor = 0;
        self.prepare_todo_list();
    }

    pub fn open_todo_manager(&mut self) {
        self.todo.picking = false;
        self.prepare_todo_list();
    }

    fn prepare_todo_list(&mut self) {
        self.todo.mode = TodoMode::Normal;
        self.todo.input_buffer.clear();
        if self.persist {
            self.todo.items = store::load_todos();
        }
        self.screen = Screen::TodoList;
    }

    pub fn todo_is_input_mode(&self) -> bool {
        !matches!(self.todo.mode, TodoMode::Normal)
    }

    pub fn todo_up(&mut self) {
        self.todo.cursor = self.todo.cursor.saturating_sub(1);
    }

    pub fn todo_down(&mut self) {
        if !self.todo.items.is_empty() {
            self.todo.cursor = (self.todo.cursor + 1).min(self.todo.items.len() - 1);
        }
    }

    pub fn todo_start_add(&mut self) {
        self.todo.input_buffer.clear();
        self.todo.mode = TodoMode::Adding;
    }

    pub fn todo_start_edit(&mut self) {
        if self.todo.items.is_empty() {
            return;
        }
        self.todo.input_buffer = self.todo.items[self.todo.cursor].text.clone();
        self.todo.mode = TodoMode::Editing(self.todo.cursor);
    }

    pub fn todo_input_char(&mut self, c: char) {
        self.todo.input_buffer.push(c);
    }

    pub fn todo_input_backspace(&mut self) {
        self.todo.input_buffer.pop();
    }

    pub fn todo_confirm_input(&mut self) {
        let text = self.todo.input_buffer.trim().to_string();
        if text.is_empty() {
            self.todo.mode = TodoMode::Normal;
            self.todo.input_buffer.clear();
            return;
        }
        match self.todo.mode {
            TodoMode::Adding => {
                self.todo.items.push(store::TodoItem { text, done: false });
                self.todo.cursor = self.todo.items.len() - 1;
            }
            TodoMode::Editing(idx) => {
                if idx < self.todo.items.len() {
                    self.todo.items[idx].text = text;
                }
            }
            TodoMode::Normal => {}
        }
        self.todo.mode = TodoMode::Normal;
        self.todo.input_buffer.clear();
        self.persist_todos();
    }

    pub fn todo_cancel_input(&mut self) {
        self.todo.mode = TodoMode::Normal;
        self.todo.input_buffer.clear();
    }

    pub fn todo_delete(&mut self) {
        if self.todo.items.is_empty() {
            return;
        }
        self.todo.items.remove(self.todo.cursor);
        if self.todo.cursor >= self.todo.items.len() && self.todo.cursor > 0 {
            self.todo.cursor -= 1;
        }
        self.persist_todos();
    }

    pub fn todo_toggle(&mut self) {
        if self.todo.items.is_empty() {
            return;
        }
        self.todo.items[self.todo.cursor].done = !self.todo.items[self.todo.cursor].done;
        self.persist_todos();
    }

    pub fn todo_select(&mut self) {
        if self.todo.items.is_empty() {
            return;
        }
        if self.todo.picking {
            self.current_task = self.todo.items[self.todo.cursor].text.clone();
            self.screen = Screen::Setup;
        } else {
            self.todo_toggle();
        }
    }

    pub fn todo_back(&mut self) {
        if self.todo.picking {
            self.should_quit = true;
        } else {
            self.screen = Screen::Timer;
        }
    }

    pub fn todo_custom_task(&mut self) {
        if !self.todo.picking {
            return;
        }
        self.task_input_buffer.clear();
        self.screen = Screen::TaskInput;
    }

    pub fn open_daily_log(&mut self) {
        self.daily_log.return_from = self.screen;
        self.daily_log.cursor = 0;
        self.daily_log.expanded.clear();
        if self.persist {
            self.daily_log.entries = store::load_daily_entries();
        }
        self.screen = Screen::DailyLog;
    }

    pub fn close_daily_log(&mut self) {
        self.screen = self.daily_log.return_from;
    }

    pub fn daily_log_cursor_up(&mut self) {
        self.daily_log.cursor = self.daily_log.cursor.saturating_sub(1);
    }

    pub fn daily_log_cursor_down(&mut self) {
        let today = store::local_date_str();
        let past_count = self.daily_stats.keys().filter(|d| **d != today).count();
        if past_count > 0 && self.daily_log.cursor < past_count - 1 {
            self.daily_log.cursor += 1;
        }
    }

    pub fn daily_log_toggle_expand(&mut self) {
        let today = store::local_date_str();
        let past_days: Vec<_> = self
            .daily_stats
            .keys()
            .rev()
            .filter(|d| **d != today)
            .cloned()
            .collect();
        if let Some(date) = past_days.get(self.daily_log.cursor) {
            if let Some(pos) = self.daily_log.expanded.iter().position(|d| d == date) {
                self.daily_log.expanded.remove(pos);
            } else {
                self.daily_log.expanded.push(date.clone());
            }
        }
    }

    pub fn request_quit(&mut self) {
        self.confirm_quit = true;
    }

    pub fn cancel_quit(&mut self) {
        self.confirm_quit = false;
    }

    pub fn confirm_quit_end_session(&mut self) {
        self.confirm_quit = false;
        self.end_task();
    }

    pub fn has_active_work_session(&self) -> bool {
        self.is_in_active_work() && matches!(self.screen, Screen::Timer | Screen::DailyLog)
    }

    pub fn start_manual_break(&mut self) {
        self.manual_break = true;
        self.screen = Screen::Timer;
        self.timer.reset();
        self.paused = false;
    }

    pub fn end_manual_break(&mut self) {
        self.manual_break = false;
        self.return_to_task_picker();
    }

    fn persist_todos(&self) {
        if self.persist {
            store::save_todos(&self.todo.items).ok();
        }
    }

    // -- End task (early completion with notes) --

    pub fn end_task(&mut self) {
        self.complete_work_session(Outcome::Completed);
    }

    pub fn help_others(&mut self) {
        self.complete_work_session(Outcome::Helping);
    }

    fn complete_work_session(&mut self, outcome: Outcome) {
        if !self.is_in_active_work() {
            return;
        }
        let elapsed = self.elapsed_secs();
        let end_wall = store::local_time_str();

        self.pending.elapsed_secs = elapsed;
        self.pending.end_wall_12h = store::local_time_12h();
        self.pending.is_helping = outcome == Outcome::Helping;

        self.history.push(HistoryEntry {
            session: self.session,
            phase: Phase::Work,
            elapsed_secs: elapsed,
            total_secs: self.phase_total_secs(),
            outcome,
            task: self.current_task.clone(),
            start_time: self.timer.phase_start_wall.clone(),
            end_time: end_wall,
        });

        let today = store::local_date_str();
        let entry = self.daily_stats.entry(today).or_default();
        match outcome {
            Outcome::Helping => entry.helping_secs += elapsed,
            _ => entry.work_secs += elapsed,
        }
        entry.sessions += 1;

        self.notes_input_buffer.clear();
        self.screen = Screen::NotesInput;
    }

    pub fn notes_input_char(&mut self, c: char) {
        self.notes_input_buffer.push(c);
    }

    pub fn notes_input_backspace(&mut self) {
        self.notes_input_buffer.pop();
    }

    pub fn submit_notes(&mut self) {
        let notes = self.notes_input_buffer.trim().to_string();
        self.save_pending_entry(&notes);
        self.notes_input_buffer.clear();
        self.transition_to_break();
    }

    pub fn skip_notes(&mut self) {
        self.save_pending_entry("");
        self.notes_input_buffer.clear();
        self.transition_to_break();
    }

    fn save_pending_entry(&self, notes: &str) {
        if self.persist {
            let today = store::local_date_str();
            store::save_work_entry_md(&store::WorkEntry {
                date: &today,
                start_time: &self.timer.phase_start_wall_12h,
                end_time: &self.pending.end_wall_12h,
                duration_secs: self.pending.elapsed_secs,
                task: &self.current_task,
                completed: true,
                helping: self.pending.is_helping,
                notes,
            })
            .ok();
        }
    }

    fn transition_to_break(&mut self) {
        self.advance_phase();
        self.reset_timer();
        self.screen = Screen::Timer;
        self.notify_break_start();
    }

    pub fn save_current_work_if_needed(&mut self) {
        if self.screen == Screen::NotesInput && self.pending.elapsed_secs > 0 {
            self.save_pending_entry("");
            return;
        }
        if self.has_active_work_session() {
            let elapsed = self.elapsed_secs();
            if elapsed > 0 {
                let end_wall_12h = store::local_time_12h();
                self.record_work(elapsed, &end_wall_12h, Outcome::Skipped);
            }
        }
    }

    pub fn today_work_secs(&self) -> u64 {
        let today = store::local_date_str();
        let saved = self.daily_stats.get(&today).map_or(0, |s| s.work_secs);
        let current = if self.is_in_active_work() && self.screen == Screen::Timer {
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

    pub fn today_helping_secs(&self) -> u64 {
        let today = store::local_date_str();
        self.daily_stats.get(&today).map_or(0, |s| s.helping_secs)
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
        let app = App::with_test_config(25, 5, 15, 4);
        assert_eq!(app.work_secs, 25 * 60);
        assert_eq!(app.break_secs, 5 * 60);
        assert_eq!(app.long_break_secs, 15 * 60);
        assert_eq!(app.sessions_before_long, 4);
        assert_eq!(app.session, 1);
        assert_eq!(app.phase, Phase::Work);
        assert_eq!(app.screen, Screen::TodoList);
        assert_eq!(app.selected_field, SetupField::Work);
        assert!(app.current_task.is_empty());
        assert!(app.task_input_buffer.is_empty());
        assert!(!app.paused);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_with_config_clamps_sessions() {
        let app = App::with_test_config(25, 5, 15, 0);
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
        assert_eq!(app.selected_field, SetupField::SessionsBeforeLong);
    }

    #[test]
    fn test_prev_field_stops_at_zero() {
        let mut app = App::new();
        app.prev_field();
        assert_eq!(app.selected_field, SetupField::Work);
    }

    #[test]
    fn test_increment_work() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.selected_field = SetupField::Work;
        assert_eq!(app.work_secs, 25 * 60);
        app.increment_field();
        assert_eq!(app.work_secs, 26 * 60);
    }

    #[test]
    fn test_decrement_work_minimum() {
        let mut app = App::with_test_config(1, 5, 15, 4);
        app.selected_field = SetupField::Work;
        app.decrement_field();
        assert_eq!(app.work_secs, 60);
    }

    #[test]
    fn test_increment_sessions_maximum() {
        let mut app = App::with_test_config(25, 5, 15, 10);
        app.selected_field = SetupField::SessionsBeforeLong;
        app.increment_field();
        assert_eq!(app.sessions_before_long, 10);
    }

    #[test]
    fn test_start_timer_begins_work() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        assert_eq!(app.screen, Screen::TodoList);
        app.start_timer();
        assert_eq!(app.screen, Screen::Timer);
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
        app.task_input_char('X');
        app.submit_task();
        assert_eq!(app.screen, Screen::Setup);
        assert_eq!(app.current_task, "X");
        assert!(app.task_input_buffer.is_empty());
    }

    #[test]
    fn test_skip_task_input_returns_to_todo() {
        let mut app = App::new();
        app.skip_task_input();
        assert_eq!(app.screen, Screen::TodoList);
    }

    // -- Timer logic --

    #[test]
    fn test_phase_total_secs() {
        let app = App::with_test_config(25, 5, 15, 4);
        assert_eq!(app.phase_total_secs(), 1500);
    }

    #[test]
    fn test_work_does_not_auto_complete() {
        let mut app = App::with_test_config(0, 5, 15, 4);
        app.tick(); // Work remaining=0, but tick should NOT auto-finish
        assert_eq!(app.phase, Phase::Work);
        assert!(app.history.is_empty());
    }

    #[test]
    fn test_break_still_auto_completes() {
        let mut app = App::with_test_config(25, 0, 15, 4);
        app.skip_phase(); // Work -> Break (0 dur)
        app.tick(); // Break remaining=0 -> auto-completes
        assert_eq!(app.phase, Phase::Work);
        assert_eq!(app.session, 2);
    }

    #[test]
    fn test_skip_advances_phase() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.skip_phase();
        assert_eq!(app.phase, Phase::Break);
        assert_eq!(app.session, 1);

        app.skip_phase();
        assert_eq!(app.phase, Phase::Work);
        assert_eq!(app.session, 2);
    }

    #[test]
    fn test_skip_to_long_break() {
        let mut app = App::with_test_config(25, 5, 15, 2);

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
        let mut app = App::with_test_config(25, 5, 15, 4);
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
        let mut app = App::with_test_config(25, 5, 15, 4);
        assert!(!app.paused);
        app.toggle_pause();
        assert!(app.paused);
        app.toggle_pause();
        assert!(!app.paused);
    }

    #[test]
    fn test_sessions_in_cycle() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        assert_eq!(app.sessions_in_cycle(), 1);

        app.skip_phase(); // Work -> Break
        app.skip_phase(); // Break -> Work (session 2)
        assert_eq!(app.sessions_in_cycle(), 2);
    }

    #[test]
    fn test_progress_zero_duration() {
        let app = App::with_test_config(0, 5, 15, 4);
        assert_eq!(app.progress(), 1.0);
    }

    #[test]
    fn test_distraction_adds_pause() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        assert_eq!(app.timer.pause_accumulated, Duration::ZERO);
        app.distraction();
        assert_eq!(app.timer.pause_accumulated, Duration::from_secs(300));
        app.distraction();
        assert_eq!(app.timer.pause_accumulated, Duration::from_secs(600));
    }

    #[test]
    fn test_distraction_noop_during_break() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.skip_phase(); // Work -> Break
        app.distraction();
        assert_eq!(app.timer.pause_accumulated, Duration::ZERO);
    }

    // -- Overtime & confirm_break --

    #[test]
    fn test_is_overtime_zero_duration() {
        let app = App::with_test_config(0, 5, 15, 4);
        assert!(app.is_overtime());
    }

    #[test]
    fn test_not_overtime_during_countdown() {
        let app = App::with_test_config(25, 5, 15, 4);
        assert!(!app.is_overtime());
        assert_eq!(app.overtime_secs(), 0);
    }

    #[test]
    fn test_confirm_break_goes_to_break() {
        let mut app = App::with_test_config(0, 5, 15, 4);
        app.confirm_break();
        assert_eq!(app.phase, Phase::Break);
        assert_eq!(app.session, 1);
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].outcome, Outcome::Completed);
    }

    #[test]
    fn test_confirm_break_noop_during_break() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.skip_phase(); // Work -> Break
        let session = app.session;
        app.confirm_break(); // Should do nothing
        assert_eq!(app.phase, Phase::Break);
        assert_eq!(app.session, session);
    }

    #[test]
    fn test_confirm_break_never_skips_long_break() {
        let mut app = App::with_test_config(0, 1, 15, 2);
        // Session 1 Work → Break (not long since 1 is not multiple of 2)
        app.confirm_break();
        assert_eq!(app.phase, Phase::Break);
        app.skip_phase(); // Break → Work (session 2)
        // Session 2 Work with 0-dur → overtime triggers breaks_to_skip > 0
        app.confirm_break();
        assert_eq!(app.phase, Phase::LongBreak);
    }

    #[test]
    fn test_completed_work_sessions() {
        let mut app = App::with_test_config(0, 0, 0, 4);
        app.confirm_break(); // Work -> Break
        app.tick(); // Break (0 dur) completes -> Work
        app.confirm_break(); // Work -> Break
        assert_eq!(app.completed_work_sessions(), 2);
    }

    // -- End task & notes --

    #[test]
    fn test_end_task_goes_to_notes_input() {
        let mut app = App::with_test_config(0, 5, 15, 4);
        app.current_task = "X".to_string();
        app.start_timer();
        app.end_task();
        assert_eq!(app.screen, Screen::NotesInput);
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].outcome, Outcome::Completed);
        assert_eq!(app.history[0].task, "X");
    }

    #[test]
    fn test_end_task_noop_during_break() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.skip_phase(); // Work -> Break
        app.end_task();
        assert_eq!(app.phase, Phase::Break);
        assert_ne!(app.screen, Screen::NotesInput);
    }

    #[test]
    fn test_submit_notes_goes_to_break() {
        let mut app = App::with_test_config(0, 5, 15, 4);
        app.start_timer();
        app.end_task();
        assert_eq!(app.screen, Screen::NotesInput);
        app.notes_input_char('g');
        app.notes_input_char('o');
        app.notes_input_char('o');
        app.notes_input_char('d');
        app.submit_notes();
        assert_eq!(app.phase, Phase::Break);
        assert_eq!(app.screen, Screen::Timer);
        assert!(app.notes_input_buffer.is_empty());
    }

    #[test]
    fn test_skip_notes_goes_to_break() {
        let mut app = App::with_test_config(0, 5, 15, 4);
        app.start_timer();
        app.end_task();
        app.skip_notes();
        assert_eq!(app.phase, Phase::Break);
        assert_eq!(app.screen, Screen::Timer);
        assert!(app.notes_input_buffer.is_empty());
    }

    // -- Todo list --

    #[test]
    fn test_open_todo_list_picking() {
        let app = App::with_test_config(25, 5, 15, 4);
        assert_eq!(app.screen, Screen::TodoList);
        assert!(app.todo.picking);
    }

    #[test]
    fn test_open_todo_list_manage() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.open_todo_manager();
        assert_eq!(app.screen, Screen::TodoList);
        assert!(!app.todo.picking);
    }

    #[test]
    fn test_todo_add_and_navigate() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.return_to_task_picker();
        app.todo_start_add();
        assert!(app.todo_is_input_mode());
        app.todo_input_char('T');
        app.todo_input_char('a');
        app.todo_input_char('s');
        app.todo_input_char('k');
        app.todo_confirm_input();
        assert!(!app.todo_is_input_mode());
        assert_eq!(app.todo.items.len(), 1);
        assert_eq!(app.todo.items[0].text, "Task");
        app.todo_start_add();
        app.todo_input_char('B');
        app.todo_confirm_input();
        assert_eq!(app.todo.items.len(), 2);
        app.todo.cursor = 0;
        app.todo_down();
        assert_eq!(app.todo.cursor, 1);
        app.todo_up();
        assert_eq!(app.todo.cursor, 0);
    }

    #[test]
    fn test_todo_delete() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.todo.items.push(store::TodoItem {
            text: "A".into(),
            done: false,
        });
        app.todo.items.push(store::TodoItem {
            text: "B".into(),
            done: false,
        });
        app.todo.cursor = 1;
        app.todo_delete();
        assert_eq!(app.todo.items.len(), 1);
        assert_eq!(app.todo.cursor, 0);
    }

    #[test]
    fn test_todo_toggle() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.todo.items.push(store::TodoItem {
            text: "A".into(),
            done: false,
        });
        app.todo_toggle();
        assert!(app.todo.items[0].done);
        app.todo_toggle();
        assert!(!app.todo.items[0].done);
    }

    #[test]
    fn test_todo_select_starts_work() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.todo.picking = true;
        app.todo.items.push(store::TodoItem {
            text: "My task".into(),
            done: false,
        });
        app.todo_select();
        assert_eq!(app.current_task, "My task");
        assert_eq!(app.screen, Screen::Setup);
    }

    #[test]
    fn test_todo_edit() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.todo.items.push(store::TodoItem {
            text: "Old".into(),
            done: false,
        });
        app.todo_start_edit();
        assert!(app.todo_is_input_mode());
        assert_eq!(app.todo.input_buffer, "Old");
        app.todo.input_buffer.clear();
        app.todo_input_char('N');
        app.todo_input_char('e');
        app.todo_input_char('w');
        app.todo_confirm_input();
        assert_eq!(app.todo.items[0].text, "New");
    }

    #[test]
    fn test_todo_back_picking_quits() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.todo.picking = true;
        app.todo_back();
        assert!(app.should_quit);
    }

    #[test]
    fn test_todo_back_manage_returns_to_timer() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.open_todo_manager();
        app.todo_back();
        assert_eq!(app.screen, Screen::Timer);
    }

    #[test]
    fn test_todo_custom_task() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.todo.picking = true;
        app.todo_custom_task();
        assert_eq!(app.screen, Screen::TaskInput);
    }

    // -- Helping --

    #[test]
    fn test_help_others_goes_to_notes_input() {
        let mut app = App::with_test_config(0, 5, 15, 4);
        app.start_timer();
        app.help_others();
        assert_eq!(app.screen, Screen::NotesInput);
        assert!(app.pending.is_helping);
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].outcome, Outcome::Helping);
    }

    #[test]
    fn test_help_others_tracks_helping_secs() {
        let mut app = App::with_test_config(0, 5, 15, 4);
        app.start_timer();
        app.help_others();
        assert_eq!(app.today_work_secs(), 0);
    }

    #[test]
    fn test_help_others_noop_during_break() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.skip_phase();
        app.help_others();
        assert_ne!(app.screen, Screen::NotesInput);
    }

    // -- Quit confirmation --

    #[test]
    fn test_request_quit_sets_flag() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        assert!(!app.confirm_quit);
        app.request_quit();
        assert!(app.confirm_quit);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_cancel_quit_clears_flag() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.request_quit();
        app.cancel_quit();
        assert!(!app.confirm_quit);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_confirm_quit_end_session_during_work() {
        let mut app = App::with_test_config(0, 5, 15, 4);
        app.start_timer();
        app.request_quit();
        app.confirm_quit_end_session();
        assert!(!app.confirm_quit);
        assert_eq!(app.screen, Screen::NotesInput);
    }

    #[test]
    fn test_has_active_work_session() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        assert!(!app.has_active_work_session());
        app.start_timer();
        assert!(app.has_active_work_session());
        app.skip_phase(); // Work -> Break
        assert!(!app.has_active_work_session());
    }

    // -- Manual break --

    #[test]
    fn test_start_manual_break() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.start_manual_break();
        assert!(app.manual_break);
        assert_eq!(app.screen, Screen::Timer);
        assert!(!app.has_active_work_session());
    }

    #[test]
    fn test_end_manual_break() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.start_manual_break();
        app.end_manual_break();
        assert!(!app.manual_break);
        assert_eq!(app.screen, Screen::TodoList);
    }

    #[test]
    fn test_manual_break_progress_zero() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.start_manual_break();
        assert_eq!(app.progress(), 0.0);
    }

    // -- Rename task --

    #[test]
    fn test_rename_task() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.current_task = "Old".to_string();
        app.start_timer();
        app.rename_task();
        assert!(app.renaming_task);
        assert_eq!(app.screen, Screen::TaskInput);
        assert_eq!(app.task_input_buffer, "Old");
    }

    #[test]
    fn test_rename_submit_returns_to_timer() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.start_timer();
        app.rename_task();
        app.task_input_buffer.clear();
        app.task_input_char('N');
        app.task_input_char('e');
        app.task_input_char('w');
        app.submit_task();
        assert_eq!(app.screen, Screen::Timer);
        assert_eq!(app.current_task, "New");
        assert!(!app.renaming_task);
    }

    #[test]
    fn test_rename_skip_returns_to_timer() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.current_task = "Keep".to_string();
        app.start_timer();
        app.rename_task();
        app.skip_task_input();
        assert_eq!(app.screen, Screen::Timer);
        assert_eq!(app.current_task, "Keep");
        assert!(!app.renaming_task);
    }

    #[test]
    fn test_rename_noop_during_break() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.skip_phase(); // Work -> Break
        app.rename_task();
        assert!(!app.renaming_task);
        assert_ne!(app.screen, Screen::TaskInput);
    }

    #[test]
    fn test_sleep_during_work_switches_to_break() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.submit_task();
        app.start_timer();
        assert_eq!(app.phase, Phase::Work);
        assert_eq!(app.session, 1);

        app.timer.last_tick = Instant::now() - Duration::from_secs(60);
        app.tick();

        assert!(matches!(app.phase, Phase::Break | Phase::LongBreak));
        let work = app
            .history
            .iter()
            .filter(|e| e.phase == Phase::Work)
            .collect::<Vec<_>>();
        assert_eq!(work.len(), 1);
        assert!(work[0].elapsed_secs < 5);
    }

    #[test]
    fn test_sleep_during_break_no_crash() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.submit_task();
        app.start_timer();
        app.skip_phase();
        assert!(matches!(app.phase, Phase::Break | Phase::LongBreak));

        app.timer.last_tick = Instant::now() - Duration::from_secs(60);
        app.tick();

        // Break should remain or auto-complete — no panic
        assert!(!app.should_quit);
    }

    #[test]
    fn test_sleep_during_paused_work_no_transition() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.submit_task();
        app.start_timer();
        app.toggle_pause();
        assert!(app.paused);

        app.timer.last_tick = Instant::now() - Duration::from_secs(60);
        app.tick();

        assert_eq!(app.phase, Phase::Work);
        assert!(app.paused);
    }

    #[test]
    fn test_normal_tick_no_sleep_detection() {
        let mut app = App::with_test_config(25, 5, 15, 4);
        app.submit_task();
        app.start_timer();
        app.tick();
        assert_eq!(app.phase, Phase::Work);
    }
}
