//! Runs a built test runner, streaming its `RUN` / `PASS` / `FAIL` / `LEAK` protocol, enforcing
//! per-test `@timeout`s, and attributing a trap or exit to the test that was running.

use super::project::TestHost;
use super::runner::PlannedTest;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

const LEAK_MARKER: &str = "[dream] leak check: live=";

#[derive(Debug, Clone)]
pub struct HostReport {
    pub host: TestHost,
    pub passed: Vec<String>,
    /// `(test label, reason)`.
    pub failed: Vec<(String, String)>,
    pub not_run: Vec<String>,
    /// `LEAK` lines: tests whose live-object count grew; hints, not failures.
    pub leak_hints: Vec<String>,
    /// A failure not tied to one test (the exit-time leak gate, an exit after the last test).
    pub problem: Option<String>,
}

impl HostReport {
    pub fn ok(&self) -> bool {
        self.failed.is_empty() && self.not_run.is_empty() && self.problem.is_none()
    }
}

/// Runs `cmd` and reads its report against `plan`. Output that is not part of the protocol is
/// passed through, so a test's own prints stay visible.
pub fn run_and_report(
    mut cmd: Command,
    host: TestHost,
    plan: &[PlannedTest],
) -> Result<HostReport, String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("starting the {} test runner: {e}", host.as_str()))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let (tx, rx) = mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });

    let mut state = State::new(host);
    loop {
        let next = match state.deadline {
            Some(deadline) => rx.recv_timeout(deadline.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match next {
            Ok(line) => state.line(&line),
            Err(RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                state.timed_out = true;
                break;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child
        .wait()
        .map_err(|e| format!("waiting for the {} test runner: {e}", host.as_str()))?;
    // A killed runner's own children can keep its pipes open, so its readers are left behind.
    let stderr = if state.timed_out {
        String::new()
    } else {
        let _ = reader.join();
        errors.join().unwrap_or_default()
    };
    Ok(state.finish(status, &stderr, plan))
}

struct State {
    report: HostReport,
    current: Option<(String, Option<u64>)>,
    deadline: Option<Instant>,
    last_fail: Option<String>,
    timed_out: bool,
}

impl State {
    fn new(host: TestHost) -> Self {
        Self {
            report: HostReport {
                host,
                passed: Vec::new(),
                failed: Vec::new(),
                not_run: Vec::new(),
                leak_hints: Vec::new(),
                problem: None,
            },
            current: None,
            deadline: None,
            last_fail: None,
            timed_out: false,
        }
    }

    fn line(&mut self, line: &str) {
        if let Some(rest) = line.strip_prefix("RUN ") {
            let (label, timeout) = match rest.split_once(" timeout=") {
                Some((label, ms)) => (label, ms.trim().parse::<u64>().ok()),
                None => (rest, None),
            };
            self.deadline = timeout.map(|ms| Instant::now() + Duration::from_millis(ms));
            self.current = Some((label.to_string(), timeout));
            self.last_fail = None;
        } else if let Some(label) = line.strip_prefix("PASS ")
            && self.current.as_ref().is_some_and(|(c, _)| c == label)
        {
            println!("test {label} ... ok");
            self.report.passed.push(label.to_string());
            self.current = None;
            self.deadline = None;
        } else if let Some(rest) = line.strip_prefix("LEAK ") {
            self.report.leak_hints.push(rest.to_string());
        } else {
            if let Some(message) = line.strip_prefix("FAIL ") {
                self.last_fail = Some(message.to_string());
            }
            println!("{line}");
        }
    }

    fn finish(mut self, status: ExitStatus, stderr: &str, plan: &[PlannedTest]) -> HostReport {
        let leak_live = stderr.find(LEAK_MARKER).and_then(|i| {
            let digits: String = stderr[i + LEAK_MARKER.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            digits.parse::<i64>().ok()
        });
        let shown: Vec<&str> = stderr
            .lines()
            .filter(|l| leak_live.is_some_and(|n| n != 0) || !l.starts_with("[dream] leak"))
            .collect();
        if !shown.is_empty() {
            eprintln!("{}", shown.join("\n"));
        }

        let report = &mut self.report;
        if let Some((label, timeout)) = self.current.take() {
            let reason = if self.timed_out {
                format!("TIMEOUT after {} ms", timeout.unwrap_or_default())
            } else if let Some(message) = self.last_fail.take() {
                message
            } else {
                format!("the runner stopped ({})", describe(status))
            };
            println!("test {label} ... FAILED");
            report.failed.push((label, reason));
        } else if !status.success() {
            report.problem = Some(format!(
                "the runner exited after its last test ({})",
                describe(status)
            ));
        }
        for test in plan {
            let seen = report.passed.contains(&test.label)
                || report.failed.iter().any(|(label, _)| *label == test.label);
            if !seen {
                report.not_run.push(test.label.clone());
            }
        }
        if report.problem.is_none()
            && let Some(live) = leak_live.filter(|&n| n != 0)
        {
            report.problem = Some(format!(
                "leak check: {live} object(s) still live at exit (want 0)"
            ));
        }
        self.report
    }
}

fn describe(status: ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit code {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("killed by signal {signal}");
        }
    }
    format!("{status}")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn plan(labels: &[&str]) -> Vec<PlannedTest> {
        labels
            .iter()
            .map(|l| PlannedTest {
                is_async: false,
                is_unsafe: false,
                label: l.to_string(),
                function: l.to_string(),
                timeout_ms: None,
            })
            .collect()
    }

    #[cfg(unix)]
    fn shell(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[cfg(unix)]
    #[test]
    fn attributes_a_failing_exit_to_the_running_test() {
        let report = run_and_report(
            shell("echo 'RUN a'; echo 'PASS a'; echo 'RUN b'; echo 'FAIL boom'; exit 1"),
            TestHost::Native,
            &plan(&["a", "b", "c"]),
        )
        .unwrap();
        assert_eq!(report.passed, vec!["a"]);
        assert_eq!(report.failed, vec![("b".to_string(), "boom".to_string())]);
        assert_eq!(report.not_run, vec!["c"]);
        assert!(!report.ok());
    }

    #[cfg(unix)]
    #[test]
    fn kills_a_test_that_exceeds_its_timeout() {
        let started = Instant::now();
        let report = run_and_report(
            shell("echo 'RUN slow timeout=100'; sleep 5; echo 'PASS slow'"),
            TestHost::Native,
            &plan(&["slow"]),
        )
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(report.failed[0].1, "TIMEOUT after 100 ms");
    }

    #[cfg(unix)]
    #[test]
    fn exit_leak_report_is_the_gate_and_leak_lines_are_hints() {
        let report = run_and_report(
            shell(
                "echo 'RUN a'; echo 'LEAK a live=+2'; echo 'PASS a'; \
                 echo '[dream] leak check: live=2 total_allocations=9' >&2",
            ),
            TestHost::Native,
            &plan(&["a"]),
        )
        .unwrap();
        assert_eq!(report.passed, vec!["a"]);
        assert_eq!(report.leak_hints, vec!["a live=+2"]);
        assert!(report.problem.as_deref().unwrap().contains("2 object"));

        let clean = run_and_report(
            shell("echo 'RUN a'; echo 'PASS a'; echo '[dream] leak check: live=0' >&2"),
            TestHost::Native,
            &plan(&["a"]),
        )
        .unwrap();
        assert!(clean.ok());
    }
}
