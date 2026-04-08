//! DATE command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
};

pub(crate) struct Date;

impl Command for Date {
    fn name(&self) -> &'static str {
        "DATE"
    }

    fn start(&self, _args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningDate)
    }
}

struct RunningDate;

impl RunningCommand for RunningDate {
    fn step(
        &mut self,
        _state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        // Hardcoded DOS date: 0x1E21 = 1995-01-01
        let date: u16 = 0x1E21;
        let year = ((date >> 9) & 0x7F) + 1980;
        let month = (date >> 5) & 0x0F;
        let day = date & 0x1F;

        // Day of week for 1995-01-01 is Sunday
        let dow = day_of_week(year, month, day);
        let dow_name = match dow {
            0 => "Sun",
            1 => "Mon",
            2 => "Tue",
            3 => "Wed",
            4 => "Thu",
            5 => "Fri",
            _ => "Sat",
        };

        let msg = format!(
            "Current date is {} {:02}-{:02}-{:04}\r\n",
            dow_name, month, day, year
        );
        for &byte in msg.as_bytes() {
            io.console.process_byte(io.memory, byte);
        }
        StepResult::Done(0)
    }
}

/// Zeller-like day-of-week calculation. Returns 0=Sun, 1=Mon, ..., 6=Sat.
fn day_of_week(year: u16, month: u16, day: u16) -> u16 {
    let mut y = year as i32;
    let mut m = month as i32;
    if m <= 2 {
        m += 12;
        y -= 1;
    }
    let q = day as i32;
    let k = y % 100;
    let j = y / 100;
    let h = (q + (13 * (m + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    ((h + 6) % 7) as u16
}
