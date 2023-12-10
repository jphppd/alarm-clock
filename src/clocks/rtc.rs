//! DS3231 RTC Interface
// https://www.analog.com/media/en/technical-documentation/data-sheets/ds3231.pdf

use super::{Date, Datetime, Time};
use embedded_hal::blocking::i2c;

/// Variants of enums
#[derive(Debug)]
pub enum Error<CommE> {
    /// I²C/SPI bus error
    Comm(CommE),
    /// Invalid input data provided
    InvalidInputData,
}

/// Hours in either 12-hour (AM/PM) or 24-hour format
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hours {
    /// AM [1-12]
    AM(u8),
    /// PM [1-12]
    PM(u8),
    /// 24H format [0-23]
    H24(u8),
}

/// Temperature conversion rate
///
/// This is only available on the DS3232 and DS3234 devices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TempConvRate {
    /// Once every 64 seconds (default)
    _64s,
    /// Once every 128 seconds
    _128s,
    /// Once every 256 seconds
    _256s,
    /// Once every 512 seconds
    _512s,
}

struct Register;

impl Register {
    const SECONDS: u8 = 0x00;
    const MINUTES: u8 = 0x01;
    const HOURS: u8 = 0x02;
    const DOW: u8 = 0x03;
    const DOM: u8 = 0x04;
    const MONTH: u8 = 0x05;
    const YEAR: u8 = 0x06;
}

struct BitFlags;

impl BitFlags {
    const H24_H12: u8 = 0b0100_0000;
    const AM_PM: u8 = 0b0010_0000;
    const CENTURY: u8 = 0b1000_0000;
}

const DEVICE_ADDRESS: u8 = 0b110_1000;

#[derive(Debug, Default)]
pub struct Rtc<I2C>
where
    I2C: i2c::Write + i2c::WriteRead,
{
    i2c: I2C,
}

impl<I2C, CommE> Rtc<I2C>
where
    I2C: i2c::Write<Error = CommE> + i2c::WriteRead<Error = CommE>,
{
    /// Create a new instance of the DS3231 device.
    pub fn init(i2c: I2C) -> Self {
        Rtc { i2c }
    }

    pub fn datetime(&mut self) -> Result<Datetime, Error<CommE>> {
        let mut data = [0; 8];
        self.read_data(&mut data)?;

        let year = packed_bcd_to_decimal(data[Register::YEAR as usize + 1]);
        let month = packed_bcd_to_decimal(data[Register::MONTH as usize + 1] & !BitFlags::CENTURY);
        let day = packed_bcd_to_decimal(data[Register::DOM as usize + 1]);
        let weekday = packed_bcd_to_decimal(data[Register::DOW as usize + 1])
            .try_into()
            .map_err(|_| Error::InvalidInputData)?;
        let hour = hours_from_register(data[Register::HOURS as usize + 1]);
        let minute = packed_bcd_to_decimal(data[Register::MINUTES as usize + 1]);
        let second = packed_bcd_to_decimal(data[Register::SECONDS as usize + 1]);

        Ok(Datetime {
            date: Date {
                year,
                month,
                day,
                weekday,
            },
            time: Time {
                hour: get_h24(hour),
                minute,
                second: Some(second),
            },
        })
    }

    pub fn set_datetime(&mut self, datetime: &Datetime) -> Result<(), Error<CommE>> {
        let (month, year) = month_year_to_registers(datetime.date.month, datetime.date.year);
        let payload = [
            Register::SECONDS,
            decimal_to_packed_bcd(datetime.time.second.unwrap_or_default()),
            decimal_to_packed_bcd(datetime.time.minute),
            hours_to_register(Hours::H24(datetime.time.hour))?,
            datetime.date.weekday as u8,
            decimal_to_packed_bcd(datetime.date.day),
            month,
            year,
        ];
        self.write_data(&payload)
    }
    /// Write to the RTC via the I2C interface.
    fn write_data(&mut self, payload: &[u8]) -> Result<(), Error<CommE>> {
        self.i2c.write(DEVICE_ADDRESS, payload).map_err(Error::Comm)
    }

    /// Read the RTC via the I2C interface.
    fn read_data(&mut self, payload: &mut [u8]) -> Result<(), Error<CommE>> {
        let len = payload.len();
        self.i2c
            .write_read(DEVICE_ADDRESS, &[payload[0]], &mut payload[1..len])
            .map_err(Error::Comm)
    }
}

/// Transform a decimal number to packed BCD format
fn decimal_to_packed_bcd(dec: u8) -> u8 {
    ((dec / 10) << 4) | (dec % 10)
}

/// Transform a number in packed BCD format to decimal
fn packed_bcd_to_decimal(bcd: u8) -> u8 {
    (bcd >> 4) * 10 + (bcd & 0xF)
}

fn hours_to_register<CommE>(hours: Hours) -> Result<u8, Error<CommE>> {
    match hours {
        Hours::H24(h) if h > 23 => Err(Error::InvalidInputData),
        Hours::H24(h) => Ok(decimal_to_packed_bcd(h)),
        Hours::AM(h) if !(1..=12).contains(&h) => Err(Error::InvalidInputData),
        Hours::AM(h) => Ok(BitFlags::H24_H12 | decimal_to_packed_bcd(h)),
        Hours::PM(h) if !(1..=12).contains(&h) => Err(Error::InvalidInputData),
        Hours::PM(h) => Ok(BitFlags::H24_H12 | BitFlags::AM_PM | decimal_to_packed_bcd(h)),
    }
}

fn hours_from_register(data: u8) -> Hours {
    if is_24h_format(data) {
        Hours::H24(packed_bcd_to_decimal(data & !BitFlags::H24_H12))
    } else if is_am(data) {
        Hours::AM(packed_bcd_to_decimal(
            data & !(BitFlags::H24_H12 | BitFlags::AM_PM),
        ))
    } else {
        Hours::PM(packed_bcd_to_decimal(
            data & !(BitFlags::H24_H12 | BitFlags::AM_PM),
        ))
    }
}

fn month_year_to_registers(month: u8, year: u8) -> (u8, u8) {
    (decimal_to_packed_bcd(month), decimal_to_packed_bcd(year))
}

fn is_24h_format(hours_data: u8) -> bool {
    hours_data & BitFlags::H24_H12 == 0
}

fn is_am(hours_data: u8) -> bool {
    hours_data & BitFlags::AM_PM == 0
}

fn get_h24(hour: Hours) -> u8 {
    match hour {
        Hours::H24(h) => h,
        Hours::AM(h) => h,
        Hours::PM(h) => h + 12,
    }
}
