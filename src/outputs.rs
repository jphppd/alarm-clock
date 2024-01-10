//! Outputs of the clock setup, for humans
use crate::{
    BuzzerOutput, DisplaySpiClkOutput, DisplaySpiCsOutput, DisplaySpiMosiOutput,
    LedStripDataOutput, LED_STRIP_COUNT,
};
use arduino_hal::port::{mode::Io, Pin};
pub use display::DisplayIntensity;
pub use led_strip::Color;

mod buzzer;
mod display;
mod led_strip;

pub struct Outputs {
    /// Matrix-leds display, to print the time and some information
    pub display: display::Display,
    /// Led strip to simulate dawn
    pub led_strip: led_strip::LedStrip<LED_STRIP_COUNT>,
    /// Buzzer alarm
    pub buzzer: buzzer::Buzzer,
}

impl Outputs {
    /// Initialize the structure, in particular with the pinout.
    /// The matrix display uses a SPI interface, the other outputs
    /// use a simple digital pin.
    pub fn init(
        data: Pin<impl Io, DisplaySpiMosiOutput>,
        cs: Pin<impl Io, DisplaySpiCsOutput>,
        clk: Pin<impl Io, DisplaySpiClkOutput>,
        led_strip_data: Pin<impl Io, LedStripDataOutput>,
        buzzer: Pin<impl Io, BuzzerOutput>,
    ) -> Self {
        Self {
            display: display::Display::init(data, cs, clk),
            led_strip: led_strip::LedStrip::init(led_strip_data),
            buzzer: buzzer::Buzzer::init(buzzer),
        }
    }

    /// Render the output.
    /// If the actual state is already the expected one, these functions do
    /// nothing (to spare computation time).
    /// These functions exist to avoid the cases when several updates of the
    /// same output occur during one loop of the main structure: the update
    /// might be a bit costly. The rendering is postponed until this function
    /// is called.
    pub fn render(&mut self) {
        self.display.render();
        self.led_strip.render();
        self.buzzer.update();
    }
}
