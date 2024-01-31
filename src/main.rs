use eyre::{eyre, Result};

#[cfg(feature = "server")]
mod server {
    pub use smol;
    pub use wb_notifier_proto::Device;
    pub use wb_notifier_server::Server;

    pub use smol::LocalExecutor;
    pub use std::net::Ipv4Addr;
    pub use std::rc::Rc;

    pub use argh::{self, FromArgs};
    pub use config::Config;
    pub use directories::ProjectDirs;
    use serde::Deserialize;

    #[derive(FromArgs)]
    /// Workbench notifier daemon
    pub struct ServerArgs {
        /// config file override
        #[argh(option, short = 'f')]
        pub cfg_file: Option<String>,
        /// do not exit if communication failure with device
        #[argh(switch, short = 'r')]
        #[allow(unused)]
        pub relaxed: bool,
        /// port to bind to
        #[argh(option, short = 'p', default = "12000")]
        pub port: u16,
        /// i2c bus to connect to
        #[argh(positional)]
        pub dev: String,
    }

    #[derive(Deserialize, Hash)]
    pub struct WbInfo {
        pub devices: Vec<Device>,
    }
}

#[cfg(feature = "server")]
use server::*;

#[cfg(feature = "server")]
fn main() -> Result<()> {
    let args: ServerArgs = argh::from_env();
    let dirs = ProjectDirs::from("", "", "wb-notifier")
        .ok_or(eyre!("could not extract project directory"))?;

    let cfg_file = dirs.config_dir().join("workbench.json");
    let settings = Config::builder();

    #[allow(unused)]
    let cfgs = if let Some(cfg_file_override) = args.cfg_file {
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

    let server = Server::new((Ipv4Addr::new(0, 0, 0, 0), args.port).into(), cfgs.devices);
    let ex = Rc::new(LocalExecutor::new());
    smol::block_on(ex.run(server.main_loop(ex.clone())))?;
    Ok(())
}

#[cfg(not(feature = "server"))]
fn main() -> Result<()> {
    println!("server feature not enabled");

    Ok(())
}
