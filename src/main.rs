use argh::FromArgs;
use config::Config;
use directories::ProjectDirs;
use eyre::{bail, eyre, Result};
use linux_embedded_hal::I2cdev;
use serde::Deserialize;
use wb_notifier::bargraph::{Bargraph, LedColor};

#[derive(Deserialize, Hash)]
struct WbInfo {
    devices: Vec<Device>
}

#[derive(Deserialize, Hash)]
struct Device {
    name: String,
    addr: u8,
    driver: Driver,
}

#[derive(Deserialize, Hash)]
enum Driver {
    Bargraph,
    Hd44780
}

#[derive(FromArgs)]
/// Workbench notifier daemon
struct ServerArgs {
    /// config file override
    #[argh(option, short = 'f')]
    cfg_file: Option<String>,
    /// do not exit if communication failure with device
    #[argh(switch, short = 'r')]
    relaxed: bool,
    /// port to bind to
    #[argh(option, short = 'p')]
    port: u16,
    /// i2c bus to connect to
    #[argh(positional)]
    dev: String,

}

fn main() -> Result<()> {
    let ctl: ServerArgs = argh::from_env();
    let dirs =
        ProjectDirs::from("", "", "wb-notifier").ok_or(eyre!("could not extract project directory"))?;

    let cfg_file = dirs.config_dir().join("devices.json");
    let settings = Config::builder();

    let cfgs = if let Some(cfg_file_override) = ctl.cfg_file {
        settings
            .add_source(config::File::with_name(&cfg_file_override))
            .build()?
            .try_deserialize::<WbInfo>()?
    } else {
        settings
            .add_source(config::File::with_name(&cfg_file.to_string_lossy()))
            .build()?
            .try_deserialize::<WbInfo>()?
    };

    Ok(())
}
