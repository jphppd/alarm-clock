//! Datetime structure and methods

/// Datetime structure
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Datetime {
    pub date: Date,
    pub time: Time,
}

/// Dev-friendly representation of the day of the week
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DayOfWeek {
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
    Sunday = 7,
}

impl TryFrom<u8> for DayOfWeek {
    type Error = ();

    /// Convert an u8 to a DayOfWeek
    fn try_from(weekday: u8) -> Result<Self, Self::Error> {
        match weekday {
            1 => Ok(DayOfWeek::Monday),
            2 => Ok(DayOfWeek::Tuesday),
            3 => Ok(DayOfWeek::Wednesday),
            4 => Ok(DayOfWeek::Thursday),
            5 => Ok(DayOfWeek::Friday),
            6 => Ok(DayOfWeek::Saturday),
            7 => Ok(DayOfWeek::Sunday),
            _ => Err(()),
        }
    }
}

impl DayOfWeek {
    /// True for week-end, false otherwise.
    pub fn is_week_end(&self) -> bool {
        matches!(self, DayOfWeek::Saturday | DayOfWeek::Sunday)
    }
}

/// Date structure
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub day: u8,
    pub month: u8,
    pub year: u8,
    pub weekday: DayOfWeek,
}

impl core::ops::Sub for Date {
    type Output = i16;

    /// Number of days between two dates.
    fn sub(self, rhs: Self) -> Self::Output {
        let lhs = self.fixed_from_gregorian() as i16;
        let rhs = rhs.fixed_from_gregorian() as i16;
        lhs - rhs
    }
}

impl Date {
    /// Number of days from an arbitrary, fixed day,
    /// suitable to computations of differences.
    fn fixed_from_gregorian(&self) -> u16 {
        let year_minus_one = self.year - 1;
        let mut out = 365 * (year_minus_one as u16)
            + (self.year / 4) as u16
            + (367 * self.month as u16 - 362) / 12
            + self.day as u16;

        if self.month > 2 {
            if self.year % 4 == 0 {
                out -= 1
            } else {
                out -= 2
            }
        }
        out
    }
}

/// Time structure, optional second.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
    pub second: Option<u8>,
}

impl core::ops::Sub for Time {
    type Output = i16;

    /// Number of minutes between two times,
    fn sub(self, rhs: Self) -> Self::Output {
        let lhs = 60 * self.hour as i16 + self.minute as i16;
        let rhs = 60 * rhs.hour as i16 + rhs.minute as i16;
        lhs - rhs
    }
}

impl core::ops::Sub for Datetime {
    type Output = Option<i16>;

    /// Number of minutes between two datetimes.
    fn sub(self, rhs: Self) -> Self::Output {
        let days_diff_in_minutes = (self.date - rhs.date).checked_mul(24 * 60)?;
        let time_diff = self.time - rhs.time;
        Some(days_diff_in_minutes + time_diff)
    }
}

/// Phase of day, refining the notion of an alarm
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PhaseOfDay {
    /// Default phase
    Default {
        /// Day of month when the last transition occured,
        /// used to avoid triggering multiples changes of phase
        /// on the same day.
        day_last_set: Option<u8>,
    },
    /// Dawn: the LED strip is to simulate the raising light
    Dawn {
        /// Number of minutes since the beginning of the dawn.
        elapsed_since_dawn: u8,
    },
    /// Sunrise: the alarm is to be triggered during this phase,
    /// until it is acked and comes back to default.
    SunRise {
        /// True when the luminosity of the environment was high
        /// during the phase transition.
        luminosity_at_sunrise: bool,
    },
}

impl ufmt::uDisplay for Date {
    /// Format a date to display on the serial port,
    /// for instance 2023-12-07
    fn fmt<W>(&self, f: &mut ufmt::Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: ufmt::uWrite + ?Sized,
    {
        if self.year < 10 {
            f.write_str("200")?;
        } else {
            f.write_str("20")?;
        }
        self.year.fmt(f)?;

        if self.month < 10 {
            f.write_str("-0")?;
        } else {
            f.write_str("-")?;
        }
        self.month.fmt(f)?;

        if self.day < 10 {
            f.write_str("-0")?;
        } else {
            f.write_str("-")?;
        }
        self.day.fmt(f)?;

        Ok(())
    }
}

impl ufmt::uDisplay for Time {
    /// Format a time to display on the serial port.
    /// for instance 21:34
    fn fmt<W>(&self, f: &mut ufmt::Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: ufmt::uWrite + ?Sized,
    {
        if self.hour < 10 {
            f.write_str("0")?;
        }
        self.hour.fmt(f)?;

        if self.minute < 10 {
            f.write_str(":0")?;
        } else {
            f.write_str(":")?;
        }
        self.minute.fmt(f)?;

        if let Some(second) = self.second {
            if second < 10 {
                f.write_str(":0")?;
            } else {
                f.write_str(":")?;
            }
            second.fmt(f)?;
        }

        Ok(())
    }
}

impl ufmt::uDisplay for Datetime {
    /// Format a datetime to display on the serial port.
    /// for instance 2023-12-07T21:34
    fn fmt<W>(&self, f: &mut ufmt::Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: ufmt::uWrite + ?Sized,
    {
        self.date.fmt(f)?;
        f.write_str("T")?;
        self.time.fmt(f)?;

        Ok(())
    }
}
