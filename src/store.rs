use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::BufRead;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::SystemTime;

const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_HOUR: u64 = 3600;

#[derive(Debug, Default, Clone)]
pub struct DayStats {
    pub work_secs: u64,
    pub helping_secs: u64,
    pub sessions: u32,
}

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub text: String,
    pub done: bool,
}

const LOG_DIR: &str = "/Users/alvisf/Documents/Notes/daily-logs";

fn config_path() -> PathBuf {
    log_dir().join(".pomodoro.conf")
}

fn log_dir() -> PathBuf {
    PathBuf::from(LOG_DIR)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn local_tm(secs: i64) -> Option<libc::tm> {
    unsafe {
        let time_t = secs as libc::time_t;
        let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
        if libc::localtime_r(&time_t, tm.as_mut_ptr()).is_null() {
            return None;
        }
        Some(tm.assume_init())
    }
}

pub fn local_time_str() -> String {
    let Some(tm) = local_tm(unix_now() as i64) else {
        return "??:??".to_string();
    };
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

pub fn local_time_12h() -> String {
    let Some(tm) = local_tm(unix_now() as i64) else {
        return "??:?? ??".to_string();
    };
    let hour = tm.tm_hour;
    let suffix = if hour < 12 { "AM" } else { "PM" };
    let h12 = if hour == 0 {
        12
    } else if hour > 12 {
        hour - 12
    } else {
        hour
    };
    format!("{h12}:{:02} {suffix}", tm.tm_min)
}

pub fn local_date_str() -> String {
    local_date_for_offset(0)
}

pub fn local_date_for_offset(offset_days: i64) -> String {
    let secs = unix_now() as i64 + offset_days * SECONDS_PER_DAY;
    let Some(tm) = local_tm(secs) else {
        return "????-??-??".to_string();
    };
    format!(
        "{:04}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
    )
}

pub fn yesterday_str() -> String {
    local_date_for_offset(-1)
}

pub fn format_hours(total_secs: u64) -> String {
    let h = total_secs / SECONDS_PER_HOUR;
    let m = (total_secs % SECONDS_PER_HOUR) / 60;
    format!("{h}h {m:02}m")
}

fn format_mmss(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m:02}:{s:02}")
}

fn quarter_for_month(month: u32) -> u32 {
    (month - 1) / 3 + 1
}

fn quarterly_filename(date: &str) -> String {
    if date.len() < 7 {
        return "unknown-Q1.md".to_string();
    }
    let month: u32 = date[5..7].parse().unwrap_or(1);
    let year = &date[0..4];
    format!("{year}-Q{}.md", quarter_for_month(month))
}

fn quarterly_title(date: &str) -> String {
    if date.len() < 7 {
        return "Unknown Q1".to_string();
    }
    let month: u32 = date[5..7].parse().unwrap_or(1);
    let year = &date[0..4];
    format!("{year} Q{}", quarter_for_month(month))
}

#[allow(clippy::too_many_arguments)]
pub fn save_work_entry_md(
    date: &str,
    start_time: &str,
    end_time: &str,
    duration_secs: u64,
    task: &str,
    completed: bool,
    helping: bool,
    notes: &str,
) -> io::Result<()> {
    let dir = log_dir();
    fs::create_dir_all(&dir)?;

    let path = dir.join(quarterly_filename(date));
    let is_new = !path.exists();

    let date_header = format!("## {date}");
    let needs_date_header = if is_new {
        true
    } else {
        let contents = fs::read_to_string(&path).unwrap_or_default();
        !contents.contains(&date_header)
    };

    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;

    if is_new {
        writeln!(file, "# {}\n", quarterly_title(date))?;
    }

    if needs_date_header {
        writeln!(file, "\n{date_header}\n")?;
    }

    let duration = format_mmss(duration_secs);
    let mark = if helping {
        "[h]"
    } else if completed {
        "[x]"
    } else {
        "[ ]"
    };

    if task.is_empty() {
        writeln!(file, "- {mark} {start_time} – {end_time} ({duration})")?;
    } else {
        writeln!(
            file,
            "- {mark} {start_time} – {end_time} ({duration}) {task}"
        )?;
    }
    if !notes.is_empty() {
        writeln!(file, "  > {notes}")?;
    }
    Ok(())
}

pub struct Config {
    pub work_secs: u64,
    pub break_secs: u64,
    pub long_break_secs: u64,
    pub sessions_before_long: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            work_secs: 25 * 60,
            break_secs: 5 * 60,
            long_break_secs: 15 * 60,
            sessions_before_long: 4,
        }
    }
}

pub fn load_config() -> Config {
    let path = config_path();
    let Ok(file) = fs::File::open(&path) else {
        return Config::default();
    };
    let mut cfg = Config::default();
    for line in io::BufReader::new(file).lines() {
        let Ok(line) = line else { continue };
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "work_secs" => {
                if let Ok(v) = val.trim().parse() {
                    cfg.work_secs = v;
                }
            }
            "break_secs" => {
                if let Ok(v) = val.trim().parse() {
                    cfg.break_secs = v;
                }
            }
            "long_break_secs" => {
                if let Ok(v) = val.trim().parse() {
                    cfg.long_break_secs = v;
                }
            }
            "sessions_before_long" => {
                if let Ok(v) = val.trim().parse() {
                    cfg.sessions_before_long = v;
                }
            }
            _ => {}
        }
    }
    cfg
}

pub fn save_config(
    work_secs: u64,
    break_secs: u64,
    long_break_secs: u64,
    sessions_before_long: u32,
) -> io::Result<()> {
    let dir = log_dir();
    fs::create_dir_all(&dir)?;
    let contents = format!(
        "work_secs={work_secs}\nbreak_secs={break_secs}\n\
         long_break_secs={long_break_secs}\nsessions_before_long={sessions_before_long}\n"
    );
    fs::write(config_path(), contents)
}

fn todo_path() -> PathBuf {
    log_dir().join(".pomodoro_todos")
}

fn parse_todo_line(line: &str) -> Option<TodoItem> {
    if let Some(text) = line.strip_prefix("- [x] ") {
        Some(TodoItem {
            text: text.to_string(),
            done: true,
        })
    } else {
        line.strip_prefix("- [ ] ").map(|text| TodoItem {
            text: text.to_string(),
            done: false,
        })
    }
}

pub fn load_todos() -> Vec<TodoItem> {
    let path = todo_path();
    let Ok(contents) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    contents.lines().filter_map(parse_todo_line).collect()
}

pub fn save_todos(todos: &[TodoItem]) -> io::Result<()> {
    let dir = log_dir();
    fs::create_dir_all(&dir)?;
    let content: String = todos
        .iter()
        .map(|t| {
            let mark = if t.done { "[x]" } else { "[ ]" };
            format!("- {mark} {}\n", t.text)
        })
        .collect();
    fs::write(todo_path(), content)
}

pub fn send_notification(title: &str, message: &str) {
    Command::new("terminal-notifier")
        .args([
            "-title",
            title,
            "-message",
            message,
            "-appIcon",
            "/Users/alvisf/Documents/pomodoro_timer_icon.png",
            "-sound",
            "default",
            "-group",
            "pomodoro",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok();
}

pub fn load_daily_stats() -> BTreeMap<String, DayStats> {
    let dir = log_dir();
    let mut stats = BTreeMap::new();

    let Ok(entries) = fs::read_dir(&dir) else {
        return stats;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if ext != "md" {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };

        if is_quarterly_filename(stem) {
            parse_quarterly_file(&contents, &mut stats);
        } else if is_daily_filename(stem) {
            parse_daily_file(stem, &contents, &mut stats);
        }
    }

    stats
}

fn is_quarterly_filename(stem: &str) -> bool {
    stem.len() == 7
        && stem.as_bytes()[4] == b'-'
        && stem.as_bytes()[5] == b'Q'
        && matches!(stem.as_bytes()[6], b'1'..=b'4')
}

fn is_daily_filename(stem: &str) -> bool {
    stem.len() == 10 && stem.as_bytes()[4] == b'-' && stem.as_bytes()[7] == b'-'
}

fn parse_quarterly_file(contents: &str, stats: &mut BTreeMap<String, DayStats>) {
    let mut current_date: Option<String> = None;
    for line in contents.lines() {
        if let Some(date) = line.strip_prefix("## ") {
            let date = date.trim();
            if is_daily_filename(date) {
                current_date = Some(date.to_string());
            }
        } else if let Some(stripped) = line.strip_prefix("- ")
            && let Some(secs) = parse_entry_duration(stripped)
            && let Some(date) = &current_date
        {
            let day = stats.entry(date.clone()).or_default();
            if stripped.starts_with("[h]") {
                day.helping_secs += secs;
            } else {
                day.work_secs += secs;
            }
            day.sessions += 1;
        }
    }
}

fn parse_daily_file(date: &str, contents: &str, stats: &mut BTreeMap<String, DayStats>) {
    let mut day = DayStats::default();
    for line in contents.lines() {
        if let Some(stripped) = line.strip_prefix("- ")
            && let Some(secs) = parse_entry_duration(stripped)
        {
            if stripped.starts_with("[h]") {
                day.helping_secs += secs;
            } else {
                day.work_secs += secs;
            }
            day.sessions += 1;
        }
    }
    if day.sessions > 0 {
        stats.insert(date.to_string(), day);
    }
}

fn parse_entry_duration(line: &str) -> Option<u64> {
    let start = line.find('(')?;
    let end = line.find(')')?;
    let dur_str = &line[start + 1..end];
    let (m_str, s_str) = dur_str.split_once(':')?;
    let m: u64 = m_str.parse().ok()?;
    let s: u64 = s_str.parse().ok()?;
    Some(m * 60 + s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_hours_zero() {
        assert_eq!(format_hours(0), "0h 00m");
    }

    #[test]
    fn test_format_hours_minutes_only() {
        assert_eq!(format_hours(2700), "0h 45m");
    }

    #[test]
    fn test_format_hours_mixed() {
        assert_eq!(format_hours(9300), "2h 35m");
    }

    #[test]
    fn test_format_hours_exact() {
        assert_eq!(format_hours(7200), "2h 00m");
    }

    #[test]
    fn test_format_mmss() {
        assert_eq!(format_mmss(0), "00:00");
        assert_eq!(format_mmss(90), "01:30");
        assert_eq!(format_mmss(1500), "25:00");
    }

    #[test]
    fn test_local_date_format() {
        let date = local_date_str();
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
    }

    #[test]
    fn test_local_time_format() {
        let time = local_time_str();
        assert_eq!(time.len(), 5);
        assert_eq!(&time[2..3], ":");
    }

    #[test]
    fn test_local_time_12h_format() {
        let time = local_time_12h();
        assert!(time.ends_with("AM") || time.ends_with("PM"));
        assert!(time.contains(':'));
    }

    #[test]
    fn test_parse_entry_duration() {
        assert_eq!(
            parse_entry_duration("[x] 09:15 – 09:40 (25:00) task"),
            Some(1500)
        );
        assert_eq!(parse_entry_duration("[ ] 10:00 – 10:05 (05:30)"), Some(330));
        // Old emoji format still parses
        assert_eq!(
            parse_entry_duration("09:15 – 09:40 (25:00) ✅ task"),
            Some(1500)
        );
        assert_eq!(parse_entry_duration("no parens here"), None);
    }

    #[test]
    fn test_quarterly_filename() {
        assert_eq!(quarterly_filename("2026-01-15"), "2026-Q1.md");
        assert_eq!(quarterly_filename("2026-03-31"), "2026-Q1.md");
        assert_eq!(quarterly_filename("2026-04-01"), "2026-Q2.md");
        assert_eq!(quarterly_filename("2026-06-30"), "2026-Q2.md");
        assert_eq!(quarterly_filename("2026-07-01"), "2026-Q3.md");
        assert_eq!(quarterly_filename("2026-10-01"), "2026-Q4.md");
        assert_eq!(quarterly_filename("2026-12-31"), "2026-Q4.md");
    }

    #[test]
    fn test_quarterly_title() {
        assert_eq!(quarterly_title("2026-04-09"), "2026 Q2");
        assert_eq!(quarterly_title("2026-01-01"), "2026 Q1");
    }

    #[test]
    fn test_is_quarterly_filename() {
        assert!(is_quarterly_filename("2026-Q1"));
        assert!(is_quarterly_filename("2026-Q4"));
        assert!(!is_quarterly_filename("2026-Q5"));
        assert!(!is_quarterly_filename("2026-Q0"));
        assert!(!is_quarterly_filename("2026-04-09"));
        assert!(!is_quarterly_filename("short"));
    }

    #[test]
    fn test_is_daily_filename() {
        assert!(is_daily_filename("2026-04-09"));
        assert!(!is_daily_filename("2026-Q1"));
        assert!(!is_daily_filename("short"));
    }

    #[test]
    fn test_parse_quarterly_file() {
        let contents = "\
# 2026 Q2

## 2026-04-09

- [x] 09:15 – 09:40 (25:00) task1
- [x] 09:45 – 10:10 (25:00) task2

## 2026-04-10

- 08:30 – 09:00 (30:00) ✅ task3
";
        let mut stats = BTreeMap::new();
        parse_quarterly_file(contents, &mut stats);
        assert_eq!(stats.len(), 2);
        assert_eq!(stats["2026-04-09"].sessions, 2);
        assert_eq!(stats["2026-04-09"].work_secs, 3000);
        assert_eq!(stats["2026-04-10"].sessions, 1);
        assert_eq!(stats["2026-04-10"].work_secs, 1800);
    }

    #[test]
    fn test_parse_helping_entries() {
        let contents = "\
# 2026 Q2

## 2026-04-09

- [x] 09:00 – 09:25 (25:00) task1
- [h] 09:30 – 09:45 (15:00) helped Bob
- [x] 10:00 – 10:25 (25:00) task2
";
        let mut stats = BTreeMap::new();
        parse_quarterly_file(contents, &mut stats);
        assert_eq!(stats["2026-04-09"].sessions, 3);
        assert_eq!(stats["2026-04-09"].work_secs, 3000);
        assert_eq!(stats["2026-04-09"].helping_secs, 900);
    }

    #[test]
    fn test_parse_daily_file() {
        let contents = "\
# 2026-04-09

- [x] 09:15 – 09:40 (25:00) task1
- [ ] 09:45 – 10:10 (10:00) task2
";
        let mut stats = BTreeMap::new();
        parse_daily_file("2026-04-09", contents, &mut stats);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats["2026-04-09"].sessions, 2);
        assert_eq!(stats["2026-04-09"].work_secs, 2100);
    }

    #[test]
    fn test_parse_todo_line() {
        let item = parse_todo_line("- [ ] Buy groceries").unwrap();
        assert_eq!(item.text, "Buy groceries");
        assert!(!item.done);

        let item = parse_todo_line("- [x] Write report").unwrap();
        assert_eq!(item.text, "Write report");
        assert!(item.done);

        assert!(parse_todo_line("not a todo").is_none());
        assert!(parse_todo_line("- Buy groceries").is_none());
    }

    #[test]
    fn test_todo_serialize() {
        let todos = vec![
            TodoItem {
                text: "Task A".to_string(),
                done: false,
            },
            TodoItem {
                text: "Task B".to_string(),
                done: true,
            },
        ];
        let content: String = todos
            .iter()
            .map(|t| {
                let mark = if t.done { "[x]" } else { "[ ]" };
                format!("- {mark} {}\n", t.text)
            })
            .collect();
        assert_eq!(content, "- [ ] Task A\n- [x] Task B\n");
    }
}
