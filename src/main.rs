use smol;
use wb_notifier_server::Server;

use smol::LocalExecutor;
use std::rc::Rc;

fn main() {
    let server = Server::new();
    let ex = Rc::new(LocalExecutor::new());
    smol::block_on(ex.run(server.main_loop(ex.clone())));
}
