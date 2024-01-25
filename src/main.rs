use smol;
use wb_notifier_server::Server;

use smol::LocalExecutor;
use std::{error::Error, rc::Rc};

fn main() -> Result<(), Box<dyn Error>> {
    let server = Server::new();
    let ex = Rc::new(LocalExecutor::new());
    smol::block_on(ex.run(server.main_loop(ex.clone())))?;
    Ok(())
}
