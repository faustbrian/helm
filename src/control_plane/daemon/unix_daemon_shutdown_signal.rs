use super::DaemonShutdownSignal;
use signal_hook::SigId;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::{flag, low_level};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Process-lifetime SIGINT and SIGTERM observation for the Unix daemon.
pub(crate) struct UnixDaemonShutdownSignal {
    requested: Arc<AtomicBool>,
    registrations: Vec<SigId>,
}

impl UnixDaemonShutdownSignal {
    pub(crate) fn install() -> io::Result<Self> {
        let requested = Arc::new(AtomicBool::new(false));
        let mut registrations = Vec::with_capacity(4);
        if let Err(error) = register_signal(SIGINT, &requested, &mut registrations)
            .and_then(|()| register_signal(SIGTERM, &requested, &mut registrations))
        {
            unregister_all(&registrations);

            return Err(error);
        }

        Ok(Self {
            requested,
            registrations,
        })
    }
}

impl DaemonShutdownSignal for UnixDaemonShutdownSignal {
    fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

impl Drop for UnixDaemonShutdownSignal {
    fn drop(&mut self) {
        unregister_all(&self.registrations);
    }
}

fn register_signal(
    signal: i32,
    requested: &Arc<AtomicBool>,
    registrations: &mut Vec<SigId>,
) -> io::Result<()> {
    registrations.push(flag::register_conditional_shutdown(
        signal,
        1,
        Arc::clone(requested),
    )?);
    registrations.push(flag::register(signal, Arc::clone(requested))?);

    Ok(())
}

fn unregister_all(registrations: &[SigId]) {
    for registration in registrations {
        low_level::unregister(*registration);
    }
}
