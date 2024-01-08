//! Clocks, date and time management
use self::{
    dcf77::{Dcf77, Dcf77SignalVariant},
    rtc::Rtc,
};
use crate::{Dcf77Input, ALARM_DAWN_DURATION_MINUTES, ALARM_WEEKEND_SUNRISE, ALARM_WEEK_SUNRISE};
use arduino_hal::port::{mode::Io, Pin};
pub use datetime::{Date, Datetime, DayOfWeek, PhaseOfDay, Time};
use embedded_hal::blocking::i2c;

pub mod datetime;
pub mod dcf77;
pub mod rtc;
pub mod timer;

/// Main structure holding the current datetime
/// and the interfaces to the DCF77 receiver and the RTC.
pub struct Clock<I2C>
where
    I2C: i2c::Write + i2c::WriteRead,
{
    /// Current datetime
    pub datetime: Option<Datetime>,
    /// Time of the last DCF77 datetime update. In optimal
    /// conditions, an update is sent every minute.
    pub last_dcf77_update: Option<Datetime>,
    /// Is "some" when a DCF77 bit was received during this loop
    pub last_dcf77_bit: Option<Dcf77SignalVariant>,
    /// Phase of the day, used to determine if the alarm
    /// should be raised or not.
    pub phase_of_day: PhaseOfDay,
    /// Optional duration of the dawn, between sunrise
    pub dawn_duration: Option<u8>,
    /// Optional time of the sunrise, during the week
    pub week_sunrise: Option<Time>,
    /// Optional time of the sunrise, during the week-end
    pub weekend_sunrise: Option<Time>,
    /// Interface with the DFC77 receiver
    dcf77: Dcf77,
    /// Interface with the RTC
    rtc: Rtc<I2C>,
}

impl<I2C, CommE> Clock<I2C>
where
    I2C: i2c::Write<Error = CommE> + i2c::WriteRead<Error = CommE>,
{
    /// Initialize the structure with default settings.
    pub fn init<MODE: Io>(
        tc0: arduino_hal::pac::TC0,
        pin: Pin<MODE, Dcf77Input>,
        i2c: I2C,
    ) -> Self {
        timer::init(tc0, pin);
        Self {
            datetime: None,
            last_dcf77_update: Default::default(),
            last_dcf77_bit: None,
            phase_of_day: PhaseOfDay::Default { day_last_set: None },
            dawn_duration: Some(ALARM_DAWN_DURATION_MINUTES),
            week_sunrise: Some(ALARM_WEEK_SUNRISE),
            weekend_sunrise: Some(ALARM_WEEKEND_SUNRISE),
            dcf77: Default::default(),
            rtc: Rtc::init(i2c),
        }
    }

    /// Public interface to process all duties at each call.
    pub fn update(&mut self) {
        let dcf77 = self.process_dcf77();
        self.process_rtc(dcf77);

        if let Some(datetime) = self.datetime {
            self.update_phase_of_day(datetime)
        }
    }

    /// Ack sunrise (the alarm), going back to the default phase of the day.
    pub fn ack_sunrise(&mut self) {
        self.phase_of_day = PhaseOfDay::Default {
            day_last_set: self.datetime.map(|dt| dt.date.day),
        }
    }

    /// Run dcf77 decoder, waiting for a new update.
    fn process_dcf77(&mut self) -> Option<Datetime> {
        match self.dcf77.run() {
            Ok((bit, None)) => {
                self.last_dcf77_bit = bit;
                // No update
                None
            }
            Ok((bit, Some(dcf77_datetime))) => {
                self.last_dcf77_bit = bit;
                // New datetime from dcf77
                Some(dcf77_datetime)
            }
            Err(_) => {
                // Reset the internal state of the dcf77 decoder
                self.dcf77 = Default::default();
                None
            }
        }
    }

    /// Set the rtc with a dcf77 update if given,
    /// and in any case read the updated value.
    fn process_rtc(&mut self, dcf77: Option<Datetime>) {
        if let Some(dcf77) = dcf77 {
            self.last_dcf77_update = Some(dcf77);
            self.rtc.set_datetime(&dcf77).ok();
        }
        self.datetime = self.rtc.datetime().ok();
    }

    /// Determine if the phase of the day must be updated, that is,
    /// if the dawn or the sunrise has come (and the alarm process should
    /// be triggered).
    fn update_phase_of_day(&mut self, datetime: Datetime) {
        // Test if the alarm has already been triggered today or not,
        // to prevent multiple consecutive triggers despite an ack.
        let can_trigger_alarm_today = match self.phase_of_day {
            PhaseOfDay::Default { day_last_set } => day_last_set
                .map(|dls| dls != datetime.date.day)
                .unwrap_or(true),
            PhaseOfDay::Dawn {
                elapsed_since_dawn: _,
            } => true,
            PhaseOfDay::SunRise {
                elapsed_since_sunrise: _,
            } => true,
        };

        if can_trigger_alarm_today {
            let sunrise = match datetime.date.weekday.is_week_end() {
                true => self.weekend_sunrise,
                false => self.week_sunrise,
            };

            // If the sunrise is None, simply ignore the alarm
            if let Some(sunrise) = sunrise {
                // datetime.time is the current time.
                // Just compare it with the sunrise time
                let elapsed_since_sunrise = datetime.time - sunrise;
                if elapsed_since_sunrise >= 0 {
                    self.phase_of_day = PhaseOfDay::SunRise {
                        elapsed_since_sunrise: elapsed_since_sunrise as u8,
                    };
                } else if let Some(dawn_duration) = self.dawn_duration {
                    let elapsed_since_dawn = elapsed_since_sunrise + dawn_duration as i16;
                    if elapsed_since_dawn >= 0 {
                        self.phase_of_day = PhaseOfDay::Dawn {
                            elapsed_since_dawn: elapsed_since_dawn as u8,
                        }
                    }
                }
            }
        }
    }

    /// Compute the number of quarter jours since the last dcf77 update.
    pub fn quarters_since_last_rtc_update(&self) -> Option<u8> {
        if let (Some(datetime), Some(last_dcf77_update)) = (self.datetime, self.last_dcf77_update) {
            let diff = core::cmp::max((datetime - last_dcf77_update)?, 0);
            Some((diff / 15) as u8)
        } else {
            None
        }
    }
}
