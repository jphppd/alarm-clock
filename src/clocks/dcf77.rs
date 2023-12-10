//! DCF77 decoder: take as input the bits polled from the receiver,
//! and return a complete datetime.
use self::phase_detector::PhaseDetector;
use self::polled_values::PolledValues;
pub use self::protocol::Dcf77SignalVariant;
use self::protocol::{ProtocolError, Signals};
use super::{
    timer::{get_polled_values, Timer, POLLED_SAMPLES_PERIOD_MS},
    Datetime,
};
use ringbuffer::RingBuffer;

pub const POLLED_SAMPLES_FREQUENCY: usize = 1000 / POLLED_SAMPLES_PERIOD_MS as usize;

mod phase_detector;
mod polled_values;
mod protocol;

/// Errors that may arise during the decoding process
pub enum WorkflowError {
    MissedPolledValue(Timer),
    LastPeakTooClose(Timer),
    LastPeakTooFar(Timer),
    BadBit(u8),
    Protocol(ProtocolError),
}

impl From<ProtocolError> for WorkflowError {
    fn from(e: ProtocolError) -> Self {
        Self::Protocol(e)
    }
}

impl ufmt::uDisplay for WorkflowError {
    /// Format the error to display on the serial.
    fn fmt<W>(&self, f: &mut ufmt::Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: ufmt::uWrite + ?Sized,
    {
        match self {
            WorkflowError::MissedPolledValue(ms) => {
                f.write_str("missed polled value ").and_then(|_| ms.fmt(f))
            }
            WorkflowError::LastPeakTooClose(ms) => {
                f.write_str("last peak too close ").and_then(|_| ms.fmt(f))
            }
            WorkflowError::LastPeakTooFar(ms) => {
                f.write_str("last peak too far ").and_then(|_| ms.fmt(f))
            }
            WorkflowError::BadBit(count) => f.write_str("bad bit ").and_then(|_| count.fmt(f)),
            WorkflowError::Protocol(e) => e.fmt(f),
        }
    }
}

/// Dcf77 decoder main structure
#[derive(Default)]
pub struct Dcf77 {
    /// Time of the last polled value
    last_update_timer: Option<Timer>,
    /// Array holding the past boolean polled values of the DFC77
    /// pin input at a "high" rate
    polled_values: PolledValues,
    /// Detector of the phase of the signal, that is,
    /// detect the beginning of UTC seconds among the polled values
    phase_detector: PhaseDetector,
    /// Time of the last detected peak of the correlation
    /// between the samples and the pattern, signaling a new bit
    last_peak_update: Option<Timer>,
    /// Array holding the decoded, 1Hz-bits of the DCF77 signal
    signals: Signals,
}

impl Dcf77 {
    /// Main call
    pub fn run(&mut self) -> Result<Option<Datetime>, WorkflowError> {
        // polled_value becomes Some almost immediately after the boot
        if let Some((timer, polled_value)) = get_polled_values() {
            if let Some(bit) = self.process_new_polled_values(timer, polled_value)? {
                // A new bit has been detected.
                self.signals.push(bit);
                // All the bits recorded before the beginning of a minute are useless.
                self.signals.clear_up_to_first_minute_end();

                // Decode the array of bits as a datetime (if possible).
                return Ok(self
                    .signals
                    .get_proto()
                    .map(Datetime::try_from)
                    .transpose()?);
            }
        }

        Ok(None)
    }

    /// Detect if the container of the polled value holds a new one;
    /// if so, process it, and if a new bit of the signal, return it.
    pub fn process_new_polled_values(
        &mut self,
        current_timer: Timer,
        polled_value: bool,
    ) -> Result<Option<Dcf77SignalVariant>, WorkflowError> {
        // A new polled value is expected at exactly
        // a difference of POLLED_SAMPLES_PERIOD_MS.
        if let Some(last_update_timer) = self.last_update_timer {
            let diff = current_timer - last_update_timer;
            if diff < Timer(POLLED_SAMPLES_PERIOD_MS) {
                return Ok(None);
            }
            if diff > Timer(POLLED_SAMPLES_PERIOD_MS) {
                self.last_update_timer = None;
                return Err(WorkflowError::MissedPolledValue(diff));
            }
        }
        self.last_update_timer = Some(current_timer);
        self.polled_values.update(polled_value);

        // To find the phase, the scalar product of the current bits
        // (of the polled values) with as predefined pattern is computed,
        // for instance:
        // ...**......*........*......**... samples
        // **......**......**......**...... pattern
        // This correlation is at a maximum (a peak) when the pattern
        // and the samples are timely aligned.
        match self.polled_values.scalar_product() {
            None => Ok(None),
            Some(sp) => {
                if !self.phase_detector.detect_peak(sp) {
                    // Wait for a subsequent polled value to detect a peak.
                    return Ok(None);
                }

                // Try to avoid false-positives, we expect 1s between peaks,
                // plus a margin to account for the inaccuracy of the internal
                // clock.
                if let Some(last_peak_update_timer) = self.last_peak_update {
                    let diff = current_timer - last_peak_update_timer;
                    if diff < Timer(900) {
                        self.last_peak_update = None;
                        return Err(WorkflowError::LastPeakTooClose(diff));
                    }
                    if diff > Timer(1100) {
                        self.last_peak_update = None;
                        return Err(WorkflowError::LastPeakTooFar(diff));
                    }
                }

                self.last_peak_update = self.last_update_timer;
                // Return the last (most recent) identified bit of dcf77
                Some(self.polled_values.last_dcf77_bit()).transpose()
            }
        }
    }
}
