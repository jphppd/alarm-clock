//! Control the sequential four 8x8 matrix LED display panel.
// https://www.analog.com/media/en/technical-documentation/data-sheets/max7219-max7221.pdf
use crate::{clocks::Datetime, DisplaySpiClkOutput, DisplaySpiCsOutput, DisplaySpiMosiOutput};
use arduino_hal::port::{
    mode::{self, Io},
    Pin,
};
use core::marker::PhantomData;
use max7219::connectors::PinConnector;

type DisplayInterface = max7219::MAX7219<
    PinConnector<
        Pin<mode::Output, DisplaySpiMosiOutput>,
        Pin<mode::Output, DisplaySpiCsOutput>,
        Pin<mode::Output, DisplaySpiClkOutput>,
    >,
>;

/// Marker trait for the data layout of the matrix
pub trait DataLayout {}
/// Column major order
struct DataLayoutColumnMajor {}
/// Row major order
struct DataLayoutRowMajor {}
impl DataLayout for DataLayoutColumnMajor {}
impl DataLayout for DataLayoutRowMajor {}

/// Luminous intensity of the display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayIntensity {
    /// Display off during the night
    #[default]
    Off,
    /// Dimmed display, to avoid dazzling the user during dawn or night
    Dim,
    /// Bright display, when the ambiant light is on
    Bright,
}

/// Main structure for the display
pub struct Display {
    /// Electronic interface with the display
    display_interface: DisplayInterface,
    /// Frame buffer containing the pixels to display
    frame_buffer: FrameBuffer,
}

impl Display {
    /// Initialize the display.
    pub fn init(
        data: Pin<impl Io, DisplaySpiMosiOutput>,
        cs: Pin<impl Io, DisplaySpiCsOutput>,
        clk: Pin<impl Io, DisplaySpiClkOutput>,
    ) -> Self {
        let display_interface =
            max7219::MAX7219::from_pins(4, data.into_output(), cs.into_output(), clk.into_output())
                .unwrap();

        let mut out = Self {
            display_interface,
            frame_buffer: Default::default(),
        };

        out.clear_display();
        out
    }

    /// Clear the content of the display, leaving it blank
    pub fn clear_display(&mut self) {
        self.frame_buffer.clear();
    }

    /// Actually render (publish) the content of the frame buffer.
    pub fn render(&mut self) {
        self.frame_buffer.render(&mut self.display_interface);
    }

    /// Set the intensity of the display.
    pub fn set_intensity(&mut self, intensity: DisplayIntensity) {
        self.frame_buffer.buffer_intensity = intensity;
    }

    /// Set one or more columns of the display begininng at the specified index
    /// with the content of the slice of u8.
    pub fn set_at(&mut self, column_index: usize, value: &[u8]) {
        self.frame_buffer.set_at(column_index, value);
    }

    /// Write the time HH:MM at the beginning of the display.
    pub fn write_time(&mut self, datetime: Option<Datetime>) {
        self.frame_buffer.clear();
        match datetime {
            Some(datetime) => {
                self.frame_buffer.push(&[0]);
                self.frame_buffer.push(&[0]);

                self.frame_buffer
                    .push(Symbols::from(datetime.time.hour / 10).into());
                self.frame_buffer.push(&[0]);
                self.frame_buffer
                    .push(Symbols::from(datetime.time.hour % 10).into());

                self.frame_buffer.push(&[0]);
                self.frame_buffer.push(Symbols::Colon.into());
                self.frame_buffer.push(&[0]);

                self.frame_buffer
                    .push(Symbols::from(datetime.time.minute / 10).into());
                self.frame_buffer.push(&[0]);
                self.frame_buffer
                    .push(Symbols::from(datetime.time.minute % 10).into());

                self.frame_buffer.push(&[0]);
            }
            None => {
                self.frame_buffer.push(Symbols::Dash.into());
                self.frame_buffer.push(Symbols::Dash.into());
                self.frame_buffer.push(&[0]);
                self.frame_buffer.push(Symbols::Colon.into());
                self.frame_buffer.push(&[0]);
                self.frame_buffer.push(Symbols::Dash.into());
                self.frame_buffer.push(Symbols::Dash.into());
            }
        };
    }
}

/// Frame buffer structure
#[derive(Debug, Default, PartialEq, Eq)]
struct FrameBuffer {
    /// Actual, known, content of the display
    current: ColumnMajorBuffer,
    /// Buffered content of the display,
    /// applied at the next call to render
    buffer: ColumnMajorBuffer,
    /// Current column index, used for the push (column) method
    column_index: usize,
    /// Actual, known intensity of the display
    current_intensity: DisplayIntensity,
    /// Buffered intensity of the display,
    /// applied at the next call to render
    buffer_intensity: DisplayIntensity,
}

impl FrameBuffer {
    /// Render buffered values (if necessary) to the display.
    /// If buffered and actual values are equal, do nothing.
    fn render(&mut self, interface: &mut DisplayInterface) {
        let data_changed = self.buffer != self.current;
        let intensity_changed = self.buffer_intensity != self.current_intensity;

        if data_changed {
            self.current = self.buffer.clone();
            let squares: [Square<_>; 4] = (&self.current).into();

            for (index, square) in squares.iter().enumerate() {
                square.transpose().write(interface, index);
            }
        }

        if intensity_changed {
            self.current_intensity = self.buffer_intensity;
            match self.current_intensity {
                DisplayIntensity::Off => {
                    interface.power_off().ok();
                }
                DisplayIntensity::Dim => {
                    interface.power_on().ok();
                    interface.set_intensity(0, 0x00).unwrap();
                    interface.set_intensity(1, 0x00).unwrap();
                    interface.set_intensity(2, 0x00).unwrap();
                    interface.set_intensity(3, 0x00).unwrap();
                }
                DisplayIntensity::Bright => {
                    interface.power_on().ok();
                    interface.set_intensity(0, 0xff).unwrap();
                    interface.set_intensity(1, 0xff).unwrap();
                    interface.set_intensity(2, 0xff).unwrap();
                    interface.set_intensity(3, 0xff).unwrap();
                }
            };
        }
    }

    /// Push columns (as slice of u8) in the buffer, beginning at the current column index.
    fn push(&mut self, value: &[u8]) {
        for &byte in value {
            self.buffer.0[self.column_index] = byte;
            if self.column_index + 1 < 32 {
                self.column_index += 1;
            }
        }
    }

    /// Set columns at the specified index, setting the column index
    /// after the newly set  columns.
    fn set_at(&mut self, column_index: usize, value: &[u8]) {
        self.column_index = column_index;
        for &byte in value {
            self.buffer.0[self.column_index] = byte;
            if self.column_index + 1 < 32 {
                self.column_index += 1;
            }
        }
    }

    /// Clear the buffer.
    fn clear(&mut self) {
        self.buffer = ColumnMajorBuffer::default();
        self.column_index = 0;
    }
}

/// Column major buffer: 8 lines, 4x8=32 columns,
/// order left-to-right and top-to-bottom
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ColumnMajorBuffer([u8; 32]);

/// Single square LED matrix, 8x8 pixels
#[derive(Debug, Clone, PartialEq, Eq)]
struct Square<T: DataLayout> {
    data: [u8; 8],
    layout: PhantomData<T>,
}

impl<T: DataLayout> Default for Square<T> {
    /// Create an empty square with the correct order.
    fn default() -> Self {
        Self {
            data: Default::default(),
            layout: PhantomData,
        }
    }
}

impl Square<DataLayoutColumnMajor> {
    /// Transpose the square matrix from column major order convenient to push symbols,
    /// to the row major order, needed for the interface of the peripheral.
    fn transpose(&self) -> Square<DataLayoutRowMajor> {
        let mut data = [0u8; 8];
        for (index, column) in self.data.iter().enumerate() {
            let rank = 1 << (7 - index);
            for (row, byte) in data.iter_mut().enumerate() {
                if (column & (1 << row)) != 0 {
                    *byte += rank
                };
            }
        }
        Square {
            data,
            layout: PhantomData,
        }
    }
}

impl Square<DataLayoutRowMajor> {
    /// Write the square to the display interface.
    fn write(&self, display: &mut DisplayInterface, addr: usize) {
        display.write_raw(addr, &self.data).ok();
    }
}

impl From<&ColumnMajorBuffer> for [Square<DataLayoutColumnMajor>; 4] {
    /// Transform the continuous buffer of 32 columns
    /// to 4 squares of 8 columns.
    fn from(buffer: &ColumnMajorBuffer) -> Self {
        let mut out = [
            Square::default(),
            Square::default(),
            Square::default(),
            Square::default(),
        ];

        let mut buffer_index = 0;

        for square in out.iter_mut() {
            for byte in square.data.iter_mut() {
                *byte = buffer.0[buffer_index];
                buffer_index += 1;
            }
        }

        out
    }
}

/// Symbols available for printing
enum Symbols {
    _0,
    _1,
    _2,
    _3,
    _4,
    _5,
    _6,
    _7,
    _8,
    _9,
    Colon,
    Dash,
}

impl From<u8> for Symbols {
    /// Convert a u8 to a symbol, if possible.
    fn from(value: u8) -> Self {
        match value {
            0 => Self::_0,
            1 => Self::_1,
            2 => Self::_2,
            3 => Self::_3,
            4 => Self::_4,
            5 => Self::_5,
            6 => Self::_6,
            7 => Self::_7,
            8 => Self::_8,
            9 => Self::_9,
            _ => Self::Dash,
        }
    }
}

impl From<Symbols> for &[u8] {
    /// Convert a symbol into a slice of u8, each representing
    /// a column (over 7 rows) of the pixelized version of the symbol.
    /// Digits are 5 columns wide.
    fn from(symbol: Symbols) -> Self {
        match symbol {
            Symbols::_0 => &[0x3e, 0x51, 0x49, 0x45, 0x3e],
            Symbols::_1 => &[0x00, 0x42, 0x7f, 0x40, 0x00],
            Symbols::_2 => &[0x42, 0x61, 0x51, 0x49, 0x46],
            Symbols::_3 => &[0x22, 0x41, 0x49, 0x49, 0x36],
            Symbols::_4 => &[0x18, 0x14, 0x12, 0x7f, 0x10],
            Symbols::_5 => &[0x27, 0x45, 0x45, 0x45, 0x39],
            Symbols::_6 => &[0x3e, 0x49, 0x49, 0x49, 0x32],
            Symbols::_7 => &[0x61, 0x11, 0x09, 0x05, 0x03],
            Symbols::_8 => &[0x36, 0x49, 0x49, 0x49, 0x36],
            Symbols::_9 => &[0x26, 0x49, 0x49, 0x49, 0x3e],
            Symbols::Colon => &[0x14],
            Symbols::Dash => &[0x00, 0x08, 0x08, 0x08, 0x00],
        }
    }
}
