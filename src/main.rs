//! Alarm clock program for ATMEGA328P microprocessor
// Compiler commands appropriate for bare-metal development
#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]

/*
References:
ATMEGA238p: https://www.e-lab.de/downloads/DOCs/mega328P.pdf
Arduino: https://content.arduino.cc/assets/A000066-full-pinout.pdf
Arduino: https://content.arduino.cc/assets/UNO-TH_Rev3e_sch.pdf
Arduino: https://docs.arduino.cc/resources/datasheets/A000066-datasheet.pdf

          +---O---+
      PC6 |1    28| PC5  I2C.SCL - d19
  RXD PD0 |2    27| PC4  I2C.SDA - d18
  TXD PD1 |3    26| PC3
      PD2 |4    25| PC2
      PD3 |5    24| PC1
      PD4 |6    23| PC0
      VCC |7    22| GND
      GND |8    21| AREF
      PB6 |9    20| AVCC
      PB7 |10   19| PB5  SPI.CLK
      PD5 |11   18| PB4
      PD6 |12   17| PB3  SPI.MOSI
      PD7 |13   16| PB2  SPI.CS - d10
      PB0 |14   15| PB1
          +-------+
*/

// Pinout of the peripherals, either as ATMETA32P port or arduino labels
type Dcf77Input = arduino_hal::hal::port::PD2; // d2
type LuminosityInput = arduino_hal::hal::port::PD3; // d3
type ButtonInput = arduino_hal::hal::port::PD4; // d4
type ProximityInput = arduino_hal::hal::port::PD6; // d6
type BuzzerOutput = arduino_hal::hal::port::PD7; // d7
type LedStripDataOutput = arduino_hal::hal::port::PB0; // d8
type LedStripRelayOutput = arduino_hal::hal::port::PB1; // d9
type DisplaySpiCsOutput = arduino_hal::hal::port::PB2; // d10
type DisplaySpiMosiOutput = arduino_hal::hal::port::PB3; // d11
type DisplaySpiClkOutput = arduino_hal::hal::port::PB5; // d13

/// Mapping between the electric levels (+3.3V or +5V) and the logical level of the DCF77 receiver
const DCF77_LOGICAL_LEVEL_HIGH: bool = false;
/// Mapping between the electric levels (+3.3V or +5V) and the logical level of the luminosity sensor
const LUMINOSITY_LOGICAL_LEVEL_HIGH: bool = false;
/// Mapping between the electric levels (+3.3V or +5V) and the logical level of the proximity sensor
const PROXIMITY_LOGICAL_LEVEL_HIGH: bool = false;
/// Mapping between the electric levels (+3.3V or +5V) and the logical level of the button
const BUTTON_LOGICAL_LEVEL_HIGH: bool = false;
/// Mapping between the electric levels (+3.3V or +5V) and the logical level of the buzzer
const BUZZER_LOGICAL_LEVEL_HIGH: bool = true;
/// Number of individual leds on the strip
const LED_STRIP_COUNT: usize = 180;
/// Value of the brightness of the simulated day
const LED_STRIP_MAX_INTENSITY: u8 = 0x55;

/// Duration of the dawn before the sunrise
const ALARM_DAWN_DURATION_MINUTES: u8 = 20;
/// Time of the sunrise (alarm) during the week
const ALARM_WEEK_SUNRISE: Time = Time {
    hour: 6,
    minute: 0,
    second: None,
};
/// Time of the sunrise (alarm) during the week-end
const ALARM_WEEKEND_SUNRISE: Time = Time {
    hour: 8,
    minute: 10,
    second: None,
};
/// Ack the alarm (if not already done manually) after that time
const ALARM_AUTO_ACK_MIN: u8 = 5;

use crate::{
    clocks::{Clock, PhaseOfDay, Time},
    inputs::Inputs,
    outputs::{Color, DisplayIntensity, Outputs},
    serial_commands::{Command, SerialBuffer, SunriseSelection, USART_MUTEX},
};
use arduino_hal::hal::wdt;
use core::{
    panic::PanicInfo,
    sync::atomic::{self, Ordering},
};
use embedded_hal::blocking::i2c;

mod clocks;
mod inputs;
mod outputs;
mod serial_commands;

/// The main state of the whole program, updated at every loop,
/// holding the memory.
struct MainState<I2C, const WRITE_BUFFER_SIZE: usize, const READ_BUFFER_SIZE: usize>
where
    I2C: i2c::Write + i2c::WriteRead,
{
    /// Clocks and alarms
    clocks: Clock<I2C>,
    /// Inputs, either from the environment (light, etc.) or
    /// from the user (proximity, etc.)
    inputs: Inputs,
    /// Outputs for the user (display, buzzer, etc.)
    outputs: Outputs,
    /// Serial I/O
    serial_buffer: SerialBuffer<WRITE_BUFFER_SIZE, READ_BUFFER_SIZE>,
    /// If set to Some (by a command), the LED strip will display this color,
    /// overwriting the nominal one
    forced_led_color: Option<Color>,
}

impl<const WRITE_BUFFER_SIZE: usize, const READ_BUFFER_SIZE: usize>
    MainState<arduino_hal::I2c, WRITE_BUFFER_SIZE, READ_BUFFER_SIZE>
{
    /// Run all the tasks needed to update the state/inputs/outputs.
    fn run(&mut self) {
        self.update_inputs();
        self.process();
        self.update_outputs();
    }

    /// Update all the inputs (clock, env, user), meant to be called before processing.
    fn update_inputs(&mut self) {
        self.inputs.update();
        self.clocks.update();
        self.serial_buffer.load();
    }

    /// Update all the outputs for the user.
    fn update_outputs(&mut self) {
        self.serial_buffer.flush();
        self.outputs.render();
    }

    /// Process every module
    fn process(&mut self) {
        self.process_alarm();
        self.process_display();
        self.process_led_strip();
        self.process_buzzer();
        // Serial commands might overwrite previous elements:
        // call it at the end
        self.process_command();
    }

    /// Ack the alarm, if the conditions are met
    fn process_alarm(&mut self) {
        if let PhaseOfDay::SunRise {
            elapsed_since_sunrise,
        } = self.clocks.phase_of_day
        {
            // The proximity sensor always acks the alarm
            if self.inputs.proximity.value {
                self.clocks.ack_sunrise();
            }
            // Ack automatically after a certain duration
            if elapsed_since_sunrise > ALARM_AUTO_ACK_MIN {
                self.clocks.ack_sunrise();
            }
        }
    }

    /// Process the LED display output, value and intensity
    fn process_display(&mut self) {
        let second = self
            .clocks
            .datetime
            .and_then(|d| d.time.second)
            .unwrap_or(0xc0);
        let quarters_since_last_rtc_update = self.clocks.quarters_since_last_rtc_update();

        self.outputs.display.write_time(self.clocks.datetime);
        self.outputs.display.set_at(
            29,
            &[second, 0, quarters_since_last_rtc_update.unwrap_or(u8::MAX)],
        );

        // Compute the intensity of the display,
        // according to the various environmental conditions.
        let display_intensity = if self.inputs.luminosity.value
            || self.inputs.button.value
            || self.inputs.proximity.value
            || matches!(
                self.clocks.phase_of_day,
                PhaseOfDay::SunRise {
                    elapsed_since_sunrise: _,
                }
            ) {
            if self.inputs.luminosity.value {
                DisplayIntensity::Bright
            } else {
                DisplayIntensity::Dim
            }
        } else {
            DisplayIntensity::Off
        };
        self.outputs.display.set_intensity(display_intensity);
    }

    /// Switch on/off the led strip
    fn process_led_strip(&mut self) {
        if let Some(forced_led_color) = self.forced_led_color {
            self.outputs.led_strip.set_color(Some(forced_led_color));
            return;
        }
        let color = match self.clocks.phase_of_day {
            // Between dawn and sunrise: ramp of intensity
            PhaseOfDay::Dawn {
                elapsed_since_dawn: elapsed,
            } => self.clocks.dawn_duration.map(|dawn_duration| {
                Color::sun(
                    ((elapsed as u16).saturating_mul(LED_STRIP_MAX_INTENSITY as u16)
                        / dawn_duration as u16) as u8,
                )
            }),
            // Sunrise: be bright!
            PhaseOfDay::SunRise {
                elapsed_since_sunrise: _,
            } => Some(Color::sun(LED_STRIP_MAX_INTENSITY)),
            // Otherwise: switch off
            PhaseOfDay::Default { day_last_set: _ } => None,
        };
        self.outputs.led_strip.set_color(color);
    }

    /// Decide to buzz... or not
    fn process_buzzer(&mut self) {
        if matches!(
            self.clocks.phase_of_day,
            PhaseOfDay::SunRise {
                elapsed_since_sunrise: _
            }
        ) {
            if !self.outputs.buzzer.is_active() {
                self.outputs.buzzer.start();
            }
        } else if self.outputs.buzzer.is_active() {
            self.outputs.buzzer.stop();
        }
    }

    /// Process commands received on serial input, if any
    fn process_command(&mut self) {
        loop {
            match self.serial_buffer.dequeue_command() {
                Ok(None) => {
                    break;
                }
                Ok(Some(Command::QueryDatetime)) => match self.clocks.datetime {
                    Some(datetime) => {
                        ufmt::uwriteln!(&mut self.serial_buffer, "{}", datetime).ok();
                    }
                    None => {
                        ufmt::uwriteln!(&mut self.serial_buffer, "None").ok();
                    }
                },
                Ok(Some(Command::QueryLastDcf77Update)) => match self.clocks.last_dcf77_update {
                    Some(last_dcf77_update) => {
                        ufmt::uwriteln!(&mut self.serial_buffer, "{}", last_dcf77_update).ok();
                    }
                    None => {
                        ufmt::uwriteln!(&mut self.serial_buffer, "None").ok();
                    }
                },
                Ok(Some(Command::QueryPhase)) => match self.clocks.phase_of_day {
                    PhaseOfDay::Default { day_last_set } => {
                        ufmt::uwrite!(&mut self.serial_buffer, "Default day last set ",).ok();
                        match day_last_set {
                            Some(day_last_set) => {
                                ufmt::uwriteln!(&mut self.serial_buffer, "{}", day_last_set).ok()
                            }
                            None => ufmt::uwriteln!(&mut self.serial_buffer, "None").ok(),
                        };
                    }
                    PhaseOfDay::Dawn { elapsed_since_dawn } => {
                        ufmt::uwriteln!(
                            &mut self.serial_buffer,
                            "Dawn since {} min",
                            elapsed_since_dawn
                        )
                        .ok();
                    }
                    PhaseOfDay::SunRise {
                        elapsed_since_sunrise,
                    } => {
                        ufmt::uwriteln!(
                            &mut self.serial_buffer,
                            "SunRise since {}",
                            elapsed_since_sunrise,
                        )
                        .ok();
                    }
                },
                Ok(Some(Command::QueryDawnDuration)) => match self.clocks.dawn_duration {
                    Some(dawn_duration) => {
                        ufmt::uwriteln!(&mut self.serial_buffer, "{}", dawn_duration).ok();
                    }
                    None => {
                        ufmt::uwriteln!(&mut self.serial_buffer, "None").ok();
                    }
                },
                Ok(Some(Command::Query(time_sel))) => {
                    let v = match time_sel {
                        SunriseSelection::Week => self.clocks.week_sunrise.as_ref(),
                        SunriseSelection::WeekEnd => self.clocks.weekend_sunrise.as_ref(),
                    };
                    match v {
                        Some(time) => {
                            ufmt::uwriteln!(&mut self.serial_buffer, "{}", time).ok();
                        }
                        None => {
                            ufmt::uwriteln!(&mut self.serial_buffer, "None").ok();
                        }
                    }
                }
                Ok(Some(Command::SetDawn(minutes))) => {
                    self.clocks.dawn_duration = Some(minutes);
                    ufmt::uwriteln!(&mut self.serial_buffer, "Ack").ok();
                }
                Ok(Some(Command::Set(time_sel, time))) => {
                    match time_sel {
                        SunriseSelection::Week => self.clocks.week_sunrise = Some(time),
                        SunriseSelection::WeekEnd => self.clocks.weekend_sunrise = Some(time),
                    };
                    self.clocks.phase_of_day = PhaseOfDay::Default { day_last_set: None };
                    ufmt::uwriteln!(&mut self.serial_buffer, "Ack").ok();
                }
                Ok(Some(Command::SetLedColor(color))) => {
                    self.forced_led_color = Some(color);
                    ufmt::uwriteln!(&mut self.serial_buffer, "Ack").ok();
                }
                Ok(Some(Command::ResetLedColor)) => {
                    self.forced_led_color = None;
                    ufmt::uwriteln!(&mut self.serial_buffer, "Ack").ok();
                }
                Ok(Some(Command::AckAlarm)) => {
                    self.clocks.ack_sunrise();
                    ufmt::uwriteln!(&mut self.serial_buffer, "Ack").ok();
                }
                Err(()) => {
                    ufmt::uwriteln!(&mut self.serial_buffer, "Bad command").ok();
                }
            }
        }
    }
}

/// Entry point: initialization of the devices and endless loop
#[arduino_hal::entry]
fn main() -> ! {
    // Acquire hardware objects
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let i2c = arduino_hal::I2c::new(
        dp.TWI,
        pins.a4.into_pull_up_input(),
        pins.a5.into_pull_up_input(),
        50000,
    );
    // Create main memory structure
    let mut main = MainState::<
        _,
        { serial_commands::SERIAL_WRITE_BUFFER_SIZE },
        { serial_commands::SERIAL_READ_BUFFER_SIZE },
    > {
        clocks: clocks::Clock::init(dp.TC0, pins.d2, i2c),
        inputs: inputs::Inputs::init(
            pins.d4,
            BUTTON_LOGICAL_LEVEL_HIGH,
            pins.d3,
            LUMINOSITY_LOGICAL_LEVEL_HIGH,
            pins.d6,
            PROXIMITY_LOGICAL_LEVEL_HIGH,
        ),
        outputs: outputs::Outputs::init(pins.d11, pins.d10, pins.d13, pins.d8, pins.d9, pins.d7),
        serial_buffer: Default::default(),
        forced_led_color: None,
    };

    {
        // Setup the USART for serial in/out
        let mut serial_read_buffer = arduino_hal::default_serial!(dp, pins, 115200);
        serial_read_buffer.listen(arduino_hal::hal::usart::Event::RxComplete);
        avr_device::interrupt::free(|cs| {
            *USART_MUTEX.borrow(cs).borrow_mut() = Some(serial_read_buffer);
        });
    }

    // Setup the hardware watchdog, in case soemthing goes wrong.
    let mut watchdog = wdt::Wdt::new(dp.WDT, &dp.CPU.mcusr);
    watchdog.start(wdt::Timeout::Ms1000).unwrap();
    unsafe { avr_device::interrupt::enable() };

    // Display an init message
    ufmt::uwriteln!(main.serial_buffer, "Start").ok();
    main.update_outputs();

    loop {
        main.run();
        watchdog.feed();
    }
}

/// Panic handler: do nothing
#[inline(never)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        atomic::compiler_fence(Ordering::SeqCst);
    }
}
