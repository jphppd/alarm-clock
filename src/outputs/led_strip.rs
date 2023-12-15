//! Control the LED strip.
//! See the [interface](https://www.led-stuebchen.de/download/WS2815.pdf)
use crate::{LedStripDataOutput, LedStripRelayOutput};
use arduino_hal::{
    delay_ms,
    port::{
        mode::{Io, Output},
        Pin, PinMode,
    },
};
use avr_device::asm::nop;
use core::iter;

const RELAY_ACTIVATION_TIME_MS: u16 = 5;

/// Main structure for a LED strip with N LEDS
pub struct LedStrip<const N: usize> {
    /// Pin connected to the digital input of the strip
    data_out: Pin<Output, LedStripDataOutput>,
    /// Pin connected to the (power supply) relay
    relay_out: Pin<Output, LedStripRelayOutput>,
    /// Current (actual) color. When None, the relay will be switched low.
    current: Option<Color>,
    /// Color to be set during the next rendering
    buffer: Option<Color>,
}

impl<const N: usize> LedStrip<N> {
    /// Initialize the structure.
    pub fn init<ModeData: PinMode + Io, ModeRelay: PinMode + Io>(
        data_out: Pin<ModeData, LedStripDataOutput>,
        relay_out: Pin<ModeRelay, LedStripRelayOutput>,
    ) -> Self {
        let mut data_out = data_out.into_output();
        data_out.set_low();
        let mut relay_out = relay_out.into_output();
        relay_out.set_low();
        LedStrip {
            data_out,
            relay_out,
            current: Default::default(),
            buffer: Default::default(),
        }
    }

    /// Set a specific color to the LED strip
    pub fn set_color(&mut self, color: Option<Color>) {
        self.buffer = color;
    }

    /// Actually render (set) the buffered color on the strip
    /// if different from the actual one.
    pub fn render(&mut self) {
        if self.buffer == self.current {
            return;
        }
        self.current = self.buffer;

        match self.current {
            Some(color) => {
                if self.relay_out.is_set_low() {
                    self.relay_out.set_high();
                    delay_ms(RELAY_ACTIVATION_TIME_MS);
                }
                self.write_color(color);
            }
            None => self.relay_out.set_low(),
        }
    }

    /// Write the same color to all LEDS
    fn write_color(&mut self, color: Color) {
        let color_raw = color.to_bits();
        avr_device::interrupt::free(|_| {
            // Write twice as many LEDs as the real number, to reduce the risk
            // of having a failed LED because of timings (jitters, ...).
            for bit in iter::repeat(color_raw).take(2 * N).flatten() {
                self.write_bit(bit);
            }
        })
    }

    /// Write a single bit from the color.
    /// A bit true or false is decided by the duty cycle of the electrical
    /// raising/falling edges, hence the calls to the delays.
    fn write_bit(&mut self, bit: bool) {
        match bit {
            true => {
                self.data_out.set_high();
                Self::delay();
                self.data_out.set_low();
            }
            false => {
                self.data_out.set_high();
                self.data_out.set_low();
                Self::delay();
            }
        }
    }

    /// Generate a delay matching the specifications for a bit
    /// of the LED strip.
    fn delay() {
        nop();
        nop();
        nop();
        nop();
        nop();
        nop();
        nop();
        nop();
        nop();
        nop();
        nop();
        nop();
    }
}

/// RGB color to display for one single LED.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Color {
    pub green: u8,
    pub red: u8,
    pub blue: u8,
}

impl Color {
    /// Get a yellowish color  with the given intensity to simulate the sun
    pub fn sun(intensity: u8) -> Self {
        Self {
            green: intensity,
            red: intensity,
            blue: intensity / 4,
        }
    }

    /// Translate a color to an array of bool (bits)
    /// suitable to be transmitted to the LED strip.
    fn to_bits(self) -> [bool; 24] {
        let mut out = [false; 24];
        for bit_rank in 0..8 {
            let bit: u8 = 1 << (7 - bit_rank);
            out[bit_rank] = (self.red & bit) == bit;
            out[8 + bit_rank] = (self.green & bit) == bit;
            out[16 + bit_rank] = (self.blue & bit) == bit;
        }
        out
    }
}
