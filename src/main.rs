use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature="server")] {
        use smol;
        use wb_notifier_server::Server;

        use smol::LocalExecutor;
        use std::rc::Rc;
    }
}

use std::error::Error;

#[cfg(feature="server")]
fn main() -> Result<(), Box<dyn Error>> {
    let server = Server::new();
    let ex = Rc::new(LocalExecutor::new());
    smol::block_on(ex.run(server.main_loop(ex.clone())))?;
    Ok(())
}

#[cfg(not(feature="server"))]
fn main() -> Result<(), Box<dyn Error>> {
    println!("server feature not enabled");

    Ok(())
}
