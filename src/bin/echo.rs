use std::{error::Error, net::UdpSocket};

use wb_notifier_proto::{ECHO, Echo, EchoResponse};
use postcard::from_bytes;
use postcard_rpc::headered::{extract_header_from_bytes, to_slice_keyed};
use postcard_rpc::Key;

fn main() -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("127.0.0.1:12000")?;

    let mut buf = vec![0; 1024];

    let key = Key::for_path::<Echo>(ECHO);
    let req = to_slice_keyed(0, key, &Echo(String::from("hello!")), &mut buf)?;
    socket.send(&req)?;

    socket.recv(&mut buf)?;
    if let Ok((hdr, rest))  = extract_header_from_bytes(&buf) {
        if hdr.seq_no == 0 && hdr.key == key {
            if let Ok(payload) = from_bytes::<EchoResponse>(&rest) {
                println!("{}", payload.0);
            }
        }
    }

    Ok(())
}
