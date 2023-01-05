use std::{cell::RefCell, error::Error};

use cliargs_t::{Command, CommandInformation, Commander};
use eyre::{bail, eyre, Result};
use ht16k33::{Dimming, Display};
use linux_embedded_hal::I2cdev;
use reedline::{DefaultPrompt, Reedline, Signal};
use wb_notifier::bargraph::driver::{Bargraph, LedColor};

// trait CommandHelpers {
//     fn
// }

// impl<T> CommandHelpers for T where T: Command {

// }

struct OpenCommand {}

impl Command for OpenCommand {
    fn execute_command(&self, flags: std::collections::HashMap<String, String>) {
        let init = || -> Result<()> {
            if DEV.with(|f| f.borrow().is_some()) {
                bail!("device already open");
            }

            let mut i2c = I2cdev::new(flags.get("p").unwrap_or(&"/dev/i2c-1".to_string()))?;
            let addr: u8 = flags.get("a").map(|s| s.parse()).unwrap_or(Ok(0x70))?;
            i2c.set_slave_address(addr as u16)?;

            let mut bargraph = Bargraph::new(i2c, addr);
            bargraph.initialize()?;

            DEV.with(|f| {
                *f.borrow_mut() = Some(bargraph);
            });

            Ok(())
        };

        let _ = init().map_err(|e| {
            eprintln!("error openining device: {}", e);
        });
    }

    fn get_information(&self) -> cliargs_t::CommandInformation {
        CommandInformation {
            command_name: "open",
            command_help: "open bargraph device",
            flags: vec![
                cliargs_t::Flag {
                    identifier: "a",
                    flag_help: "address",
                    required: false,
                },
                cliargs_t::Flag {
                    identifier: "p",
                    flag_help: "path",
                    required: false,
                },
            ],
        }
    }
}

struct SetNCommand {}

impl Command for SetNCommand {
    fn execute_command(&self, flags: std::collections::HashMap<String, String>) {
        let _ = DEV
            .with(|f| -> Result<()> {
                let mut dev_ref = f.borrow_mut();
                let dev = dev_ref.as_mut().ok_or(eyre!("device not open"))?;

                let number = flags.get("n").unwrap().parse()?;
                let color = match flags.get("c").unwrap().as_str() {
                    "r" => LedColor::Red,
                    "g" => LedColor::Green,
                    "y" => LedColor::Yellow,
                    "off" => LedColor::Off,
                    e => return Err(eyre!("expected \"r\", \"g\", \"y\" or \"off\", got {}", e)),
                };

                dev.set_led_no(number, color)?;

                Ok(())
            })
            .map_err(|e| {
                eprintln!("error setting leds: {}", e);
            });
    }

    fn get_information(&self) -> CommandInformation {
        CommandInformation {
            command_name: "setn",
            command_help: "set leds on bargraph device",
            flags: vec![
                cliargs_t::Flag {
                    identifier: "n",
                    flag_help: "number",
                    required: true,
                },
                cliargs_t::Flag {
                    identifier: "c",
                    flag_help: "color",
                    required: true,
                },
            ],
        }
    }
}

struct DimCommand {}

impl Command for DimCommand {
    fn execute_command(&self, flags: std::collections::HashMap<String, String>) {
        let _ = DEV
            .with(|f| -> Result<()> {
                let mut dev_ref = f.borrow_mut();
                let dev = dev_ref.as_mut().ok_or(eyre!("device not open"))?;

                let pwm = match flags.get("p").unwrap().parse()? {
                    1 => Dimming::BRIGHTNESS_1_16,
                    2 => Dimming::BRIGHTNESS_2_16,
                    3 => Dimming::BRIGHTNESS_3_16,
                    4 => Dimming::BRIGHTNESS_4_16,
                    5 => Dimming::BRIGHTNESS_5_16,
                    6 => Dimming::BRIGHTNESS_6_16,
                    7 => Dimming::BRIGHTNESS_7_16,
                    8 => Dimming::BRIGHTNESS_8_16,
                    9 => Dimming::BRIGHTNESS_9_16,
                    10 => Dimming::BRIGHTNESS_10_16,
                    11 => Dimming::BRIGHTNESS_11_16,
                    12 => Dimming::BRIGHTNESS_12_16,
                    13 => Dimming::BRIGHTNESS_13_16,
                    14 => Dimming::BRIGHTNESS_14_16,
                    15 => Dimming::BRIGHTNESS_15_16,
                    16 => Dimming::BRIGHTNESS_16_16,
                    e => return Err(eyre!("expected integer between 1 and 16, got {}", e)),
                };

                dev.set_dimming(pwm)?;

                Ok(())
            })
            .map_err(|e| {
                eprintln!("error dimming leds: {}", e);
            });
    }

    fn get_information(&self) -> CommandInformation {
        CommandInformation {
            command_name: "dim",
            command_help: "set LED brightness",
            flags: vec![cliargs_t::Flag {
                identifier: "p",
                flag_help: "pwm",
                required: true,
            }],
        }
    }
}

struct BlinkCommand {}

impl Command for BlinkCommand {
    fn execute_command(&self, flags: std::collections::HashMap<String, String>) {
        let _ = DEV
            .with(|f| -> Result<()> {
                let mut dev_ref = f.borrow_mut();
                let dev = dev_ref.as_mut().ok_or(eyre!("device not open"))?;

                let rate = match flags.get("r").unwrap().as_str() {
                    "on" => Display::ON,
                    "off" => Display::OFF,
                    "0.5" => Display::HALF_HZ,
                    "1" => Display::ONE_HZ,
                    "2" => Display::TWO_HZ,
                    e => return Err(eyre!("{} could not be parsed as a blink rate", e)),
                };

                dev.set_display(rate)?;

                Ok(())
            })
            .map_err(|e| {
                eprintln!("error blinking leds: {}", e);
            });
    }

    fn get_information(&self) -> CommandInformation {
        CommandInformation {
            command_name: "blink",
            command_help: "set LED blink rate",
            flags: vec![cliargs_t::Flag {
                identifier: "r",
                flag_help: "rate",
                required: true,
            }],
        }
    }
}

struct SetCommand {}

impl Command for SetCommand {
    fn execute_command(&self, flags: std::collections::HashMap<String, String>) {
        let _ = DEV
            .with(|f| -> Result<()> {
                let mut dev_ref = f.borrow_mut();
                let dev = dev_ref.as_mut().ok_or(eyre!("device not open"))?;

                let row = flags.get("r").unwrap().parse()?;
                let col = flags.get("c").unwrap().parse()?;
                let state = match flags.get("s").unwrap().as_str() {
                    "on" => true,
                    "off" => false,
                    e => return Err(eyre!("expected \"on\" or \"off\", got {}", e)),
                };

                dev.set_led(row, col, state)?;

                Ok(())
            })
            .map_err(|e| {
                eprintln!("error setting leds: {}", e);
            });
    }

    fn get_information(&self) -> CommandInformation {
        CommandInformation {
            command_name: "set",
            command_help: "set leds on bargraph device",
            flags: vec![
                cliargs_t::Flag {
                    identifier: "r",
                    flag_help: "row",
                    required: true,
                },
                cliargs_t::Flag {
                    identifier: "c",
                    flag_help: "col",
                    required: true,
                },
                cliargs_t::Flag {
                    identifier: "s",
                    flag_help: "state",
                    required: true,
                },
            ],
        }
    }
}

struct ResetCommand {}

impl Command for ResetCommand {
    fn execute_command(&self, _flags: std::collections::HashMap<String, String>) {
        let _ = DEV
            .with(|f| -> Result<()> {
                let mut dev_ref = f.borrow_mut();
                let dev = dev_ref.as_mut().ok_or(eyre!("device not open"))?;
                dev.initialize()?;

                Ok(())
            })
            .map_err(|e| {
                eprintln!("error resetting leds: {}", e);
            });
    }

    fn get_information(&self) -> CommandInformation {
        CommandInformation {
            command_name: "reset",
            command_help: "reset LEDs to initial state",
            flags: vec![],
        }
    }
}

thread_local! {
    pub static DEV: RefCell<Option<Bargraph<I2cdev>>> = RefCell::new(None);
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut line_editor = Reedline::create();
    let prompt = DefaultPrompt::default();

    let open: Box<dyn Command> = Box::new(OpenCommand {});
    let setn: Box<dyn Command> = Box::new(SetNCommand {});
    let dim: Box<dyn Command> = Box::new(DimCommand {});
    let blink: Box<dyn Command> = Box::new(BlinkCommand {});
    let set: Box<dyn Command> = Box::new(SetCommand {});
    let reset: Box<dyn Command> = Box::new(ResetCommand {});

    let mut commands = vec![open, setn, dim, blink, set, reset];
    let cmdr = Commander::new(&mut commands);

    loop {
        let sig = line_editor.read_line(&prompt);
        match sig {
            Ok(Signal::Success(buffer)) => {
                cmdr.handle_input(buffer);
            }
            Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                println!("\nAborted!");
                break;
            }
            x => {
                println!("Event: {:?}", x);
            }
        }
    }

    Ok(())
}
