//! Original process-group identity after collection of its leader.

use rustix::fd::AsFd as _;
use rustix::{io, process};
use serial_test::serial;
use std::io::{Read as _, Write as _};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};

// Refuse unsupported scope before any held fixture child exists. A process
// which is not itself a group leader can legitimately return ESRCH here.
fn admit_group_scope() {
    let fd = process::pidfd_open(process::getpid(), process::PidfdFlags::empty()).unwrap();
    match process::pidfd_send_signal_with_flags(&fd, None, process::PidfdSignalFlags::PROCESS_GROUP)
    {
        Ok(()) | Err(io::Errno::SRCH) => (),
        result => panic!("process-group pidfd scope unavailable: {:?}", result),
    }
}

#[test]
#[serial]
fn process_group_probe_requires_original_leader_collection() {
    admit_group_scope();
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("printf ready; read release; printf out; printf err >&2; exit 29")
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    let mut errors = child.stderr.take().unwrap();
    let mut ready = [0; 5];
    let readiness = output.read_exact(&mut ready);
    let fd = process::pidfd_open(
        process::Pid::from_child(&child),
        process::PidfdFlags::empty(),
    );
    let group = process::PidfdSignalFlags::PROCESS_GROUP;
    let observations = fd.as_ref().ok().map(|fd| {
        [
            process::pidfd_send_signal_with_flags(fd, None, process::PidfdSignalFlags::empty()),
            process::pidfd_send_signal_with_flags(fd, None, group),
            process::pidfd_send_signal_with_flags(
                fd,
                None,
                group | process::PidfdSignalFlags::THREAD_GROUP,
            ),
            process::pidfd_send_signal_with_flags(
                fd,
                None,
                process::PidfdSignalFlags::from_bits_retain(1 << 31),
            ),
        ]
    });
    let released = input.write_all(b"x\n");
    drop(input);
    let observed_exit = process::waitid(
        process::WaitId::Pid(process::Pid::from_child(&child)),
        process::WaitIdOptions::EXITED | process::WaitIdOptions::NOWAIT,
    );
    let zombie_probe = fd
        .as_ref()
        .ok()
        .map(|fd| process::pidfd_send_signal_with_flags(fd, None, group));
    let status = child.wait();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let stdout_eof = output.read_to_end(&mut stdout);
    let stderr_eof = errors.read_to_end(&mut stderr);
    let completed = fd.as_ref().ok().map(|fd| {
        (
            process::pidfd_send_signal_with_flags(fd, None, group),
            process::waitid(
                process::WaitId::PidFd(fd.as_fd()),
                process::WaitIdOptions::EXITED,
            ),
        )
    });
    // Every operation result is compared only after the original physical
    // child wait and both original stream EOFs, even when registration failed.
    readiness.unwrap();
    released.unwrap();
    stdout_eof.unwrap();
    stderr_eof.unwrap();
    fd.unwrap();
    assert_eq!(ready, *b"ready");
    assert_eq!(
        observations,
        Some([Ok(()), Ok(()), Err(io::Errno::INVAL), Err(io::Errno::INVAL)])
    );
    assert_eq!(observed_exit.unwrap().unwrap().exit_status(), Some(29));
    assert_eq!(zombie_probe, Some(Ok(())));
    assert_eq!(status.unwrap().code(), Some(29));
    assert_eq!(stdout, b"out");
    assert_eq!(stderr, b"err");
    let (probe, wait) = completed.unwrap();
    assert_eq!(probe, Err(io::Errno::SRCH));
    assert_eq!(wait.unwrap_err(), io::Errno::CHILD);
}

#[test]
#[serial]
fn original_group_fd_signals_residual_after_leader_collection() {
    admit_group_scope();
    let mut leader = Command::new("sh")
        .arg("-c")
        .arg("printf ready; read release; exit 29")
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut leader_input = leader.stdin.take().unwrap();
    let mut leader_output = leader.stdout.take().unwrap();
    let mut leader_errors = leader.stderr.take().unwrap();
    let mut leader_ready = [0; 5];
    let leader_readiness = leader_output.read_exact(&mut leader_ready);
    let fd = process::pidfd_open(
        process::Pid::from_child(&leader),
        process::PidfdFlags::empty(),
    );
    let residual = Command::new("sh")
        .arg("-c")
        .arg("printf ready; read release; printf out; printf err >&2; exit 17")
        .process_group(leader.id() as i32)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut residual = match residual {
        Ok(child) => child,
        Err(error) => {
            drop(leader_input);
            let status = leader.wait();
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let stdout_eof = leader_output.read_to_end(&mut stdout);
            let stderr_eof = leader_errors.read_to_end(&mut stderr);
            panic!("residual spawn refused: {error}; leader={status:?} EOF={stdout_eof:?}/{stderr_eof:?}");
        }
    };
    let mut residual_input = Some(residual.stdin.take().unwrap());
    let mut residual_output = residual.stdout.take().unwrap();
    let mut residual_errors = residual.stderr.take().unwrap();
    let mut residual_ready = [0; 5];
    let residual_readiness = residual_output.read_exact(&mut residual_ready);
    let released = leader_input.write_all(b"x\n");
    drop(leader_input);
    let leader_status = leader.wait();
    let group = process::PidfdSignalFlags::PROCESS_GROUP;
    let effects = fd.as_ref().ok().map(|fd| {
        (
            process::pidfd_send_signal_with_flags(fd, None, group),
            process::pidfd_send_signal_with_flags(fd, Some(process::Signal::KILL), group),
        )
    });
    // A refused signal must finish the known stdin-held fixture naturally;
    // that exit remains a failed SIGKILL oracle rather than a successful effect.
    if !matches!(effects.as_ref(), Some((_, Ok(())))) {
        drop(residual_input.take());
    }
    let residual_status = residual.wait();
    drop(residual_input);
    let mut leader_stdout = Vec::new();
    let mut leader_stderr = Vec::new();
    let mut residual_stdout = Vec::new();
    let mut residual_stderr = Vec::new();
    let eofs = [
        leader_output.read_to_end(&mut leader_stdout),
        leader_errors.read_to_end(&mut leader_stderr),
        residual_output.read_to_end(&mut residual_stdout),
        residual_errors.read_to_end(&mut residual_stderr),
    ];
    let completed = fd.as_ref().ok().map(|fd| {
        (
            process::pidfd_send_signal_with_flags(fd, None, group),
            process::waitid(
                process::WaitId::PidFd(fd.as_fd()),
                process::WaitIdOptions::EXITED,
            ),
        )
    });
    leader_readiness.unwrap();
    residual_readiness.unwrap();
    released.unwrap();
    for eof in eofs {
        eof.unwrap();
    }
    fd.unwrap();
    assert_eq!((leader_ready, residual_ready), (*b"ready", *b"ready"));
    assert_eq!(leader_status.unwrap().code(), Some(29));
    assert_eq!(effects, Some((Ok(()), Ok(()))));
    assert_eq!(
        residual_status.unwrap().signal(),
        Some(process::Signal::KILL.as_raw())
    );
    assert_eq!(
        (
            leader_stdout,
            leader_stderr,
            residual_stdout,
            residual_stderr
        ),
        (vec![], vec![], vec![], vec![])
    );
    let (probe, wait) = completed.unwrap();
    assert_eq!(probe, Err(io::Errno::SRCH));
    assert_eq!(wait.unwrap_err(), io::Errno::CHILD);
}
