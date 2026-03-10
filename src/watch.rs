use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Local;

use crate::SortBy;
use crate::filter::ProjectFilter;

/// Clear the terminal screen and move cursor to top-left.
fn clear_screen() -> Result<()> {
    // Use ANSI escape codes — works on all modern terminals
    print!("\x1B[2J\x1B[H");
    io::stdout().flush().context("Failed to flush stdout")?;
    Ok(())
}

/// Format a timestamp header for display.
pub fn format_watch_header(path: &Path, interval: u64) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");
    format!(
        "devpulse — watching {} (every {}s) | Last update: {} | Press Ctrl+C to exit\n",
        path.display(),
        interval,
        now,
    )
}

/// Sleep for the given duration, checking the stop flag every 500ms.
/// Returns true if we should stop (flag was set), false if sleep completed.
pub fn interruptible_sleep(duration: Duration, stop: &AtomicBool) -> bool {
    let step = Duration::from_millis(500);
    let mut remaining = duration;

    while remaining > Duration::ZERO {
        if stop.load(Ordering::Relaxed) {
            return true;
        }
        let sleep_time = remaining.min(step);
        std::thread::sleep(sleep_time);
        remaining = remaining.saturating_sub(sleep_time);
    }

    stop.load(Ordering::Relaxed)
}

/// Run the watch loop: scan, display, sleep, repeat until Ctrl+C.
pub fn run_watch_loop(
    scan_path: &Path,
    sort: &SortBy,
    json: bool,
    interval_secs: u64,
    filters: &[ProjectFilter],
    depth: u32,
) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);

    ctrlc::set_handler(move || {
        stop_clone.store(true, Ordering::Relaxed);
    })
    .context("Failed to set Ctrl+C handler")?;

    loop {
        clear_screen()?;
        print!("{}", format_watch_header(scan_path, interval_secs));
        io::stdout().flush().context("Failed to flush stdout")?;

        crate::scan_and_display(scan_path, sort, json, filters, &[], depth)?;

        if interruptible_sleep(Duration::from_secs(interval_secs), &stop) {
            println!("\nExiting watch mode.");
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    #[test]
    fn test_format_watch_header_contains_path() {
        let path = PathBuf::from("/home/user/projects");
        let header = format_watch_header(&path, 60);
        assert!(
            header.contains("/home/user/projects"),
            "Header should contain the scan path"
        );
    }

    #[test]
    fn test_format_watch_header_contains_interval() {
        let path = PathBuf::from("/tmp");
        let header = format_watch_header(&path, 30);
        assert!(header.contains("30s"), "Header should contain the interval");
    }

    #[test]
    fn test_format_watch_header_contains_timestamp() {
        let path = PathBuf::from("/tmp");
        let header = format_watch_header(&path, 60);
        // Should contain a date-like pattern (YYYY-MM-DD)
        assert!(
            header.contains("Last update:"),
            "Header should contain 'Last update:'"
        );
        // The timestamp format is YYYY-MM-DD HH:MM:SS
        let now = Local::now().format("%Y-%m-%d").to_string();
        assert!(header.contains(&now), "Header should contain today's date");
    }

    #[test]
    fn test_format_watch_header_contains_exit_hint() {
        let path = PathBuf::from("/tmp");
        let header = format_watch_header(&path, 60);
        assert!(
            header.contains("Ctrl+C"),
            "Header should mention Ctrl+C to exit"
        );
    }

    #[test]
    fn test_interruptible_sleep_completes_normally() {
        let stop = AtomicBool::new(false);
        let start = Instant::now();
        let was_stopped = interruptible_sleep(Duration::from_millis(100), &stop);
        let elapsed = start.elapsed();

        assert!(!was_stopped, "Should not report stopped");
        assert!(
            elapsed >= Duration::from_millis(100),
            "Should sleep for at least the requested duration"
        );
    }

    #[test]
    fn test_interruptible_sleep_stops_early_when_flagged() {
        let stop = AtomicBool::new(false);

        // Spawn a thread to set the flag after 200ms
        let stop_ref = &stop as *const AtomicBool as usize;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            // SAFETY: we know the AtomicBool outlives this thread because
            // the test waits for interruptible_sleep to return
            unsafe {
                let stop_ptr = stop_ref as *const AtomicBool;
                (*stop_ptr).store(true, Ordering::Relaxed);
            }
        });

        let start = Instant::now();
        let was_stopped = interruptible_sleep(Duration::from_secs(10), &stop);
        let elapsed = start.elapsed();

        assert!(was_stopped, "Should report stopped");
        assert!(
            elapsed < Duration::from_secs(2),
            "Should exit well before 10s timeout (exited in {:?})",
            elapsed
        );
    }

    #[test]
    fn test_interruptible_sleep_immediate_stop() {
        let stop = AtomicBool::new(true); // Already flagged
        let start = Instant::now();
        let was_stopped = interruptible_sleep(Duration::from_secs(60), &stop);
        let elapsed = start.elapsed();

        assert!(was_stopped, "Should report stopped immediately");
        assert!(
            elapsed < Duration::from_secs(2),
            "Should exit almost immediately (exited in {:?})",
            elapsed
        );
    }

    #[test]
    fn test_interruptible_sleep_zero_duration() {
        let stop = AtomicBool::new(false);
        let was_stopped = interruptible_sleep(Duration::ZERO, &stop);
        assert!(!was_stopped, "Zero duration should complete without stop");
    }

    #[test]
    fn test_clear_screen_does_not_panic() {
        // Just verify it doesn't error out — actual screen clearing is a side effect
        let result = clear_screen();
        assert!(result.is_ok(), "clear_screen should not fail");
    }
}
