use crate::fd::OwnedFd;
use crate::process::{Pid, Signal};
use crate::{backend, ffi, io};
use backend::fd::AsFd;

bitflags::bitflags! {
    /// `PIDFD_*` flags for use with [`pidfd_open`].
    ///
    /// [`pidfd_open`]: crate::process::pidfd_open
    #[repr(transparent)]
    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
    pub struct PidfdFlags: ffi::c_uint {
        /// `PIDFD_NONBLOCK`
        const NONBLOCK = backend::c::PIDFD_NONBLOCK;

        /// <https://docs.rs/bitflags/*/bitflags/#externally-defined-flags>
        const _ = !0;
    }
}

bitflags::bitflags! {
    /// Signal-scope flags for [`pidfd_send_signal_with_flags`].
    ///
    /// Explicit scopes require Linux 6.9. The kernel rejects combined scopes
    /// and unsupported flags; this API preserves that refusal.
    #[repr(transparent)]
    #[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
    pub struct PidfdSignalFlags: ffi::c_uint {
        /// `PIDFD_SIGNAL_THREAD`: signal the referenced thread.
        const THREAD = 1 << 0;
        /// `PIDFD_SIGNAL_THREAD_GROUP`: signal the referenced thread group.
        const THREAD_GROUP = 1 << 1;
        /// `PIDFD_SIGNAL_PROCESS_GROUP`: signal the referenced process group.
        const PROCESS_GROUP = 1 << 2;

        /// <https://docs.rs/bitflags/*/bitflags/#externally-defined-flags>
        const _ = !0;
    }
}

/// `syscall(SYS_pidfd_open, pid, flags)`—Creates a file descriptor for a
/// process.
///
/// # References
///  - [Linux]
///
/// [Linux]: https://man7.org/linux/man-pages/man2/pidfd_open.2.html
#[inline]
pub fn pidfd_open(pid: Pid, flags: PidfdFlags) -> io::Result<OwnedFd> {
    backend::process::syscalls::pidfd_open(pid, flags)
}

/// `syscall(SYS_pidfd_send_signal, pidfd, sig, NULL, 0)`—Send a signal to a
/// process specified by a file descriptor.
///
/// # References
///  - [Linux]
///
/// [Linux]: https://man7.org/linux/man-pages/man2/pidfd_send_signal.2.html
#[inline]
pub fn pidfd_send_signal<Fd: AsFd>(pidfd: Fd, sig: Signal) -> io::Result<()> {
    pidfd_send_signal_with_flags(pidfd, Some(sig), PidfdSignalFlags::empty())
}

/// `syscall(SYS_pidfd_send_signal, pidfd, sig, NULL, flags)`—Signal or probe
/// the kernel identity referenced by a process file descriptor.
///
/// `None` requests signal 0: a permissions and existence probe with no signal
/// delivery. Empty flags preserve the descriptor's default signal scope.
/// Explicit scopes require Linux 6.9 and never fall back to a numeric PID.
///
/// A process-group leader's descriptor can address its original process group
/// after that leader is collected. A successful probe includes unreaped zombie
/// members; it does not prove that a group is empty or that a child was reaped.
///
/// # References
///  - [Linux]
///
/// [Linux]: https://man7.org/linux/man-pages/man2/pidfd_send_signal.2.html
#[inline]
pub fn pidfd_send_signal_with_flags<Fd: AsFd>(
    pidfd: Fd,
    sig: Option<Signal>,
    flags: PidfdSignalFlags,
) -> io::Result<()> {
    backend::process::syscalls::pidfd_send_signal(pidfd.as_fd(), sig, flags)
}
