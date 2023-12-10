//! Phase detector, to detect the beginning of a second
//! in the polled bits.
use super::POLLED_SAMPLES_FREQUENCY;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};

/// Structure for the phase detector
pub(super) struct PhaseDetector {
    /// Buffer of the previous scalar products
    sp_buffer: ConstGenericRingBuffer<u8, POLLED_SAMPLES_FREQUENCY>,
    /// Previous height above moving average, used to detect peaks
    prev_height_above_ma: u8,
    /// Don't detect more than one peak during one second
    peak_detected_for_current_second: bool,
}

impl Default for PhaseDetector {
    /// Initialize a default phase detector
    fn default() -> Self {
        Self {
            sp_buffer: ConstGenericRingBuffer::new(),
            prev_height_above_ma: 0,
            peak_detected_for_current_second: false,
        }
    }
}

impl PhaseDetector {
    /// Return true if a peak is detected.
    /// In our caracterisation, a peak is found when the scalar product given as argument
    /// is, for the first time, lower than the previous one.
    /// To eliminate spurious detections from ripples, we want the scalar product to be "much" larger
    /// than a typical threshold value, a shifted moving average.
    pub fn detect_peak(&mut self, scalar_product: u8) -> bool {
        self.sp_buffer.push(scalar_product);
        if !self.sp_buffer.is_full() {
            return false;
        }

        // Compute the typical value of the scalar product
        let moving_average =
            self.sp_buffer.iter().map(|v| *v as u16).sum::<u16>() / POLLED_SAMPLES_FREQUENCY as u16;
        let moving_average = moving_average as u8;

        // Compute the height of the scalar product above a threshold.
        // It was observed that a shift of 4 above the moving average should eliminate the ripples,
        // even with a reasonably noisy signal.
        let height_above_ma = scalar_product.saturating_sub(moving_average + 4);

        // Core of the detection
        let peak_found =
            !self.peak_detected_for_current_second && height_above_ma < self.prev_height_above_ma;
        if peak_found {
            self.peak_detected_for_current_second = true;
        }
        if height_above_ma == 0 {
            // When the scalar product returns below the threshold,
            // we consider that a new second ahs begun
            self.peak_detected_for_current_second = false;
        }

        self.prev_height_above_ma = height_above_ma;

        peak_found
    }
}
