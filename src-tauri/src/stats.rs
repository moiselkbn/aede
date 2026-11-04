//! The statistics of the dashboard, saved in stats.json in the app data folder: one line of
//! counters per day (dictations, words, seconds of speech), and never any text. The file is
//! separate from the history, so "Tout effacer" does not touch it; only `reset_stats` empties it.
//! The file only stays on this Mac and only the user can read it.
//!
//! Days change at midnight, Mac time. A day is a number (days since 1970-01-01) in the code and
//! a "2026-10-28" text in the file. The calculations work on plain numbers, so they are tested
//! without any file or system call.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::history::{self, Entry};
use crate::recordings;

const FILE_NAME: &str = "stats.json";

/// Sent to the window whenever the counters change, so that an open page shows them as they are.
const CHANGED_EVENT: &str = "stats-changed";

/// Only one write at a time: a dictation must not save while a reset empties the file.
static FILE_LOCK: Mutex<()> = Mutex::new(());

const MS_PER_DAY: i64 = 86_400_000;

/// The lengths in days the window can ask for: 1J, 7J, 30J, 3M, 1Y.
const PERIODS: [u32; 5] = [1, 7, 30, 90, 365];

/// What was counted on one day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Counters {
    dictations: u32,
    words: u64,
    /// The words of the dictations whose length is known: the words per minute use only these.
    timed_words: u64,
    speech_seconds: f64,
}

impl Counters {
    fn add(&mut self, other: &Counters) {
        self.dictations += other.dictations;
        self.words += other.words;
        self.timed_words += other.timed_words;
        self.speech_seconds += other.speech_seconds;
    }
}

/// The counters of every day that has some, by day number.
type Days = BTreeMap<i64, Counters>;

#[derive(Serialize, Deserialize)]
struct StoredDay {
    date: String,
    #[serde(flatten)]
    counters: Counters,
}

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    days: Vec<StoredDay>,
}

// ---- Dates ----

/// The number of the day of `at_ms`, with the offset of the time zone in seconds.
fn day_number(at_ms: u64, offset_secs: i64) -> i64 {
    (at_ms as i64 + offset_secs * 1000).div_euclid(MS_PER_DAY)
}

/// Days since 1970-01-01 of a calendar date (the algorithm of Howard Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The calendar date (year, month, day) of a day number. The opposite of `days_from_civil`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn format_date(day: i64) -> String {
    let (year, month, day) = civil_from_days(day);
    format!("{year:04}-{month:02}-{day:02}")
}

fn parse_date(text: &str) -> Option<i64> {
    let mut parts = text.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let number = days_from_civil(year, month, day);
    // Refuses a date that does not exist, such as 2026-02-31.
    (format_date(number) == text).then_some(number)
}

/// 0 for Monday, up to 6 for Sunday. The day 0 (1970-01-01) was a Thursday.
fn weekday(day: i64) -> u32 {
    (day + 3).rem_euclid(7) as u32
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    days_from_civil(next_year, next_month, 1) - days_from_civil(year, month, 1)
}

/// The offset of the Mac time zone from UTC at that moment, in seconds (daylight saving time
/// included). 0 if macOS does not answer.
fn local_offset_secs(at_ms: u64) -> i64 {
    use objc2_foundation::{NSDate, NSTimeZone};
    // The system zone is cached by macOS: reset it, in case the user travelled since the launch.
    NSTimeZone::resetSystemTimeZone();
    let date = NSDate::dateWithTimeIntervalSince1970((at_ms / 1000) as f64);
    NSTimeZone::systemTimeZone().secondsFromGMTForDate(&date) as i64
}

// ---- Calculations ----

pub fn count_words(text: &str) -> u64 {
    text.split_whitespace().count() as u64
}

fn counters_of(words: u64, speech_seconds: f64) -> Counters {
    Counters {
        dictations: 1,
        words,
        timed_words: if speech_seconds > 0.0 { words } else { 0 },
        speech_seconds: speech_seconds.max(0.0),
    }
}

/// The sum of the counters of the days from `first` to `last`, both included.
fn totals(days: &Days, first: i64, last: i64) -> Counters {
    let mut sum = Counters::default();
    for counters in days.range(first..=last).map(|(_, counters)| counters) {
        sum.add(counters);
    }
    sum
}

fn words_per_minute(counters: &Counters) -> f64 {
    if counters.timed_words == 0 || counters.speech_seconds <= 0.0 {
        0.0
    } else {
        counters.timed_words as f64 / (counters.speech_seconds / 60.0)
    }
}

/// How much `current` is above (or below) `previous`, in percent. None when there is nothing to
/// compare with, instead of an infinite increase.
fn percent_change(current: f64, previous: f64) -> Option<f64> {
    (previous > 0.0).then(|| (current - previous) / previous * 100.0)
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Metric {
    value: f64,
    /// Percent, compared with the period just before. None when that period is empty.
    change: Option<f64>,
}

fn metric(current: f64, previous: f64) -> Metric {
    Metric {
        value: current,
        change: percent_change(current, previous),
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Summary {
    dictations: Metric,
    words: Metric,
    words_per_minute: Metric,
    /// Days in a row with a dictation, up to today.
    streak: u32,
}

/// The cards of the page for the last `period` days (today included) and the same number of days
/// before them.
fn summary(days: &Days, today: i64, period: i64) -> Summary {
    let current = totals(days, today - period + 1, today);
    let previous = totals(days, today - 2 * period + 1, today - period);
    Summary {
        dictations: metric(current.dictations as f64, previous.dictations as f64),
        words: metric(current.words as f64, previous.words as f64),
        words_per_minute: metric(words_per_minute(&current), words_per_minute(&previous)),
        streak: streak(days, today),
    }
}

/// Days in a row with a dictation. A today without any dictation yet does not break the streak
/// of the days before it.
fn streak(days: &Days, today: i64) -> u32 {
    let has_dictation = |day: i64| {
        days.get(&day)
            .is_some_and(|counters| counters.dictations > 0)
    };
    let mut day = if has_dictation(today) {
        today
    } else {
        today - 1
    };
    let mut count = 0;
    while has_dictation(day) {
        count += 1;
        day -= 1;
    }
    count
}

/// Dictations per day of a stretch of days, from `first` for `length` days.
fn counts(days: &Days, first: i64, length: i64) -> Vec<u32> {
    (first..first + length)
        .map(|day| days.get(&day).map_or(0, |counters| counters.dictations))
        .collect()
}

/// The calendar covers this many days, today included: a fixed width of 53 weeks whatever the date.
const YEAR_DAYS: i64 = 365;

#[derive(Debug, PartialEq, Serialize)]
pub struct YearActivity {
    /// "2025-10-30": the first day of the calendar, 364 days before today.
    start: String,
    /// Of that first day: 0 for Monday, up to 6 for Sunday.
    first_weekday: u32,
    /// Dictations of each of the 365 days, the last one being today.
    counts: Vec<u32>,
    total: u32,
}

/// The last 365 days, today included.
fn year_activity(days: &Days, today: i64) -> YearActivity {
    let first = today - (YEAR_DAYS - 1);
    let counts = counts(days, first, YEAR_DAYS);
    YearActivity {
        start: format_date(first),
        first_weekday: weekday(first),
        total: counts.iter().sum(),
        counts,
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct MonthActivity {
    year: i64,
    /// 1 for January, up to 12.
    month: u32,
    /// Dictations of each day of the month. The days after today are 0.
    counts: Vec<u32>,
    /// Days of the month up to today, today included (all of them for a past month).
    elapsed: u32,
    /// Dictations per day over the elapsed days.
    average: f64,
}

fn month_activity(days: &Days, today: i64, year: i64, month: i64) -> MonthActivity {
    let first = days_from_civil(year, month, 1);
    let length = days_in_month(year, month);
    let elapsed = (today - first + 1).clamp(0, length);
    let mut counts = counts(days, first, length);
    for count in counts.iter_mut().skip(elapsed as usize) {
        *count = 0;
    }
    let total: u32 = counts.iter().sum();
    MonthActivity {
        year,
        month: month as u32,
        average: if elapsed == 0 {
            0.0
        } else {
            f64::from(total) / elapsed as f64
        },
        counts,
        elapsed: elapsed as u32,
    }
}

/// Everything the page shows, for the chosen period.
#[derive(Debug, PartialEq, Serialize)]
pub struct Stats {
    /// "2026-10-28": the window needs it to place the activity in the calendar.
    today: String,
    summary: Summary,
    year: YearActivity,
    last_month: MonthActivity,
    this_month: MonthActivity,
}

fn stats_for(days: &Days, today: i64, period: i64) -> Stats {
    let (year, month, _) = civil_from_days(today);
    let (last_year, last_month) = if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    };
    Stats {
        today: format_date(today),
        summary: summary(days, today, period),
        year: year_activity(days, today),
        last_month: month_activity(days, today, last_year, last_month),
        this_month: month_activity(days, today, year, month),
    }
}

// ---- The file ----

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Dossier de données de l'app introuvable : {err}"))?;
    Ok(data.join(FILE_NAME))
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn parse(text: &str, path: &Path) -> Result<Days, String> {
    let unreadable = |err: String| format!("Statistiques illisibles ({}) : {err}", path.display());
    let stored: Stored = serde_json::from_str(text).map_err(|err| unreadable(err.to_string()))?;
    let mut days = Days::new();
    for day in stored.days {
        let number = parse_date(&day.date)
            .ok_or_else(|| unreadable(format!("date invalide « {} »", day.date)))?;
        days.entry(number).or_default().add(&day.counters);
    }
    Ok(days)
}

fn serialize(days: &Days) -> Result<String, String> {
    let stored = Stored {
        days: days
            .iter()
            .map(|(day, counters)| StoredDay {
                date: format_date(*day),
                counters: *counters,
            })
            .collect(),
    };
    serde_json::to_string_pretty(&stored)
        .map_err(|err| format!("Impossible d'enregistrer les statistiques : {err}"))
}

fn save_to(path: &Path, days: &Days) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Impossible de créer le dossier {} : {err}", dir.display()))?;
    }
    history::replace_file(path, &serialize(days)?)
}

/// The counters rebuilt from past dictations, for the first start of the statistics. A dictation
/// whose audio is gone counts its words but no time.
fn from_entries(
    entries: &[Entry],
    seconds_of: impl Fn(&Entry) -> Option<f64>,
    offset_of: impl Fn(u64) -> i64,
) -> Days {
    let mut days = Days::new();
    for entry in entries {
        let day = day_number(entry.at_ms, offset_of(entry.at_ms));
        let counters = counters_of(count_words(&entry.text), seconds_of(entry).unwrap_or(0.0));
        days.entry(day).or_default().add(&counters);
    }
    days
}

/// The saved counters. Without a file yet, they are built from the history, and saved: from then
/// on the file exists, so a reset (which leaves it empty) is never undone by the history.
fn load_or_build(app: &AppHandle, path: &Path) -> Result<Days, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text, path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let entries = history::all_entries(app)?;
            let days = from_entries(
                &entries,
                |entry| {
                    entry
                        .audio_file
                        .as_deref()
                        .and_then(|name| recordings::duration_seconds(app, name))
                },
                local_offset_secs,
            );
            save_to(path, &days)?;
            Ok(days)
        }
        Err(err) => Err(format!(
            "Statistiques illisibles ({}) : {err}",
            path.display()
        )),
    }
}

fn notify_changed(app: &AppHandle) {
    if let Err(err) = app.emit(CHANGED_EVENT, ()) {
        eprintln!("[stats] could not notify the UI: {err}");
    }
}

/// Counts one dictation that ended at `at_ms`. Never fails: the statistics must not stop the
/// dictation, so an error is shown in the window instead. To be called before the dictation is
/// added to the history, so that a first start does not count it twice.
pub fn record(app: &AppHandle, at_ms: u64, text: &str, speech_seconds: f64) {
    let saved = path(app).and_then(|path| {
        let _guard = lock();
        let mut days = load_or_build(app, &path)?;
        let day = day_number(at_ms, local_offset_secs(at_ms));
        days.entry(day)
            .or_default()
            .add(&counters_of(count_words(text), speech_seconds));
        save_to(&path, &days)
    });
    match saved {
        Ok(()) => notify_changed(app),
        Err(message) => history::report_error(app, message),
    }
}

/// What the page shows, for the last 7, 30, 90 or 365 days.
#[tauri::command]
pub fn get_stats(app: AppHandle, period: u32) -> Result<Stats, String> {
    if !PERIODS.contains(&period) {
        return Err(format!("Période inconnue : {period} jours."));
    }
    let path = path(&app)?;
    let days = {
        let _guard = lock();
        load_or_build(&app, &path)?
    };
    let now = history::now_ms();
    let today = day_number(now, local_offset_secs(now));
    Ok(stats_for(&days, today, i64::from(period)))
}

/// Empties the counters. The file stays, empty, so that the history does not fill it again.
#[tauri::command]
pub fn reset_stats(app: AppHandle) -> Result<(), String> {
    let path = path(&app)?;
    {
        let _guard = lock();
        save_to(&path, &Days::new())?;
    }
    notify_changed(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn day(text: &str) -> i64 {
        parse_date(text).unwrap()
    }

    fn days_with(dictations: &[(&str, u32)]) -> Days {
        dictations
            .iter()
            .map(|(date, dictations)| {
                (
                    day(date),
                    Counters {
                        dictations: *dictations,
                        ..Counters::default()
                    },
                )
            })
            .collect()
    }

    #[test]
    fn dates_go_back_and_forth() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        for number in [-1000, 0, 11_016, 20_000, 20_390] {
            let (y, m, d) = civil_from_days(number);
            assert_eq!(days_from_civil(y, m, d), number);
        }
        assert_eq!(format_date(day("2026-10-28")), "2026-10-28");
    }

    #[test]
    fn a_date_that_does_not_exist_is_refused() {
        assert_eq!(parse_date("2026-02-31"), None);
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(parse_date("2026-10"), None);
        assert_eq!(parse_date("2026-10-28-1"), None);
        assert_eq!(parse_date("abc"), None);
        // A leap day exists only in a leap year.
        assert!(parse_date("2028-02-29").is_some());
        assert_eq!(parse_date("2026-02-29"), None);
    }

    #[test]
    fn the_weekday_starts_on_monday() {
        // 2026-10-29 is a Thursday, 2026-11-01 a Sunday.
        assert_eq!(weekday(day("2026-10-29")), 3);
        assert_eq!(weekday(day("2026-10-26")), 0);
        assert_eq!(weekday(day("2026-11-01")), 6);
    }

    #[test]
    fn months_have_their_length() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2026, 9), 30);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    #[test]
    fn the_offset_moves_a_late_dictation_to_the_next_day() {
        // 2026-10-28 23:30 UTC.
        let at_ms = (day("2026-10-28") * 86_400 + 23 * 3600 + 1800) as u64 * 1000;
        assert_eq!(day_number(at_ms, 0), day("2026-10-28"));
        assert_eq!(day_number(at_ms, 3600), day("2026-10-29"));
        assert_eq!(day_number(at_ms, -3600 * 24), day("2026-10-27"));
    }

    #[test]
    fn the_local_offset_is_a_real_one() {
        let offset = local_offset_secs(history::now_ms());
        assert!((-14 * 3600..=14 * 3600).contains(&offset));
    }

    #[test]
    fn words_are_counted_whatever_the_spaces() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("  Crée   la table\nusers. "), 4);
    }

    #[test]
    fn a_dictation_without_known_length_counts_no_timed_words() {
        let known = counters_of(10, 4.0);
        assert_eq!((known.words, known.timed_words), (10, 10));
        let unknown = counters_of(10, 0.0);
        assert_eq!((unknown.words, unknown.timed_words), (10, 0));
        assert_eq!(unknown.dictations, 1);
    }

    #[test]
    fn the_words_per_minute_use_the_speech_time() {
        let counters = Counters {
            dictations: 2,
            words: 300,
            timed_words: 300,
            speech_seconds: 120.0,
        };
        assert_eq!(words_per_minute(&counters), 150.0);
        assert_eq!(words_per_minute(&Counters::default()), 0.0);
        // Words without a known length are left out of the rate.
        let partly = Counters {
            words: 400,
            timed_words: 300,
            ..counters
        };
        assert_eq!(words_per_minute(&partly), 150.0);
    }

    #[test]
    fn the_change_compares_with_the_period_before() {
        assert_eq!(percent_change(120.0, 100.0), Some(20.0));
        assert_eq!(percent_change(97.0, 100.0), Some(-3.0));
        assert_eq!(percent_change(5.0, 0.0), None);
        assert_eq!(percent_change(0.0, 0.0), None);
    }

    #[test]
    fn the_summary_sums_the_period_and_the_one_before() {
        let days = days_with(&[
            ("2026-10-29", 3),
            ("2026-10-23", 1),
            // 7 days before today and earlier: the period before.
            ("2026-10-22", 2),
            ("2026-10-16", 2),
            // Older than 14 days: out of both.
            ("2026-10-15", 50),
        ]);
        let summary = summary(&days, day("2026-10-29"), 7);
        assert_eq!(
            summary.dictations,
            Metric {
                value: 4.0,
                change: Some(0.0)
            }
        );
        let nothing = super::summary(&Days::new(), day("2026-10-29"), 7);
        assert_eq!(
            nothing.dictations,
            Metric {
                value: 0.0,
                change: None
            }
        );
        assert_eq!(nothing.streak, 0);
    }

    #[test]
    fn a_period_of_one_day_compares_today_with_yesterday() {
        let days = days_with(&[("2026-10-29", 3), ("2026-10-28", 2), ("2026-10-27", 9)]);
        let summary = summary(&days, day("2026-10-29"), 1);
        assert_eq!(
            summary.dictations,
            Metric {
                value: 3.0,
                change: Some(50.0)
            }
        );
    }

    #[test]
    fn the_streak_counts_the_days_in_a_row() {
        let days = days_with(&[
            ("2026-10-29", 1),
            ("2026-10-28", 2),
            ("2026-10-27", 1),
            ("2026-10-25", 1),
        ]);
        assert_eq!(streak(&days, day("2026-10-29")), 3);
    }

    #[test]
    fn an_empty_today_does_not_break_the_streak() {
        let days = days_with(&[("2026-10-28", 2), ("2026-10-27", 1)]);
        assert_eq!(streak(&days, day("2026-10-29")), 2);
        // Yesterday empty too: it is over.
        assert_eq!(streak(&days, day("2026-10-30")), 0);
    }

    #[test]
    fn a_day_with_a_zero_counter_is_not_a_dictation_day() {
        let days = days_with(&[("2026-10-29", 0), ("2026-10-28", 1)]);
        assert_eq!(streak(&days, day("2026-10-29")), 1);
    }

    #[test]
    fn the_year_activity_covers_the_last_365_days() {
        let days = days_with(&[
            // Older than the first day of the calendar: left out.
            ("2025-10-29", 9),
            ("2025-10-30", 2),
            ("2026-10-29", 5),
            // After today: never counted, even if the file has one.
            ("2026-10-30", 7),
        ]);
        let year = year_activity(&days, day("2026-10-29"));
        assert_eq!(year.start, "2025-10-30");
        // 2025-10-30 is a Thursday, like 2026-10-29: 52 weeks apart.
        assert_eq!(year.first_weekday, 3);
        assert_eq!(year.counts.len(), 365);
        assert_eq!((year.counts[0], year.counts[364]), (2, 5));
        assert_eq!(year.total, 7);
    }

    #[test]
    fn a_past_month_is_counted_in_full() {
        let days = days_with(&[("2026-09-01", 4), ("2026-09-30", 5), ("2026-10-02", 7)]);
        let month = month_activity(&days, day("2026-10-29"), 2026, 9);
        assert_eq!(month.counts.len(), 30);
        assert_eq!(month.elapsed, 30);
        assert_eq!((month.counts[0], month.counts[29]), (4, 5));
        assert_eq!(month.average, 0.3);
    }

    #[test]
    fn the_current_month_is_counted_up_to_today() {
        let days = days_with(&[("2026-10-01", 6), ("2026-10-10", 4), ("2026-10-29", 2)]);
        let month = month_activity(&days, day("2026-10-10"), 2026, 10);
        assert_eq!(month.counts.len(), 31);
        assert_eq!(month.elapsed, 10);
        // The 29th is in the future for that "today".
        assert_eq!(month.counts[28], 0);
        assert_eq!(month.average, 1.0);
    }

    #[test]
    fn the_stats_in_january_look_at_december_of_the_year_before() {
        let stats = stats_for(&Days::new(), day("2027-01-05"), 30);
        assert_eq!(stats.today, "2027-01-05");
        assert_eq!((stats.last_month.year, stats.last_month.month), (2026, 12));
        assert_eq!((stats.this_month.year, stats.this_month.month), (2027, 1));
        assert_eq!(stats.this_month.elapsed, 5);
    }

    #[test]
    fn past_dictations_are_rebuilt_into_days() {
        let entry = |at_ms: u64, text: &str, audio: Option<&str>| Entry {
            at_ms,
            text: text.to_string(),
            raw_text: text.to_string(),
            fallback_reason: None,
            paste_error: None,
            audio_file: audio.map(str::to_string),
        };
        let noon = (day("2026-10-28") * 86_400 + 12 * 3600) as u64 * 1000;
        let entries = [
            entry(noon, "un deux trois", Some("a.wav")),
            entry(noon + 1000, "quatre cinq", None),
        ];
        let days = from_entries(
            &entries,
            |entry| entry.audio_file.as_ref().map(|_| 3.0),
            |_| 0,
        );
        assert_eq!(days.len(), 1);
        let counters = days[&day("2026-10-28")];
        assert_eq!(counters.dictations, 2);
        assert_eq!(counters.words, 5);
        // The second dictation has no audio: its words are counted, but not in the rate.
        assert_eq!(counters.timed_words, 3);
        assert_eq!(counters.speech_seconds, 3.0);
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aede-stats-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    #[test]
    fn the_file_keeps_the_counters_and_no_text() {
        let path = temp_file("roundtrip");
        let mut days = Days::new();
        days.insert(day("2026-10-28"), counters_of(12, 6.5));
        days.insert(day("2026-10-29"), counters_of(3, 1.5));
        save_to(&path, &days).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"date\": \"2026-10-28\""));
        assert_eq!(parse(&text, &path).unwrap(), days);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_damaged_file_is_reported_not_hidden() {
        let path = Path::new("stats.json");
        assert!(parse("pas du json", path).is_err());
        let wrong_date = r#"{"days":[{"date":"2026-02-31","dictations":1,"words":1,"timed_words":1,"speech_seconds":1.0}]}"#;
        let message = parse(wrong_date, path).unwrap_err();
        assert!(message.contains("2026-02-31"));
    }

    #[test]
    fn two_lines_of_the_same_day_are_added_up() {
        let line = r#"{"date":"2026-10-28","dictations":1,"words":5,"timed_words":5,"speech_seconds":2.0}"#;
        let text = format!(r#"{{"days":[{line},{line}]}}"#);
        let days = parse(&text, Path::new("stats.json")).unwrap();
        assert_eq!(days[&day("2026-10-28")].dictations, 2);
        assert_eq!(days[&day("2026-10-28")].words, 10);
    }

    #[test]
    fn only_the_five_periods_of_the_window_are_known() {
        assert!(PERIODS.contains(&1));
        assert!(PERIODS.contains(&30));
        assert!(!PERIODS.contains(&31));
    }
}
