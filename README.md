# Alarm clock project

Rust code for an alarm-clock project based on an Atmega328P, with the following functionalities:

- radio-controlled time based on the DCF77 emitter,
  RTC to keep time when the signal is of bad quality;
- dawn simulator thanks to a LED strip;
- switch off the light of the display during the night,
  temporary switch it on when a motion is detected;
- two different alarms, during the week and during the week-end,
  and acknowledgement by button, motion detection or ambient light;
- programmation through serial port.

## Peripherals

TODO: electronid schematic

- RTC clock: [ds3231](https://www.analog.com/media/en/technical-documentation/data-sheets/ds3231.pdf)
- DCF77 receiver
- Motion detector [HC-SR501](https://www.mpja.com/download/31227sc.pdf)
- Luminosity sensor, LM393-based
- LED Matrix display, [MAX7219](https://www.analog.com/media/en/technical-documentation/data-sheets/max7219-max7221.pdf)
- LED strip, [WS2815B](https://www.peace-corp.co.jp/data/WS2815B_V1.0_EN_18112616281473.pdf) power supply 12V

|                    | in 3.3V | out 3.3V | in 5V | out 5V |
|--------------------|---------|----------|-------|--------|
| RTC ds3132         | x       | x        | x     | x      |
| DCF77              | x       | x        |       |        |
| Motion detector    |         | x        | x     |        |
| Luminosity sensor  | x       | x        | x     | x      |
| LED Matrix display |         |          | x     | x      |
| LED strip          |         |          | x     |        |


## Build Instructions
1. Install prerequisites as described in the [`avr-hal` README] (`avr-gcc`, `avr-libc`, `avrdude`, [`ravedude`]).

2. Run `cargo build --release` to build the firmware.

3. Run `cargo run --release` to flash the firmware to a connected board.  If `ravedude`
   fails to detect your board, check its documentation at
   <https://crates.io/crates/ravedude>.

4. `ravedude` will open a console session after flashing where you can interact
   with the UART console of your board.

[`avr-hal` README]: https://github.com/Rahix/avr-hal#readme
[`ravedude`]: https://crates.io/crates/ravedude

## License
Licensed under ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
