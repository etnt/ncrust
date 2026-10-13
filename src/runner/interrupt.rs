//! Ctrl-C while waiting for the server ends the run quietly, as in the Python script.
//!
//! The signal is caught only while an `Interrupt` is armed. Otherwise it keeps its usual
//! effect, so a Ctrl-C during connecting, or while the user is typing in interactive mode,
//! still ends the program at once.
//!
//! May depend on: nothing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use signal_hook::consts::SIGINT;

struct Flags {
    /// While set, SIGINT takes its default action.
    default_action: Arc<AtomicBool>,
    interrupted: Arc<AtomicBool>,
}

/// The handlers cannot be removed again, so they are installed once and switched by flags.
fn flags() -> Option<&'static Flags> {
    static FLAGS: OnceLock<Option<Flags>> = OnceLock::new();
    FLAGS
        .get_or_init(|| {
            let default_action = Arc::new(AtomicBool::new(true));
            let interrupted = Arc::new(AtomicBool::new(false));
            // Actions run in the order they were registered.
            signal_hook::flag::register_conditional_default(SIGINT, default_action.clone()).ok()?;
            signal_hook::flag::register(SIGINT, interrupted.clone()).ok()?;
            Some(Flags {
                default_action,
                interrupted,
            })
        })
        .as_ref()
}

pub struct Interrupt {
    armed: bool,
}

impl Interrupt {
    /// Starts catching SIGINT. If that is not possible, Ctrl-C keeps its usual effect.
    pub fn arm() -> Self {
        match flags() {
            Some(flags) => {
                flags.interrupted.store(false, Ordering::SeqCst);
                flags.default_action.store(false, Ordering::SeqCst);
                Interrupt { armed: true }
            }
            None => Self::never(),
        }
    }

    /// An interrupt that never triggers.
    pub fn never() -> Self {
        Interrupt { armed: false }
    }

    pub fn triggered(&self) -> bool {
        self.armed && flags().is_some_and(|f| f.interrupted.load(Ordering::SeqCst))
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        if let (true, Some(flags)) = (self.armed, flags()) {
            flags.default_action.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interrupt_that_never_triggers_stays_quiet() {
        assert!(!Interrupt::never().triggered());
    }

    #[test]
    fn a_sigint_sets_the_flag_while_armed() {
        let interrupt = Interrupt::arm();
        assert!(!interrupt.triggered());
        signal_hook::low_level::raise(SIGINT).unwrap();
        assert!(interrupt.triggered());
    }
}
