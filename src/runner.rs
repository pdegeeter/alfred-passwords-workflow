//! Thin abstraction over the impure boundary: spawning external processes.
//!
//! Everything that talks to `apw` goes through [`CommandRunner`]. Production
//! code uses [`SystemRunner`]; tests inject a fake so the rest of the crate
//! stays deterministic and offline.

use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};

/// What a finished command produced. A non-zero exit is not an error here:
/// `apw` reports its failures as JSON on stderr with a meaningful exit code,
/// which the caller decodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Abstraction over running an external command.
pub trait CommandRunner {
    /// Run `program` with `args`, feeding `stdin` when given, and return what
    /// it printed once it exits.
    ///
    /// Errors only when the command could not run at all: a missing program
    /// yields an error that downcasts to [`NotFound`], one that outlives
    /// `timeout` is killed and yields [`TimedOut`].
    fn run(
        &self,
        program: &str,
        args: &[&str],
        stdin: Option<&str>,
        timeout: Duration,
    ) -> Result<CommandOutput>;
}

/// The program does not exist at the given path or on `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFound {
    pub program: String,
}

impl fmt::Display for NotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` was not found", self.program)
    }
}

impl std::error::Error for NotFound {}

/// The program was killed because it outlived its timeout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedOut {
    pub program: String,
    pub timeout: Duration,
}

impl fmt::Display for TimedOut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` did not answer within {} seconds",
            self.program,
            self.timeout.as_secs()
        )
    }
}

impl std::error::Error for TimedOut {}

/// Real implementation backed by [`std::process::Command`].
pub struct SystemRunner;

impl SystemRunner {
    /// How often [`CommandRunner::run`] checks whether the command exited.
    const POLL_INTERVAL: Duration = Duration::from_millis(20);

    /// How long to wait for the output pipes to close once the command exited.
    const DRAIN_GRACE: Duration = Duration::from_millis(500);

    fn read_pipe(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<String> {
        let (sender, receiver) = mpsc::channel();

        if let Some(mut pipe) = pipe {
            thread::spawn(move || {
                let mut buffer = Vec::new();
                let _ = pipe.read_to_end(&mut buffer);

                let _ = sender.send(String::from_utf8_lossy(&buffer).into_owned());
            });
        }

        receiver
    }
}

impl CommandRunner for SystemRunner {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        stdin: Option<&str>,
        timeout: Duration,
    ) -> Result<CommandOutput> {
        let spawned = Command::new(program)
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match spawned {
            Ok(child) => child,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Err(anyhow!(NotFound {
                    program: program.to_owned(),
                }));
            }
            Err(err) => {
                return Err(err).with_context(|| format!("failed to spawn `{program}`"));
            }
        };

        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            pipe.write_all(input.as_bytes())
                .with_context(|| format!("failed to write to the stdin of `{program}`"))?;
        }

        let stdout = SystemRunner::read_pipe(child.stdout.take());
        let stderr = SystemRunner::read_pipe(child.stderr.take());
        let deadline = Instant::now() + timeout;

        let status = loop {
            if let Some(status) = child
                .try_wait()
                .with_context(|| format!("failed to wait for `{program}`"))?
            {
                break status;
            }

            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();

                return Err(anyhow!(TimedOut {
                    program: program.to_owned(),
                    timeout,
                }));
            }

            thread::sleep(SystemRunner::POLL_INTERVAL);
        };

        Ok(CommandOutput {
            success: status.success(),
            stdout: stdout
                .recv_timeout(SystemRunner::DRAIN_GRACE)
                .unwrap_or_default(),
            stderr: stderr
                .recv_timeout(SystemRunner::DRAIN_GRACE)
                .unwrap_or_default(),
        })
    }
}
