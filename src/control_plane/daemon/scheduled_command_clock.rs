/// Minute boundary state for daemon-owned project schedules.
#[derive(Default)]
pub(crate) struct ScheduledCommandClock {
    last_minute: Option<i64>,
}

impl ScheduledCommandClock {
    /// Claims the current minute once without replaying any missed minutes.
    pub(crate) fn take_due(&mut self, now_unix_seconds: i64) -> bool {
        let current_minute = now_unix_seconds / 60;
        match self.last_minute {
            None => {
                self.last_minute = Some(current_minute);
                true
            }
            Some(last_minute) if current_minute > last_minute => {
                self.last_minute = Some(current_minute);
                true
            }
            Some(last_minute) if current_minute < last_minute => {
                self.last_minute = Some(current_minute);
                false
            }
            Some(_) => false,
        }
    }
}
