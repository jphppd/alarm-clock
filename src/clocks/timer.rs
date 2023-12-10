//! Timer based on the internal clock, moderately accurate.
use crate::{Dcf77Input, DCF77_LOGICAL_LEVEL_HIGH};
use arduino_hal::port::{
    mode::{Input, Io, PullUp},
    Pin, PinOps,
};
use core::cell::RefCell;

/// Prescaler of the internal timer (see the doc of the microprocessor)
const PRESCALER: u16 = 64;
/// Tick counts before raising an interrupt
const TIMER_COUNTS: u8 = 250;
/// Number of milliseconds to increment the counter by at each interrupt.
/// Possible Values:
///
/// ╔═══════════╦══════════════╦═══════════════════╗
/// ║ PRESCALER ║ TIMER_COUNTS ║ Overflow Interval ║
/// ╠═══════════╬══════════════╬═══════════════════╣
/// ║        64 ║          250 ║              1 ms ║
/// ║       256 ║          125 ║              2 ms ║
/// ║       256 ║          250 ║              4 ms ║
/// ║      1024 ║          125 ║              8 ms ║
/// ║      1024 ║          250 ║             16 ms ║
/// ╚═══════════╩══════════════╩═══════════════════╝
///
pub const MILLIS_INCREMENT: u16 = (((PRESCALER as u32) * (TIMER_COUNTS as u32)) / 16000u32) as u16;
/// Downsampling factor for the polling of the DCF77 input
pub const POLLED_SAMPLES_PERIOD_MS: u16 = 25;
/// The downsampling factor is based on the modulo of the timer.
/// The rolling of the timer must occur at the biggest multiple of the POLLED_SAMPLES_PERIOD_MS
/// without overflowing the capacity of the integer type.
const TIMER_MAX: u16 = (u16::MAX / POLLED_SAMPLES_PERIOD_MS) * POLLED_SAMPLES_PERIOD_MS;

/// Global timer object
static TIMER: avr_device::interrupt::Mutex<RefCell<Option<TimerPolling<Dcf77Input>>>> =
    avr_device::interrupt::Mutex::new(RefCell::new(None));

/// Timer structure, new-type pattern
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timer(pub u16);

impl ufmt::uDisplay for Timer {
    /// Formatter for the serial output.
    fn fmt<W>(&self, f: &mut ufmt::Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: ufmt::uWrite + ?Sized,
    {
        self.0.fmt(f)
    }
}

impl Timer {
    /// Increment the timer, wrapping at TIMER_MAX.
    fn increment(&mut self) {
        self.0 += MILLIS_INCREMENT;
        if self.0 > TIMER_MAX {
            self.0 = 1;
        }
    }
}

impl core::ops::Sub for Timer {
    type Output = Timer;

    /// Difference of timer, taking into account wrapping.
    /// The result is meaningful only if the time between both arguments
    /// are less than TIMER_MAX apart.
    fn sub(self, rhs: Self) -> Self::Output {
        Timer(if self.0 >= rhs.0 {
            self.0 - rhs.0
        } else {
            let rhs = TIMER_MAX - rhs.0;
            self.0 + rhs
        })
    }
}

impl core::ops::Rem<u16> for Timer {
    type Output = u16;

    /// Modulo operation on the timer.
    fn rem(self, rhs: u16) -> Self::Output {
        self.0 % rhs
    }
}

/// Timer structure holding polled values of the DCF77 input
struct TimerPolling<PIN> {
    /// Timer per se
    timer: Timer,
    /// DCF77 pin input
    pin: Pin<Input<PullUp>, PIN>,
    /// Latest polled value, set during the interrupt, with the timer value of its setting
    polled_value: Option<(Timer, bool)>,
    /// Counter for the downsampling of DCF77,
    /// incremented for high (logical) levels, decremented for low (logical) levels
    downsampling_counter_dcf77: i8,
}

impl<PIN: PinOps> TimerPolling<PIN> {
    /// Create new structure.
    fn new(pin: Pin<Input<PullUp>, PIN>) -> Self {
        Self {
            timer: Timer::default(),
            pin,
            polled_value: None,
            downsampling_counter_dcf77: 0,
        }
    }
}

/// Timer interrupt function. Borrow the global timer, increment the counter,
/// poll DFC77 input and optionaly publish (setting Some) a downsampled value
/// of DCF77.
#[avr_device::interrupt(atmega328p)]
fn TIMER0_COMPA() {
    avr_device::interrupt::free(|cs| {
        if let Some(timer) = TIMER.borrow(cs).borrow_mut().as_mut() {
            timer.timer.increment();

            if timer.pin.is_high() == DCF77_LOGICAL_LEVEL_HIGH {
                timer.downsampling_counter_dcf77 += 1;
            } else {
                timer.downsampling_counter_dcf77 -= 1;
            }

            if timer.timer % POLLED_SAMPLES_PERIOD_MS == 0 {
                timer.polled_value = Some((timer.timer, timer.downsampling_counter_dcf77 > 0));
                timer.downsampling_counter_dcf77 = 0;
            }
        }
    })
}

/// Initialize the registrers for the hardware timer.
pub fn init<MODE: Io>(tc0: arduino_hal::pac::TC0, pin: Pin<MODE, Dcf77Input>) {
    // Configure the timer for the above interval (in CTC mode)
    // and enable its interrupt.
    tc0.tccr0a.write(|w| w.wgm0().ctc());
    tc0.ocr0a.write(|w| w.bits(TIMER_COUNTS));
    tc0.tccr0b.write(|w| match PRESCALER {
        8 => w.cs0().prescale_8(),
        64 => w.cs0().prescale_64(),
        256 => w.cs0().prescale_256(),
        1024 => w.cs0().prescale_1024(),
        _ => w.cs0().direct(),
    });
    tc0.timsk0.write(|w| w.ocie0a().set_bit());

    // Reset the global millisecond counter
    avr_device::interrupt::free(|cs| {
        let mut timer = TIMER.borrow(cs).borrow_mut();
        *timer = Some(TimerPolling::new(pin.into_pull_up_input()));
    });
}

/// Get the current value of the timer.
pub fn get_timer() -> Option<Timer> {
    avr_device::interrupt::free(|cs| TIMER.borrow(cs).borrow().as_ref().map(|timer| timer.timer))
}

/// Get the polled value of DCF77, if any, with the time when it was set
/// to discrimate between an old value already processed and a new one.
pub fn get_polled_values() -> Option<(Timer, bool)> {
    avr_device::interrupt::free(|cs| {
        TIMER
            .borrow(cs)
            .borrow()
            .as_ref()
            .and_then(|timer| timer.polled_value)
    })
}
