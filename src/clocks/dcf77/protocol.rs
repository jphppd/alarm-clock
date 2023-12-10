//! Decode the bits of the DCF77 signal
use crate::clocks::{Date, Datetime, DayOfWeek, Time};
use core::ops::{Deref, DerefMut};
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};

/// Logical bits of the signal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dcf77SignalVariant {
    High,
    Low,
    MinuteEnd,
}

impl ufmt::uDisplay for Dcf77SignalVariant {
    /// Format a bit to display on the serial port.
    fn fmt<W>(&self, f: &mut ufmt::Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: ufmt::uWrite + ?Sized,
    {
        match self {
            Dcf77SignalVariant::High => f.write_char('#'),
            Dcf77SignalVariant::Low => f.write_char('_'),
            Dcf77SignalVariant::MinuteEnd => f.write_char('|'),
        }
    }
}

/// Signal, new-type pattern for an array of bits
#[derive(Default)]
pub(super) struct Signals(ConstGenericRingBuffer<Dcf77SignalVariant, 60>);

impl Deref for Signals {
    type Target = ConstGenericRingBuffer<Dcf77SignalVariant, 60>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Signals {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Signals {
    /// Dequeue bits up to the next "minute end".
    pub fn clear_up_to_first_minute_end(&mut self) {
        if let Some(first_minute_end) = self
            .iter()
            .position(|s| s == &Dcf77SignalVariant::MinuteEnd)
        {
            for _ in 0..first_minute_end {
                self.dequeue();
            }
        } else {
            self.clear();
        }
    }

    /// Convert, if possible, the 59 candidate bits of
    /// a full minute of DCF77 into a Protocol object (an array).
    pub fn get_proto(&mut self) -> Option<Protocol> {
        if self.front() != Some(&Dcf77SignalVariant::MinuteEnd) {
            return None;
        }

        if self.len() < 60 {
            return None;
        }

        // Dequeue first element MinuteEnd
        self.dequeue();

        let mut bits = [false; 59];
        for bit in &mut bits {
            let signal = self.dequeue();

            match signal {
                Some(Dcf77SignalVariant::High) => {
                    *bit = true;
                }
                Some(Dcf77SignalVariant::Low) => {
                    *bit = false;
                }
                _ => return None,
            }
        }

        Some(Protocol { bits })
    }
}

/// A wrapper for an array made of the 59 bits of a candidate message.
pub(crate) struct Protocol {
    pub bits: [bool; 59],
}

/// Possible decode errors
pub enum ProtocolError {
    BadStartMinute,
    BadStartOfTime,
    MinuteChecksum,
    HourChecksum,
    DateChecksum,
    SummerTime,
    WeekdayValue,
    MinuteValue,
    HourValue,
    DayValue,
    MonthValue,
    YearValue,
}

impl ufmt::uDisplay for ProtocolError {
    /// Format a protocol error to display on the serial port.
    fn fmt<W>(&self, f: &mut ufmt::Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: ufmt::uWrite + ?Sized,
    {
        match self {
            ProtocolError::BadStartMinute => f.write_str("proto:start minute"),
            ProtocolError::BadStartOfTime => f.write_str("proto:start time"),
            ProtocolError::MinuteChecksum => f.write_str("proto:minute checksum"),
            ProtocolError::HourChecksum => f.write_str("proto:hour checksum"),
            ProtocolError::DateChecksum => f.write_str("proto:date checksum"),
            ProtocolError::SummerTime => f.write_str("proto:summer time"),
            ProtocolError::WeekdayValue => f.write_str("proto:weekday value"),
            ProtocolError::MinuteValue => f.write_str("proto:minute value"),
            ProtocolError::HourValue => f.write_str("proto:hour value"),
            ProtocolError::DayValue => f.write_str("proto:day value"),
            ProtocolError::MonthValue => f.write_str("proto:month value"),
            ProtocolError::YearValue => f.write_str("proto:year value"),
        }
    }
}

impl Protocol {
    pub const START_MINUTE: usize = 0;
    pub const CEST: usize = 17;
    pub const CET: usize = 18;
    pub const START_OF_TIME: usize = 20;
    pub const MINUTE_LSB: usize = 21;
    pub const MINUTE_MSB: usize = 27;
    pub const MINUTE_PARITY: usize = 28;
    pub const HOUR_LSB: usize = 29;
    pub const HOUR_MSB: usize = 34;
    pub const HOUR_PARITY: usize = 35;
    pub const DAY_OF_MONTH_LSB: usize = 36;
    pub const DAY_OF_MONTH_MSB: usize = 41;
    pub const DAY_OF_WEEK_LSB: usize = 42;
    pub const DAY_OF_WEEK_MSB: usize = 44;
    pub const MONTH_LSB: usize = 45;
    pub const MONTH_MSB: usize = 49;
    pub const YEAR_LSB: usize = 50;
    pub const YEAR_MSB: usize = 57;
    pub const DATE_PARITY: usize = 58;
    pub const DATE_LSB: usize = Protocol::DAY_OF_MONTH_LSB;
    pub const DATE_MSB: usize = Protocol::YEAR_MSB;
}

impl TryFrom<Protocol> for Datetime {
    type Error = ProtocolError;

    /// Try to convert the bits of a protocol into a datetime object
    fn try_from(protocol: Protocol) -> Result<Self, Self::Error> {
        // Check fixed values
        if protocol.bits[Protocol::START_MINUTE] {
            return Err(ProtocolError::BadStartMinute);
        }

        if !protocol.bits[Protocol::START_OF_TIME] {
            return Err(ProtocolError::BadStartOfTime);
        }

        // Check parity
        if !check_even_parity(
            &protocol.bits[Protocol::MINUTE_LSB..=Protocol::MINUTE_MSB],
            protocol.bits[Protocol::MINUTE_PARITY],
        ) {
            return Err(ProtocolError::MinuteChecksum);
        };
        if !check_even_parity(
            &protocol.bits[Protocol::HOUR_LSB..=Protocol::HOUR_MSB],
            protocol.bits[Protocol::HOUR_PARITY],
        ) {
            return Err(ProtocolError::HourChecksum);
        };
        if !check_even_parity(
            &protocol.bits[Protocol::DATE_LSB..=Protocol::DATE_MSB],
            protocol.bits[Protocol::DATE_PARITY],
        ) {
            return Err(ProtocolError::DateChecksum);
        };

        // Check CEST/CET consistency
        if !(protocol.bits[Protocol::CEST] ^ protocol.bits[Protocol::CET]) {
            return Err(ProtocolError::SummerTime);
        }

        let weekday =
            bits_slice_to_u8(&protocol.bits[Protocol::DAY_OF_WEEK_LSB..=Protocol::DAY_OF_WEEK_MSB]);
        let weekday = match weekday {
            1 => DayOfWeek::Monday,
            2 => DayOfWeek::Tuesday,
            3 => DayOfWeek::Wednesday,
            4 => DayOfWeek::Thursday,
            5 => DayOfWeek::Friday,
            6 => DayOfWeek::Saturday,
            7 => DayOfWeek::Sunday,
            _ => return Err(ProtocolError::WeekdayValue),
        };

        let minute = bits_slice_to_u8(&protocol.bits[Protocol::MINUTE_LSB..=Protocol::MINUTE_MSB]);
        if minute > 59 {
            return Err(ProtocolError::MinuteValue);
        }

        let hour = bits_slice_to_u8(&protocol.bits[Protocol::HOUR_LSB..=Protocol::HOUR_MSB]);
        if hour > 23 {
            return Err(ProtocolError::HourValue);
        }

        let day = bits_slice_to_u8(
            &protocol.bits[Protocol::DAY_OF_MONTH_LSB..=Protocol::DAY_OF_MONTH_MSB],
        );
        if day > 31 {
            return Err(ProtocolError::DayValue);
        }
        let month = bits_slice_to_u8(&protocol.bits[Protocol::MONTH_LSB..=Protocol::MONTH_MSB]);
        if month > 12 {
            return Err(ProtocolError::MonthValue);
        }

        let year = bits_slice_to_u8(&protocol.bits[Protocol::YEAR_LSB..=Protocol::YEAR_MSB]);
        if year > 99 {
            return Err(ProtocolError::YearValue);
        }

        Ok(Datetime {
            date: Date {
                year,
                month,
                day,
                weekday,
            },
            time: Time {
                hour,
                minute,
                second: None,
            },
        })
    }
}

/// Check the parity of a slice of the bits
fn check_even_parity(bits: &[bool], checksum: bool) -> bool {
    !bits.iter().fold(checksum, |acc, value| (acc ^ value))
}

/// Convert a slice of bits to a number according to
/// the pseudo-BCD encoding used in the protocol.
fn bits_slice_to_u8(bits: &[bool]) -> u8 {
    [1, 2, 4, 8, 10, 20, 40, 80]
        .iter()
        .zip(bits)
        .flat_map(|(&weight, &bit)| if bit { Some(weight) } else { None })
        .sum()
}
