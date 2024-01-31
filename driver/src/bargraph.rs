/// Inspired by: https://github.com/jasonpeacock/led-bargraph, tweaked for
/// my purposes.
use embedded_hal::blocking::i2c::{Write, WriteRead};
use std::error;
use std::fmt;

#[allow(unused)]
use ht16k33::{Display, DisplayData, LedLocation, Oscillator, COMMONS_SIZE, HT16K33, ROWS_SIZE};

pub use ht16k33::Dimming;
pub use wb_notifier_proto::LedColor;

pub struct Bargraph<I2C> {
    drv: HT16K33<I2C>,
}

#[derive(Debug, Clone)]
pub enum Error<E> {
    Hal(E),
    OutOfRange,
}

impl<E> From<E> for Error<E> {
    fn from(value: E) -> Self {
        Error::Hal(value)
    }
}

impl<E> fmt::Display for Error<E>
where
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Hal(_) => write!(f, "HAL error"),
            Error::OutOfRange => write!(f, "LED out of range"),
        }
    }
}

impl<E> error::Error for Error<E> where E: error::Error {}

impl<I2C, E> Bargraph<I2C>
where
    I2C: Write<Error = E> + WriteRead<Error = E>,
{
    pub fn new(i2c: I2C, addr: u8) -> Self {
        let drv = HT16K33::new(i2c, addr);

        Bargraph { drv }
    }

    pub fn initialize(&mut self) -> Result<(), Error<E>> {
        self.drv.initialize()?;
        self.drv.set_display(Display::ON)?;

        Ok(())
    }

    pub fn set_led_no(&mut self, num: u8, color: LedColor) -> Result<(), Error<E>> {
        if num > 23 {
            return Err(Error::OutOfRange);
        }

        // Row and column mappings found via trial and error.
        let row = if num >= 12 { num % 4 + 4 } else { num % 4 };
        let col = (num / 4) % 3;

        let red_loc = LedLocation::new(row, col).unwrap();
        let green_loc = LedLocation::new(row + 8, col).unwrap();

        self.drv.update_display_buffer(red_loc, false);
        self.drv.update_display_buffer(green_loc, false);

        if color == LedColor::Red || color == LedColor::Yellow {
            self.drv.update_display_buffer(red_loc, true);
        }

        if color == LedColor::Green || color == LedColor::Yellow {
            self.drv.update_display_buffer(green_loc, true);
        }

        self.drv.write_display_buffer()?;

        Ok(())
    }

    pub fn set_dimming(&mut self, dim: Dimming) -> Result<(), Error<E>> {
        self.drv.set_dimming(dim)?;

        Ok(())
    }

    pub fn set_display(&mut self, disp: Display) -> Result<(), Error<E>> {
        self.drv.set_display(disp)?;

        Ok(())
    }

    pub fn free(mut self) -> I2C {
        let _ = self.drv.set_display(Display::OFF);
        self.drv.destroy()
    }
}
