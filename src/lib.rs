//! A crate with an "easy" child process API called [`EasyCommand`] that facilitates common use
//! cases for using child processes. It offers the following:
//!
//! * A "nice" [`Display`] implementation that
//! * Straightforward error-handling; you should have most of the context you need for debugging
//!   from the errors that this API returns, minus anything application-specific you wish to add on
//!   top.
//! * Logging using the [`log`] crate.

use std::{
    ffi::OsStr,
    fmt::{self, Debug, Display, Formatter},
    io,
    iter::once,
    process::{Command, ExitStatus, Output},
};

/// A convenience API around [`Command`].
pub struct EasyCommand {
    inner: Command,
}

impl EasyCommand {
    /// Equivalent to [`Command::new`].
    pub fn new<C>(cmd: C) -> Self
    where
        C: AsRef<OsStr>,
    {
        Self::new_with(cmd, |cmd| cmd)
    }

    /// A convenience constructor that allows other method calls to be chained onto this one.
    pub fn new_with<C>(cmd: C, f: impl FnOnce(&mut Command) -> &mut Command) -> Self
    where
        C: AsRef<OsStr>,
    {
        let mut cmd = Command::new(cmd);
        f(&mut cmd);
        Self { inner: cmd }
    }

    /// Like [`Self::new_with`], but optimized for ergonomic usage of an [`IntoIterator`] for
    /// arguments.
    pub fn simple<C, A, I>(cmd: C, args: I) -> Self
    where
        C: AsRef<OsStr>,
        A: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
    {
        Self::new_with(cmd, |cmd| cmd.args(args))
    }

    fn spawn_and_wait_impl(&mut self) -> Result<ExitStatus, SpawnAndWaitErrorKind> {
        log::debug!("spawning child process with {self}…");

        self.inner
            .spawn()
            .map_err(|source| SpawnAndWaitErrorKind::Spawn { source })
            .and_then(|mut child| {
                log::trace!("waiting for exit from {self}…");
                let status = child
                    .wait()
                    .map_err(|source| SpawnAndWaitErrorKind::WaitForExitCode { source })?;
                log::debug!("received exit code {:?} from {self}", status.code());
                Ok(status)
            })
    }

    /// Execute this command, returning its exit status.
    ///
    /// This command wraps around [`Command::spawn`], which causes `stdout` and `stderr` to be
    /// inherited from its parent.
    pub fn spawn_and_wait(&mut self) -> Result<ExitStatus, ExecuteError<SpawnAndWaitErrorKind>> {
        self.spawn_and_wait_impl()
            .map_err(|source| ExecuteError::new(self, source))
    }

    fn run_impl(&mut self) -> Result<(), RunErrorKind> {
        let status = self.spawn_and_wait_impl()?;

        if status.success() {
            Ok(())
        } else {
            Err(RunErrorKind::UnsuccessfulExitCode {
                code: status.code(),
            })
        }
    }

    /// Execute this command, returning an error if it did not return a successful exit code.
    ///
    /// This command wraps around [`Command::spawn`], which causes `stdout` and `stderr` to be
    /// inherited from its parent.
    pub fn run(&mut self) -> Result<(), ExecuteError<RunErrorKind>> {
        self.run_impl()
            .map_err(|source| ExecuteError::new(self, source))
    }

    fn output_impl(&mut self) -> Result<Output, io::Error> {
        log::debug!("getting output from {self}…");
        let output = self.inner.output()?;
        log::debug!("received exit code {:?} from {self}", output.status.code());
        Ok(output)
    }

    /// Execute this command, capturing its output.
    pub fn output(&mut self) -> Result<Output, ExecuteError<io::Error>> {
        self.output_impl()
            .map_err(|source| ExecuteError::new(self, source))
    }
}

impl Debug for EasyCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.inner, f)
    }
}

impl Display for EasyCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Self { inner } = self;
        let prog = inner.get_program().to_string_lossy();
        let args = inner.get_args().map(|a| a.to_string_lossy());
        let shell_words = ::shell_words::join(once(prog).chain(args));
        write!(f, "`{shell_words}`")
    }
}

#[derive(Debug)]
struct EasyCommandInvocation {
    shell_words: String,
}

impl EasyCommandInvocation {
    fn new(cmd: &EasyCommand) -> Self {
        let EasyCommand { inner } = cmd;
        let prog = inner.get_program().to_string_lossy();
        let args = inner.get_args().map(|a| a.to_string_lossy());
        let shell_words = ::shell_words::join(once(prog).chain(args));
        Self { shell_words }
    }
}

impl Display for EasyCommandInvocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Self { shell_words } = self;
        write!(f, "{shell_words}")
    }
}

/// An error returned by [`EasyCommand`]'s methods.
#[derive(Debug, thiserror::Error)]
#[error("failed to execute {cmd}")]
pub struct ExecuteError<E> {
    cmd: EasyCommandInvocation,
    pub source: E,
}

impl<E> ExecuteError<E> {
    fn new(cmd: &EasyCommand, source: E) -> Self {
        Self {
            cmd: EasyCommandInvocation::new(cmd),
            source,
        }
    }
}

/// The specific error case encountered with [`EasyCommand::spawn_and_wait`].
#[derive(Debug, thiserror::Error)]
pub enum SpawnAndWaitErrorKind {
    #[error("failed to spawn")]
    Spawn { source: io::Error },
    #[error("failed to wait for exit code")]
    WaitForExitCode { source: io::Error },
}

/// The specific error case encountered with a [`EasyCommand::run`].
#[derive(Debug, thiserror::Error)]
pub enum RunErrorKind {
    #[error(transparent)]
    SpawnAndWait(#[from] SpawnAndWaitErrorKind),
    #[error("returned exit code {code:?}")]
    UnsuccessfulExitCode { code: Option<i32> },
}
