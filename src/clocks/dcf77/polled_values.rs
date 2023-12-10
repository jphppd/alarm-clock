//! Structure and methods related to the polled values
//! of the pin linked to the DCF77 receiver.
use super::{Dcf77SignalVariant, WorkflowError, POLLED_SAMPLES_FREQUENCY};

/// Duration of the history of polled values to keep
pub const POLLED_SAMPLES_HISTORY_S: u8 = 6;
/// Number of bytes necessary to store 1 second of data
const POLLED_SAMPLES_BYTES_PER_S: usize = POLLED_SAMPLES_FREQUENCY / 8;
/// Number of bytes necessary to store all the data
const POLLED_SAMPLES_BYTES: usize =
    (POLLED_SAMPLES_HISTORY_S as usize) * POLLED_SAMPLES_BYTES_PER_S;

/// Count number of ones in the binary expansion of the byte
/// given as argument: 0b1001_1101 -> 5
fn count_ones(byte: u8) -> u8 {
    const NIBBLE_LOOKUP: [u8; 16] = [0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4];
    NIBBLE_LOOKUP[(byte & 0x0F) as usize] + NIBBLE_LOOKUP[(byte >> 4) as usize]
}

/// Storage for the polled values
#[derive(Clone, Copy)]
pub(super) struct PolledValues {
    /// Current number of samples hold in value
    samples_count: usize,
    /// Little-endian bit-array of the past values.
    /// value\[0\] holds the most recent data, value[len-1] the oldest date.
    value: [u8; POLLED_SAMPLES_BYTES],
}

impl Default for PolledValues {
    /// Initialize a default structure.
    fn default() -> Self {
        Self {
            samples_count: 0,
            value: [0u8; POLLED_SAMPLES_BYTES],
        }
    }
}

impl PolledValues {
    /// Update the structure by adding a new polled value
    pub fn update(&mut self, level: bool) {
        self.samples_count += 1;
        // Saturate the count
        if self.samples_count > 8 * POLLED_SAMPLES_BYTES {
            self.samples_count = 8 * POLLED_SAMPLES_BYTES;
        }

        // Shift the "value" array by one bit, by working on every byte.
        // To link two consecutive bytes, a carry is used.
        let mut carry = if level { 1 } else { 0 };
        for byte in self.value.iter_mut() {
            let next_carry = (*byte & 0x80) != 0;
            *byte = byte.wrapping_shl(1);
            *byte |= carry;
            carry = if next_carry { 1 } else { 0 };
        }
    }

    /// Compute the scalar product between the current values
    /// and a fixed pattern representing a typical expected signal.
    pub fn scalar_product(&self) -> Option<u8> {
        if self.samples_count < EXPECTED.len() {
            return None;
        }

        // We actually use inverted signals. In the nominal signal, there is
        // much fewer "logical true" values than "logical false" (15:85 ratio).
        // Without an inverted pattern, in the scalar product, it would not be
        // possible to distinguish a true or false value during the "logical low" phase:
        // <[1,0,0,0,0], [x,0,0,1,0]> = <[1,0,0,0,0], [x,0,0,0,0]>
        // This "effect" is detrimental to the main goal (finding the best correlation).
        const EXPECTED: [u8; POLLED_SAMPLES_BYTES] = [
            0xff, 0xff, 0xff, 0xff, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0xff, 0xff, 0xff, 0xff,
            0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0xff, 0xff, 0xff,
            0xff, 0x00,
        ];

        Some(
            self.value
                .iter()
                .zip(EXPECTED)
                .map(|(actual_value, expected_byte)| count_ones(!actual_value & expected_byte))
                .sum::<u8>(),
        )
    }

    /// Identify the last dcf77 bit.
    /// This function must be called at the end of a second,
    /// just before the beginning of the next one
    /// (found thanks to the phase detector).
    pub fn last_dcf77_bit(&self) -> Result<Dcf77SignalVariant, WorkflowError> {
        // At the end of the second, the value contains:
        // - 800ms of data that are expected to be false (and ignored)
        // - 200ms of data that will decide of the bit
        // Theoretically, 800ms / (period of 25ms) = 32 samples to ignore,
        // but let's skip only 30 samples to account for the inaccuracy of the phase detection.
        // Since we expect "false" values around the 200ms, it should not matter.
        // Similarly, 1000ms / (period of 25ms) = 40 samples, and we want to add
        // 2 samples for the inaccuracy.
        // Hence, we want to evaluate the counts of "logical true" bits
        // over the bits 30 to 42: bytes 3 to 6, with some specified masks.
        let count = self.value[3..=POLLED_SAMPLES_BYTES_PER_S]
            .iter()
            .zip([0xc0, 0xff, 0x03])
            .map(|(value, mask)| count_ones(value & mask))
            .sum::<u8>();

        // The logical value of the (dcf77) bit,
        // depending on the count of the (polled) bits.
        // Protocol:
        // - 200 ms = 8 polled bits at logical true => dcf77 high
        // - 100 ms = 4 polled bits at logical true => dcf77 low
        // - 0 ms = 0 polled bits at logical true => dcf77 minute end
        match count {
            0..=2 => Ok(Dcf77SignalVariant::MinuteEnd),
            3..=6 => Ok(Dcf77SignalVariant::Low),
            7..=10 => Ok(Dcf77SignalVariant::High),
            _ => Err(WorkflowError::BadBit(count)),
        }
    }
}
