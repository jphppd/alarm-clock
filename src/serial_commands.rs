//! Serial communication, with commands from the user
use crate::{clocks::Time, outputs::Color};
use arduino_hal::hal::usart::Usart0;
use core::cell::RefCell;
use embedded_hal::serial::Read;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};

/// Size, in bytes, of the buffer for serial input
pub const SERIAL_READ_BUFFER_SIZE: usize = 14;
/// Size, in bytes, of the buffer for serial output
pub const SERIAL_WRITE_BUFFER_SIZE: usize = 64;

/// Interface with the hardware USART
pub static USART_MUTEX: avr_device::interrupt::Mutex<
    RefCell<Option<Usart0<arduino_hal::DefaultClock>>>,
> = avr_device::interrupt::Mutex::new(RefCell::new(None));
/// Buffer containing the input bytes read on the USART (there is no hardware buffer)
static SERIAL_READ_BUFFER: avr_device::interrupt::Mutex<
    RefCell<ConstGenericRingBuffer<u8, SERIAL_READ_BUFFER_SIZE>>,
> = avr_device::interrupt::Mutex::new(RefCell::new(ConstGenericRingBuffer::new()));

/// Read one byte of the USART into local buffer, via interupt mechanism.
#[avr_device::interrupt(atmega328p)]
unsafe fn USART_RX() {
    avr_device::interrupt::free(|cs| {
        if let Some(ref mut usart) = USART_MUTEX.borrow(cs).borrow_mut().as_mut() {
            let mut serial_read_buffer = SERIAL_READ_BUFFER.borrow(cs).borrow_mut();
            while let Ok(byte) = usart.read() {
                if (0x20..0x7f).contains(&byte) || byte == 0x0a {
                    serial_read_buffer.push(byte);
                }
            }
        }
    });
}

/// Buffers for serial communication, instantiated within a structure/a function
/// rather than a global, mutex-protected variable.
#[derive(Default)]
pub struct SerialBuffer<const WRITE_BUFFER_SIZE: usize, const READ_BUFFER_SIZE: usize> {
    output: ConstGenericRingBuffer<u8, WRITE_BUFFER_SIZE>,
    input: ConstGenericRingBuffer<u8, READ_BUFFER_SIZE>,
}

/// Implement ufmt::uWrite for the serial buffer, to be able to call ufmt::uwriteln
impl<const WRITE_BUFFER_SIZE: usize, const READ_BUFFER_SIZE: usize> ufmt::uWrite
    for SerialBuffer<WRITE_BUFFER_SIZE, READ_BUFFER_SIZE>
{
    type Error = ();

    fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
        for byte in s.as_bytes() {
            self.output.push(*byte);
        }
        Ok(())
    }
}

/// Variants for commands: either week or week-end
#[derive(PartialEq, Eq)]
pub enum SunriseSelection {
    Week,
    WeekEnd,
}

/// Commands for the serial interfaces
#[derive(PartialEq, Eq)]
pub enum Command {
    /// Query current datetime: ?dt
    QueryDatetime,
    /// Query dawn duration: ?dw
    QueryDawnDuration,
    /// Query last DCF77 update: ?77
    QueryLastDcf77Update,
    /// Debug dcf77: !dbg77
    DebugDcf77,
    /// Query the current phase of the day: ?phase
    QueryPhase,
    /// Query the time of sunrise (alarm), week or week-end: ?w\[ke\]
    Query(SunriseSelection),
    /// Set dawn duration: !dawn MM
    SetDawn(u8),
    /// Set the time of sunrise (alarm), week or week-end: !w\[ke\] HH:MM
    Set(SunriseSelection, Time),
    /// Set the color of the led stripe: !led rr,gg,bb
    SetLedColor(Color),
    /// Reset led color: !led
    ResetLedColor,
    /// Ack alarm: !ack
    AckAlarm,
}

impl<const WRITE_BUFFER_SIZE: usize, const READ_BUFFER_SIZE: usize>
    SerialBuffer<WRITE_BUFFER_SIZE, READ_BUFFER_SIZE>
{
    /// Actually flush (print) data from the output buffer on the USART.
    pub fn flush(&mut self) {
        avr_device::interrupt::free(|cs| {
            if let Some(ref mut usart) = USART_MUTEX.borrow(cs).borrow_mut().as_mut() {
                if !self.output.is_empty() {
                    while let Some(byte) = self.output.dequeue() {
                        usart.write_byte(byte);
                    }
                }
            }
        });
    }

    /// Load bytes from the USART to the (input) ring buffer of this structure.
    pub fn load(&mut self) {
        avr_device::interrupt::free(|cs| {
            let mut serial_read_buffer = SERIAL_READ_BUFFER.borrow(cs).borrow_mut();
            while let Some(byte) = serial_read_buffer.dequeue() {
                self.input.push(byte);
            }
        });
    }

    /// Try to dequeue a command from the input buffer. Return
    /// - Ok(Some()) when a command is identified
    /// - Err(()) when a \n separator was found, but no valid command
    ///   could be parsed
    /// - Ok(None) most of the time, when there is no/not enough data
    /// Bytes are also removed from the ring buffer.
    pub fn dequeue_command(&mut self) -> Result<Option<Command>, ()> {
        while let Some(&byte) = self.input.peek() {
            if byte == b'!' || byte == b'?' {
                break;
            } else {
                self.input.dequeue();
            }
        }
        match self.input.iter().position(|&b| b == b'\n') {
            Some(3) => match &self.dequeue_to_array() {
                b"?dt" => Ok(Some(Command::QueryDatetime)),
                b"?dw" => Ok(Some(Command::QueryDawnDuration)),
                b"?wk" => Ok(Some(Command::Query(SunriseSelection::Week))),
                b"?we" => Ok(Some(Command::Query(SunriseSelection::WeekEnd))),
                b"?77" => Ok(Some(Command::QueryLastDcf77Update)),
                _ => Err(()),
            },
            Some(4) => match self.dequeue_to_array() {
                [b'!', b'l', b'e', b'd'] => Ok(Some(Command::ResetLedColor)),
                [b'!', b'a', b'c', b'k'] => Ok(Some(Command::AckAlarm)),
                _ => Err(()),
            },
            Some(6) => match self.dequeue_to_array() {
                [b'?', b'p', b'h', b'a', b's', b'e'] => Ok(Some(Command::QueryPhase)),
                [b'!', b'd', b'b', b'g', b'7', b'7'] => Ok(Some(Command::DebugDcf77)),
                _ => Err(()),
            },
            Some(8) => match self.dequeue_to_array() {
                [b'!', b'd', b'a', b'w', b'n', b' ', m1, m2] => {
                    let minute = Self::decode_two_ascii_digits(m1, m2, 10)?;
                    Ok(Some(Command::SetDawn(minute)))
                }
                _ => Err(()),
            },
            Some(9) => match self.dequeue_to_array() {
                [b'!', b'w', s, b' ', h1, h2, b':', m1, m2] => {
                    let hour = Self::decode_two_ascii_digits(h1, h2, 10)?;
                    let minute = Self::decode_two_ascii_digits(m1, m2, 10)?;
                    let time = Time {
                        hour,
                        minute,
                        second: None,
                    };

                    match s {
                        b'k' => Ok(Some(Command::Set(SunriseSelection::Week, time))),
                        b'e' => Ok(Some(Command::Set(SunriseSelection::WeekEnd, time))),
                        _ => Err(()),
                    }
                }
                _ => Err(()),
            },
            Some(13) => match self.dequeue_to_array() {
                [b'!', b'l', b'e', b'd', b' ', r1, r2, b',', g1, g2, b',', b1, b2] => {
                    Ok(Some(Command::SetLedColor(Color {
                        red: Self::decode_two_ascii_digits(r1, r2, 0x10)?,
                        green: Self::decode_two_ascii_digits(g1, g2, 0x10)?,
                        blue: Self::decode_two_ascii_digits(b1, b2, 0x10)?,
                    })))
                }
                _ => Err(()),
            },
            Some(_) => {
                while Some(b'\n') != self.input.dequeue() {}
                Err(())
            }
            None => Ok(None),
        }
    }

    /// Dequeue from the input buffer to an array of a given size,
    /// dequeing (and dropping) the separator \n
    fn dequeue_to_array<const T: usize>(&mut self) -> [u8; T] {
        let mut array = [0u8; T];
        for byte in array.iter_mut() {
            *byte = self.input.dequeue().unwrap();
        }
        // Dequeue the next \n
        self.input.dequeue();
        array
    }

    /// Decode d1d2 where d1 and d2 are digits in ascii, in a given base
    fn decode_two_ascii_digits(d1: u8, d2: u8, base: u8) -> Result<u8, ()> {
        Ok(base * Self::decode_ascii_digit(d1)? + Self::decode_ascii_digit(d2)?)
    }

    /// Map an ascii char to its value, expressed as as u8
    fn decode_ascii_digit(d: u8) -> Result<u8, ()> {
        match d {
            ascii @ 0x30..=0x39 => Ok(ascii - 0x30),
            ascii @ 0x41..=0x46 => Ok(ascii - 0x37),
            ascii @ 0x61..=0x66 => Ok(ascii - 0x57),
            _ => Err(()),
        }
    }
}
