//! Inputs, either of the environment or of the user
use crate::{ButtonInput, LuminosityInput, ProximityInput};
use arduino_hal::port::{
    mode::{Input, Io, PullUp},
    Pin, PinOps,
};

/// Misc. inputs
pub struct Inputs {
    /// Button to be activated by the user
    pub button: BoolInput<ButtonInput>,
    /// Ambiant luminosity sensor
    pub luminosity: BoolInput<LuminosityInput>,
    /// Infrared proximity/motion sensor
    pub proximity: BoolInput<ProximityInput>,
}

impl Inputs {
    /// Initialize all the inputs, in particular with the pinout
    /// and the electrical to logical mapping.
    pub fn init(
        button_pin: Pin<impl Io, ButtonInput>,
        button_elec_level_to_logical_level: bool,
        luminosity_pin: Pin<impl Io, LuminosityInput>,
        luminosity_elec_level_to_logical_level: bool,
        proximity_pin: Pin<impl Io, ProximityInput>,
        proximity_elec_level_to_logical_level: bool,
    ) -> Self {
        Self {
            button: BoolInput::init(button_pin, button_elec_level_to_logical_level),
            luminosity: BoolInput::init(luminosity_pin, luminosity_elec_level_to_logical_level),
            proximity: BoolInput::init(proximity_pin, proximity_elec_level_to_logical_level),
        }
    }

    /// Update the values of the inputs by reading the electric state of the pins.
    pub fn update(&mut self) {
        self.button.update();
        self.luminosity.update();
        self.proximity.update();
    }
}

/// Generic boolean input.
pub struct BoolInput<PIN: PinOps> {
    /// Pin to read the state from.
    pin: Pin<Input<PullUp>, PIN>,
    /// Mapping between the electric level of the pin (+3.3V or +5V)
    /// and the logical level of the input.
    logical_level_high: bool,
    /// Logical value of the input:
    /// true for an active state, false for an inactive state.
    pub value: bool,
}

impl<PIN: PinOps> BoolInput<PIN> {
    /// Initialize the structure.
    pub fn init(pin: Pin<impl Io, PIN>, logical_level_high: bool) -> Self {
        Self {
            pin: pin.into_pull_up_input(),
            logical_level_high,
            value: !logical_level_high,
        }
    }

    /// Update the value by reading the electric state of the pin.
    pub fn update(&mut self) {
        self.value = self.pin.is_high() == self.logical_level_high;
    }
}
