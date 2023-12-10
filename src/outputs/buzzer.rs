//! Control the buzzer.
use crate::{
    clocks::timer::{get_timer, Timer},
    BuzzerOutput, BUZZER_LOGICAL_LEVEL_HIGH,
};
use arduino_hal::port::{
    mode::{Io, Output},
    Pin, PinMode,
};

/// Buzzer structure. We don't want a continuous buzzer,
/// rather a beep-beep with a defined pawm duty-cycle.
pub struct Buzzer {
    /// Pin connected to the digital input of the buzzer
    data_out: Pin<Output, BuzzerOutput>,
    /// PWM object. If Some, it means the buzzer is active,
    /// if None it means it's not active.
    pwm: Option<Pwm>,
}

impl Buzzer {
    /// Initialize the object.
    pub fn init<MODE: PinMode + Io>(data_out: Pin<MODE, BuzzerOutput>) -> Self {
        let mut out = Self {
            data_out: data_out.into_output(),
            pwm: None,
        };
        out.do_mute();
        out
    }

    /// Get the status of the buzzer.
    pub fn is_active(&self) -> bool {
        self.pwm.is_some()
    }

    /// Start the buzzer.
    pub fn start(&mut self) {
        self.pwm = Some(Pwm::default());
    }

    /// Stop the buzzer.
    pub fn stop(&mut self) {
        self.pwm = None;
    }

    /// Update the object,
    pub fn update(&mut self) {
        match self.pwm {
            Some(ref mut pwm) => match pwm.update() {
                true => self.do_buzz(),
                false => self.do_mute(),
            },
            None => self.do_mute(),
        }
    }

    /// Set the pin to the appropriate electrical level
    /// to actually generate a sound.
    fn do_buzz(&mut self) {
        if BUZZER_LOGICAL_LEVEL_HIGH {
            self.data_out.set_high();
        } else {
            self.data_out.set_low();
        }
    }

    /// Set the pin to the appropriate electrical level
    /// to mute the buzzer.
    fn do_mute(&mut self) {
        if BUZZER_LOGICAL_LEVEL_HIGH {
            self.data_out.set_low();
        } else {
            self.data_out.set_high();
        }
    }
}

/// PWM object
struct Pwm {
    /// Duty cycle over 1 second, in 10ms
    duty_over_second_centisecond: u8,
    /// Start of the cycle
    cycle_start: Option<Timer>,
}

impl Pwm {
    /// Create the object, begin with a duty cycle of 20ms - 980ms
    fn default() -> Self {
        Self {
            duty_over_second_centisecond: 2,
            cycle_start: get_timer(),
        }
    }

    /// Update the PWM, according to the current global timer and the cycle start,
    /// returning the bool value matching the instant in the cycle.
    pub fn update(&mut self) -> bool {
        let now = get_timer();
        if let (Some(now), Some(start)) = (now, self.cycle_start) {
            let time_diff_centisecond = ((now - start).0 / 10) as u8;

            if time_diff_centisecond <= self.duty_over_second_centisecond {
                return true;
            }
            if time_diff_centisecond < 100 {
                return false;
            }
        }

        self.cycle_start = now;
        true
    }
}
