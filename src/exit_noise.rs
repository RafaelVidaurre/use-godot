//! Built-in exit-noise rules: reclassify known false-crash exits as success.
//!
//! Unmatched exits pass through unchanged. Never mutates Godot argv or crash handlers.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use std::{fs, time::SystemTime};

/// Shell-style mapped wait status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MappedExit {
    Exited {
        code: u8,
    },
    Signaled {
        signal: i32,
        core_dumped: bool,
        code: u8,
    },
    Other {
        code: u8,
    },
}

impl MappedExit {
    pub fn code(&self) -> u8 {
        match self {
            Self::Exited { code } | Self::Signaled { code, .. } | Self::Other { code } => *code,
        }
    }

    pub fn signal(&self) -> Option<i32> {
        match self {
            Self::Signaled { signal, .. } => Some(*signal),
            _ => None,
        }
    }
}

/// Compress a process exit code to a shell-style `u8` without fail-open.
///
/// Uses the low 8 bits of the status word. On Windows, `ExitStatus::code()` can
/// surface NTSTATUS values such as `0xC0000005` as negative `i32`s; `i32::clamp(0, 255)`
/// would map those crashes to **0**. When the low byte is 0 but the full status is
/// non-zero, return `1` so unmatched crashes never become success.
pub fn process_code_to_u8(code: i32) -> u8 {
    let low = (code as u32) & 0xff;
    if code != 0 && low == 0 { 1 } else { low as u8 }
}

pub fn map_exit_status(status: &std::process::ExitStatus) -> MappedExit {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            let code = process_code_to_u8(128i32.saturating_add(signal));
            return MappedExit::Signaled {
                signal,
                core_dumped: status.core_dumped(),
                code,
            };
        }
        if let Some(code) = status.code() {
            return MappedExit::Exited {
                code: process_code_to_u8(code),
            };
        }
        MappedExit::Other { code: 1 }
    }
    #[cfg(not(unix))]
    {
        if let Some(code) = status.code() {
            MappedExit::Exited {
                code: process_code_to_u8(code),
            }
        } else {
            MappedExit::Other { code: 1 }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExitObservation {
    pub mapped: MappedExit,
    pub argv: Vec<String>,
    pub binary: PathBuf,
    pub duration: Duration,
    /// Optional crash-report text (macOS correlator); empty when unavailable.
    pub crash_report_excerpt: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confidence {
    Stable,
    Experimental,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchResult {
    pub rule_id: &'static str,
    pub confidence: Confidence,
}

pub trait ExitNoiseRule: Send + Sync {
    fn id(&self) -> &'static str;
    fn confidence(&self) -> Confidence;
    fn matches(&self, obs: &ExitObservation) -> bool;
}

/// SIGABRT ∧ argv contains `--quit` or `--quit-after` (headless CI paths).
pub struct HeadlessQuitSigabrtRule;

impl ExitNoiseRule for HeadlessQuitSigabrtRule {
    fn id(&self) -> &'static str {
        "godot-headless-quit-sigabrt"
    }

    fn confidence(&self) -> Confidence {
        Confidence::Stable
    }

    fn matches(&self, obs: &ExitObservation) -> bool {
        if !cfg!(unix) {
            return false;
        }
        let Some(signal) = obs.mapped.signal() else {
            return false;
        };
        // SIGABRT is typically 6 on Unix.
        if signal != 6 {
            return false;
        }
        // Godot uses separate argv tokens (`--quit-after` then `N`), not `--quit-after=N`.
        obs.argv
            .iter()
            .any(|arg| arg == "--quit" || arg == "--quit-after")
    }
}

/// Stack-canary bind abort (primary user capture): SIGABRT + report evidence.
pub struct StackChkBindAbortRule;

const STACK_CHK_MARKERS: &[&str] = &[
    "stack buffer overflow",
    "__stack_chk_fail",
    "stack_chk_fail",
];

const BIND_SITE_DENYLIST: &[&str] = &[
    "libeosg",
    "IEOS::_bind_methods",
    "IEOS::_bind_methods()",
    "bind_static_method",
];

impl ExitNoiseRule for StackChkBindAbortRule {
    fn id(&self) -> &'static str {
        "godot-stack-chk-bind-abort"
    }

    fn confidence(&self) -> Confidence {
        Confidence::Stable
    }

    fn matches(&self, obs: &ExitObservation) -> bool {
        if !cfg!(unix) {
            return false;
        }
        let Some(signal) = obs.mapped.signal() else {
            return false;
        };
        if signal != 6 {
            return false;
        }
        let Some(report) = obs.crash_report_excerpt.as_deref() else {
            // Fail closed without correlator evidence.
            return false;
        };
        let report_lower = report.to_ascii_lowercase();
        let stack_chk = STACK_CHK_MARKERS
            .iter()
            .any(|m| report_lower.contains(&m.to_ascii_lowercase()));
        if !stack_chk {
            return false;
        }
        BIND_SITE_DENYLIST
            .iter()
            .any(|m| report.contains(m) || report_lower.contains(&m.to_ascii_lowercase()))
    }
}

pub fn builtin_rules() -> Vec<Box<dyn ExitNoiseRule>> {
    // More specific stack-chk rule first for accurate notices.
    vec![
        Box::new(StackChkBindAbortRule),
        Box::new(HeadlessQuitSigabrtRule),
    ]
}

pub fn evaluate(
    obs: &ExitObservation,
    rules: &[Box<dyn ExitNoiseRule>],
    allow_experimental: bool,
) -> Option<MatchResult> {
    for rule in rules {
        if rule.confidence() == Confidence::Experimental && !allow_experimental {
            continue;
        }
        if rule.matches(obs) {
            return Some(MatchResult {
                rule_id: rule.id(),
                confidence: rule.confidence(),
            });
        }
    }
    None
}

/// Apply policy: on match return 0, else raw mapped code.
pub fn apply_exit_policy(
    obs: &ExitObservation,
    allow_experimental: bool,
) -> (u8, Option<MatchResult>) {
    let rules = builtin_rules();
    match evaluate(obs, &rules, allow_experimental) {
        Some(m) => (0, Some(m)),
        None => (obs.mapped.code(), None),
    }
}

/// Look for a fresh macOS Diagnostic Report mentioning this process.
/// Read-only; fail closed (returns None) when unavailable.
pub fn correlate_macos_crash_report(
    child_pid: u32,
    binary: &Path,
    child_start: Instant,
    deadline: Duration,
) -> Option<String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (child_pid, binary, child_start, deadline);
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        correlate_macos_crash_report_impl(child_pid, binary, child_start, deadline)
    }
}

#[cfg(target_os = "macos")]
fn correlate_macos_crash_report_impl(
    child_pid: u32,
    binary: &Path,
    child_start: Instant,
    deadline: Duration,
) -> Option<String> {
    let home = env_home()?;
    let dirs = [
        home.join("Library/Logs/DiagnosticReports"),
        home.join("Library/Logs/DiagnosticReports/Retired"),
    ];
    let binary_name = binary
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Godot");
    let started = Instant::now();
    // CrashReporter writes asynchronously; poll briefly.
    while started.elapsed() < deadline {
        if let Some(text) = scan_report_dirs(&dirs, child_pid, binary_name, binary, child_start) {
            return Some(text);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    scan_report_dirs(&dirs, child_pid, binary_name, binary, child_start)
}

#[cfg(target_os = "macos")]
fn env_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn scan_report_dirs(
    dirs: &[PathBuf],
    child_pid: u32,
    binary_name: &str,
    binary: &Path,
    child_start: Instant,
) -> Option<String> {
    let pid_str = child_pid.to_string();
    let binary_str = binary.to_string_lossy();
    // Accept reports modified after process start (with small skew).
    let min_mtime = SystemTime::now()
        .checked_sub(child_start.elapsed() + Duration::from_secs(5))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            // Godot-*.ips or Godot-*.crash
            let looks_relevant = name.contains("Godot")
                || name.contains(binary_name)
                || name.to_ascii_lowercase().contains("godot");
            if !looks_relevant {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if let Ok(modified) = meta.modified() {
                if modified < min_mtime {
                    continue;
                }
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            // Fail closed: require a PID match so an unrelated recent Godot report
            // cannot reclassify this child's SIGABRT.
            let pid_hit = text.contains(&format!("[{pid_str}]"))
                || text.contains(&format!("\"pid\" : {pid_str}"))
                || text.contains(&format!("\"pid\":{pid_str}"))
                || text.contains(&format!("pid: {pid_str}"));
            if !pid_hit {
                continue;
            }
            let path_hit = text.contains(binary_str.as_ref())
                || text.contains(binary_name)
                || text.contains("Godot");
            if path_hit {
                return Some(text);
            }
        }
    }
    None
}

/// Test helper: build observation with optional report text.
#[cfg(test)]
pub fn observation_for_test(
    mapped: MappedExit,
    argv: &[&str],
    report: Option<&str>,
) -> ExitObservation {
    ExitObservation {
        mapped,
        argv: argv.iter().map(|s| (*s).to_owned()).collect(),
        binary: PathBuf::from("/Applications/Godot.app/Contents/MacOS/Godot"),
        duration: Duration::from_millis(120),
        crash_report_excerpt: report.map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_code_to_u8_preserves_windows_crash_low_byte() {
        // ACCESS_VIOLATION / STACK_BUFFER_OVERRUN must not become 0.
        assert_eq!(process_code_to_u8(0xC000_0005u32 as i32), 0x05);
        assert_eq!(process_code_to_u8(0xC000_0409u32 as i32), 0x09);
        assert_eq!(process_code_to_u8(0), 0);
        assert_eq!(process_code_to_u8(1), 1);
        assert_eq!(process_code_to_u8(256), 1);
        let mapped = MappedExit::Exited {
            code: process_code_to_u8(0xC000_0005u32 as i32),
        };
        let obs = observation_for_test(mapped, &["--editor"], None);
        let (code, matched) = apply_exit_policy(&obs, false);
        assert_eq!(code, 0x05);
        assert!(matched.is_none());
    }

    #[test]
    fn headless_quit_matches_sigabrt_with_quit_flag() {
        let obs = observation_for_test(
            MappedExit::Signaled {
                signal: 6,
                core_dumped: false,
                code: 134,
            },
            &["--path", "proj", "--quit"],
            None,
        );
        let m = evaluate(&obs, &builtin_rules(), false).unwrap();
        assert_eq!(m.rule_id, "godot-headless-quit-sigabrt");
    }

    #[test]
    fn headless_quit_ignores_sigabrt_without_quit() {
        let obs = observation_for_test(
            MappedExit::Signaled {
                signal: 6,
                core_dumped: false,
                code: 134,
            },
            &["--editor"],
            None,
        );
        assert!(evaluate(&obs, &builtin_rules(), false).is_none());
    }

    #[test]
    fn headless_quit_ignores_sigsegv_even_with_quit() {
        let obs = observation_for_test(
            MappedExit::Signaled {
                signal: 11,
                core_dumped: false,
                code: 139,
            },
            &["--quit"],
            None,
        );
        assert!(evaluate(&obs, &builtin_rules(), false).is_none());
    }

    #[test]
    fn stack_chk_matches_capture_signature() {
        let report = r#"
Exception Type:    EXC_CRASH (SIGABRT)
Application Specific Information:
stack buffer overflow
4   libeosg.macos.template_debug  __stack_chk_fail
5   libeosg.macos.template_debug  godot::IEOS::_bind_methods() + 4180
"#;
        let obs = observation_for_test(
            MappedExit::Signaled {
                signal: 6,
                core_dumped: false,
                code: 134,
            },
            &["--editor"],
            Some(report),
        );
        let m = evaluate(&obs, &builtin_rules(), false).unwrap();
        assert_eq!(m.rule_id, "godot-stack-chk-bind-abort");
    }

    #[test]
    fn stack_chk_without_denylist_site_does_not_match() {
        let report = r#"
stack buffer overflow
__stack_chk_fail
some_other_library.dylib
"#;
        let obs = observation_for_test(
            MappedExit::Signaled {
                signal: 6,
                core_dumped: false,
                code: 134,
            },
            &["--editor"],
            Some(report),
        );
        // No libeosg / IEOS / bind_static_method — fail closed for primary rule;
        // headless also fails without --quit.
        assert!(evaluate(&obs, &builtin_rules(), false).is_none());
    }

    #[test]
    fn stack_chk_without_report_does_not_match() {
        let obs = observation_for_test(
            MappedExit::Signaled {
                signal: 6,
                core_dumped: false,
                code: 134,
            },
            &["--editor"],
            None,
        );
        assert!(evaluate(&obs, &builtin_rules(), false).is_none());
    }

    #[test]
    fn apply_policy_rewrites_match_to_zero() {
        let obs = observation_for_test(
            MappedExit::Signaled {
                signal: 6,
                core_dumped: false,
                code: 134,
            },
            &["--quit"],
            None,
        );
        let (code, matched) = apply_exit_policy(&obs, false);
        assert_eq!(code, 0);
        assert!(matched.is_some());
    }

    #[test]
    fn apply_policy_passes_through_exit_one() {
        let obs = observation_for_test(MappedExit::Exited { code: 1 }, &["--quit"], None);
        let (code, matched) = apply_exit_policy(&obs, false);
        assert_eq!(code, 1);
        assert!(matched.is_none());
    }
}
