use smol;
use wb_notifier_server::Server;

fn main() {
    let server = Server::new();
    smol::block_on(server.main_loop());
}
