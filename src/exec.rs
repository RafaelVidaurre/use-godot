//! Direct and wrap-mode Godot execution.

use std::{path::Path, process::Command};

#[cfg(not(windows))]
use std::{process::Stdio, time::Instant};

use anyhow::{Context, Result};

use crate::config::ExitNoisePolicy;
#[cfg(not(windows))]
use crate::exit_noise::{
    ExitObservation, apply_exit_policy, correlate_macos_crash_report, map_exit_status,
};

#[cfg(any(windows, test))]
#[path = "shutdown_output.rs"]
mod shutdown_output;

/// Run Godot: `exec(2)` when tolerate is off (Unix); wrap + policy when on.
pub fn run_godot(binary: &Path, args: &[String], policy: ExitNoisePolicy) -> Result<u8> {
    if policy.tolerate {
        wrap_execute(binary, args, policy)
    } else {
        direct_execute(binary, args)
    }
}

#[cfg(unix)]
fn direct_execute(binary: &Path, args: &[String]) -> Result<u8> {
    use std::os::unix::process::CommandExt;

    let mut command = Command::new(binary);
    command.args(args);
    let error = command.exec();
    Err(error).with_context(|| format!("execute {}", binary.display()))
}

#[cfg(not(unix))]
fn direct_execute(binary: &Path, args: &[String]) -> Result<u8> {
    use crate::exit_noise::process_code_to_u8;

    let status = Command::new(binary)
        .args(args)
        .status()
        .with_context(|| format!("execute {}", binary.display()))?;
    Ok(status.code().map(process_code_to_u8).unwrap_or(1))
}

#[cfg(not(windows))]
fn wrap_execute(binary: &Path, args: &[String], policy: ExitNoisePolicy) -> Result<u8> {
    let t0 = Instant::now();
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawn {}", binary.display()))?;

    let child_pid = child.id();
    let status = child
        .wait()
        .with_context(|| format!("wait for {}", binary.display()))?;
    let mapped = map_exit_status(&status);

    // Fast path: rules that need only wait status + argv (e.g. headless --quit).
    let mut obs = ExitObservation {
        mapped: mapped.clone(),
        argv: args.to_vec(),
        binary: binary.to_path_buf(),
        duration: t0.elapsed(),
        crash_report_excerpt: None,
    };
    let (mut code, mut matched) = apply_exit_policy(&obs, policy.allow_experimental_rules);

    // Correlate macOS crash reports only if still unmatched and SIGABRT (primary stack-chk rule).
    if matched.is_none() {
        #[cfg(unix)]
        {
            use crate::exit_noise::MappedExit;
            if matches!(&mapped, MappedExit::Signaled { signal: 6, .. }) {
                obs.crash_report_excerpt = correlate_macos_crash_report(
                    child_pid,
                    binary,
                    t0,
                    std::time::Duration::from_secs(2),
                );
                let second = apply_exit_policy(&obs, policy.allow_experimental_rules);
                code = second.0;
                matched = second.1;
            }
        }
        #[cfg(not(unix))]
        {
            let _ = child_pid;
        }
    }
    if let Some(m) = matched {
        if !policy.quiet {
            let raw = obs.mapped.code();
            eprintln!("ug: tolerated exit noise: {} (raw status {raw})", m.rule_id);
        }
    }
    Ok(code)
}

#[cfg(windows)]
fn wrap_execute(binary: &Path, args: &[String], policy: ExitNoisePolicy) -> Result<u8> {
    use std::{io, process::Stdio, thread};

    // Windows Godot builds emit shutdown diagnostics on either stream. Drain
    // both concurrently, preserving bytes and ordering within each stream.
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {}", binary.display()))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let out = thread::spawn(move || shutdown_output::drain(stdout, io::stdout()));
    let err = thread::spawn(move || shutdown_output::drain(stderr, io::stderr()));
    let status = child.wait();
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let success = status.as_ref().is_ok_and(|status| status.success());
    // Join both readers even when one fails, and replay pending bytes on failure.
    let out = out
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader panicked"))
        .and_then(|result| result.context("read Godot stdout"));
    let err = err
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader panicked"))
        .and_then(|result| result.context("read Godot stderr"));
    let success = success && out.is_ok() && err.is_ok();
    let out = out.and_then(|tail| {
        tail.finish(success, io::stdout())
            .context("write Godot stdout")
    });
    let err = err.and_then(|tail| {
        tail.finish(success, io::stderr())
            .context("write Godot stderr")
    });
    let count = out? + err?;
    let status = status.with_context(|| format!("wait for {}", binary.display()))?;
    if count > 0 && !policy.quiet {
        eprintln!(
            "ug: tolerated exit noise: godot-windows-shutdown-leaks ({count} diagnostics, raw status 0)"
        );
    }
    Ok(crate::exit_noise::map_exit_status(&status).code())
}
