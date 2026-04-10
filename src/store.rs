use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::BufRead;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Default, Clone)]
pub struct DayStats {
    pub work_secs: u64,
    pub sessions: u32,
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
        .unwrap()
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

pub fn local_date_str() -> String {
    local_date_for_offset(0)
}

pub fn local_date_for_offset(offset_days: i64) -> String {
    let secs = unix_now() as i64 + offset_days * 86400;
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
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    format!("{h}h {m:02}m")
}

fn format_mmss(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m:02}:{s:02}")
}

pub fn save_work_entry_md(
    date: &str,
    start_time: &str,
    end_time: &str,
    duration_secs: u64,
    task: &str,
    completed: bool,
) -> io::Result<()> {
    let dir = log_dir();
    fs::create_dir_all(&dir)?;

    let path = dir.join(format!("{date}.md"));
    let is_new = !path.exists();

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    if is_new {
        writeln!(file, "# {date}\n")?;
    }

    let icon = if completed { "✅" } else { "⏭" };
    let duration = format_mmss(duration_secs);

    if task.is_empty() {
        writeln!(file, "- {start_time} – {end_time} ({duration}) {icon}")
    } else {
        writeln!(file, "- {start_time} – {end_time} ({duration}) {icon} {task}")
    }
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

pub fn save_config(work_secs: u64, break_secs: u64, long_break_secs: u64, sessions_before_long: u32) -> io::Result<()> {
    let dir = log_dir();
    fs::create_dir_all(&dir)?;
    let contents = format!("work_secs={work_secs}\nbreak_secs={break_secs}\nlong_break_secs={long_break_secs}\nsessions_before_long={sessions_before_long}\n");
    fs::write(config_path(), contents)
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
        if stem.len() != 10
            || stem.as_bytes()[4] != b'-'
            || stem.as_bytes()[7] != b'-'
        {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };

        let mut day = DayStats::default();
        for line in contents.lines() {
            if let Some(stripped) = line.strip_prefix("- ")
                && let Some(secs) = parse_entry_duration(stripped) {
                day.work_secs += secs;
                day.sessions += 1;
            }
        }

        if day.sessions > 0 {
            stats.insert(stem.to_string(), day);
        }
    }

    stats
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
    fn test_parse_entry_duration() {
        assert_eq!(
            parse_entry_duration("09:15 – 09:40 (25:00) ✅ task"),
            Some(1500)
        );
        assert_eq!(
            parse_entry_duration("10:00 – 10:05 (05:30) ⏭"),
            Some(330)
        );
        assert_eq!(parse_entry_duration("no parens here"), None);
    }
}
