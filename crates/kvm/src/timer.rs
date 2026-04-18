//! SIGALRM-based budget timer.
//!
//! [`BudgetTimer`] arms a POSIX per-process timer that fires after a given
//! number of nanoseconds, delivering `SIGALRM` to the calling thread. The
//! signal causes any in-flight `ioctl(KVM_RUN)` to return with `EINTR`, which
//! [`KvmVcpu::run`](crate::KvmVcpu::run) then surfaces as
//! [`VmExit::Interrupted`](crate::VmExit::Interrupted).
//!
//! The first [`BudgetTimer`] created installs a no-op `SIGALRM` handler
//! without `SA_RESTART`, so the signal reliably interrupts blocking syscalls.

use std::{
    io,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use crate::error::Error;

/// Linux-only `SIGEV_THREAD_ID` constant (`libc` does not expose it on
/// `linux_like`).
const SIGEV_THREAD_ID: libc::c_int = 4;

static SIGALRM_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

/// One-shot, thread-targeted budget timer that causes `KVM_RUN` to preempt.
pub struct BudgetTimer {
    timer_id: libc::timer_t,
}

impl BudgetTimer {
    /// Creates a new disarmed timer targeting the calling thread.
    pub fn new() -> Result<Self, Error> {
        install_sigalrm_handler_once()?;

        // SAFETY: gettid is thread-safe and always succeeds on Linux.
        let tid = unsafe { libc::gettid() };

        let mut sigev = sigevent_zero();
        sigev.sigev_notify = SIGEV_THREAD_ID;
        sigev.sigev_signo = libc::SIGALRM;
        sigev.sigev_notify_thread_id = tid;

        let mut timer_id: libc::timer_t = std::ptr::null_mut();
        // SAFETY: `sigev` is fully initialized above; `timer_id` is an out-pointer.
        let ret = unsafe {
            libc::timer_create(
                libc::CLOCK_MONOTONIC,
                &mut sigev as *mut libc::sigevent,
                &mut timer_id as *mut libc::timer_t,
            )
        };
        if ret != 0 {
            return Err(Error::Os(io::Error::last_os_error()));
        }
        Ok(Self { timer_id })
    }

    /// Arms the timer to fire once after `duration` has elapsed.
    ///
    /// A zero duration immediately fires (SIGALRM queued). Re-arming an
    /// already-armed timer is allowed and replaces the previous setting.
    pub fn arm(&self, duration: Duration) -> Result<(), Error> {
        let secs = duration.as_secs() as libc::time_t;
        let nanos = duration.subsec_nanos() as libc::c_long;
        let spec = libc::itimerspec {
            it_interval: libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: libc::timespec {
                tv_sec: secs,
                tv_nsec: nanos,
            },
        };
        // SAFETY: `self.timer_id` is a valid timer created above; `spec` is
        // fully initialized.
        let ret = unsafe {
            libc::timer_settime(
                self.timer_id,
                0,
                &spec as *const libc::itimerspec,
                std::ptr::null_mut(),
            )
        };
        if ret != 0 {
            return Err(Error::Os(io::Error::last_os_error()));
        }
        Ok(())
    }

    /// Disarms the timer.
    pub fn disarm(&self) -> Result<(), Error> {
        self.arm(Duration::ZERO)?;
        // A zero it_value means "disarm" per POSIX timer_settime.
        Ok(())
    }
}

impl Drop for BudgetTimer {
    fn drop(&mut self) {
        // SAFETY: `self.timer_id` was created by `timer_create` above and not
        // deleted yet.
        unsafe {
            libc::timer_delete(self.timer_id);
        }
    }
}

// Send is safe: POSIX timer ids are plain handles that can cross threads.
// Sync is NOT implemented: arming from multiple threads concurrently would
// race on the underlying timer.
unsafe impl Send for BudgetTimer {}

fn install_sigalrm_handler_once() -> Result<(), Error> {
    if SIGALRM_HANDLER_INSTALLED.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let mut action: libc::sigaction = unsafe { MaybeUninit::zeroed().assume_init() };
    let handler: extern "C" fn(libc::c_int) = sigalrm_noop;
    action.sa_sigaction = handler as libc::sighandler_t;
    action.sa_flags = 0; // no SA_RESTART: KVM_RUN must return EINTR.
    // SAFETY: sa_mask is zero-initialized; sigemptyset is an in-place write.
    unsafe {
        libc::sigemptyset(&mut action.sa_mask as *mut libc::sigset_t);
    }
    // SAFETY: installing a process-wide handler for SIGALRM. Handler is an
    // extern "C" function with the correct signature for sa_sigaction use.
    let ret = unsafe {
        libc::sigaction(
            libc::SIGALRM,
            &action as *const libc::sigaction,
            std::ptr::null_mut(),
        )
    };
    if ret != 0 {
        SIGALRM_HANDLER_INSTALLED.store(false, Ordering::Release);
        return Err(Error::Os(io::Error::last_os_error()));
    }
    Ok(())
}

extern "C" fn sigalrm_noop(_signum: libc::c_int) {
    // Intentionally empty. The signal's only purpose is to cause KVM_RUN to
    // return EINTR so the machine loop can re-check its budget.
}

fn sigevent_zero() -> libc::sigevent {
    // SAFETY: `sigevent` is a POD-like struct; zero is a valid bit pattern for
    // every field (checked at runtime by the kernel when we populate the
    // fields we care about).
    unsafe { MaybeUninit::<libc::sigevent>::zeroed().assume_init() }
}
