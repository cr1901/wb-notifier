use std::{error, fmt};
use std::cell::RefCell;

use embedded_hal::blocking::i2c::Write as I2cWrite;
use embedded_hal::blocking::delay::{DelayMs, DelayUs};
use hd44780_driver::bus::I2CMCP23008Bus;
use hd44780_driver::{Cursor, CursorBlink, Display, DisplayMode, HD44780};
use hd44780_driver::display_size::DisplaySize;

pub use wb_notifier_proto::SetBacklight;

pub struct Lcd<I2C, D> where I2C: I2cWrite {
    drv: HD44780<I2CMCP23008Bus<I2C>>,
    delay: D,
    pos: u8,
}

struct Msg(u8, String);

#[derive(Debug, Clone)]
pub enum Error {
    InitMcp,
    Init,
    Busy(u8),
    SetCursorPos,
    WriteStr,
    Clear,
    SetBacklight,
    /// Non-fatal error that indicates the driver yielded voluntarily.
    Yielded(u8)
}

impl fmt::Display for Error
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InitMcp => write!(f, "I2C expander initialization error"),
            Error::Init => write!(f, "LCD initialization error"),
            Error::SetCursorPos => write!(f, "could not set cursor pos"),
            Error::Busy(id) => write!(f, "driver busy writing msg id {id}"),
            Error::WriteStr => write!(f, "could not write string"),
            Error::Clear => write!(f, "could not clear display"),
            Error::SetBacklight => write!(f, "could not control backlight"),
            Error::Yielded(pos) => write!(f, "driver voluntarily yielded pos {pos}"),
        }
    }
}

impl error::Error for Error {}

enum LineFsm {
    Idle,
    One,
    Two,
    Three,
    Four
}

impl<I2C, D, E> Lcd<I2C, D>
where
    I2C: I2cWrite<Error = E>, D: DelayMs<u8> + DelayUs<u16> 
{
    pub fn new(i2c: I2C, mut delay: D, addr: u8) -> Result<Self, Error> {
        let drv = HD44780::new_i2c_mcp23008(i2c, addr, true, &mut delay).map_err(|_| Error::InitMcp)?;

        Ok(Lcd { drv, delay, pos: 0 })
    }

    pub fn initialize(&mut self) -> Result<(), Error> {
        self.drv.reset(&mut self.delay).map_err(|_| Error::Init)?;
        self.drv.clear(&mut self.delay).map_err(|_| Error::Init)?;
        self.drv.set_display_mode(
            DisplayMode { display: Display::On, cursor_visibility: Cursor::Visible, cursor_blink: CursorBlink::On },
            &mut self.delay
        ).map_err(|_| Error::Init)?;
        self.drv.set_display_size(DisplaySize::new(20, 4));

        Ok(())
    }

    pub fn set_backlight(&mut self, back: SetBacklight) -> Result<(), Error> {
        match back {
            SetBacklight::Off => self.drv.get_mut().set_backlight(false).map_err(|_| Error::SetBacklight)?,
            SetBacklight::On => self.drv.get_mut().set_backlight(true).map_err(|_| Error::SetBacklight)?
        }

        Ok(())
    }

    pub fn write_msg(&mut self, /* id: u8, */ msg: String)  -> Result<u8, Error> {
        self.drv.clear(&mut self.delay).map_err(|_| Error::Clear)?;
        self.drv.set_cursor_pos(0, &mut self.delay).map_err(|_| Error::SetCursorPos)?;
        self.pos = 0;

        for (i, c) in msg.chars().enumerate() {
            if i % 20 == 0 && i != 0 {
                self.drv.set_cursor_pos(i as u8, &mut self.delay).map_err(|_| Error::SetCursorPos)?;
            }

            if c.is_ascii() {
                self.drv.write_byte(c as u8, &mut self.delay).map_err(|_| Error::WriteStr)?;
            }
        }

        Ok(0)
    }
}
