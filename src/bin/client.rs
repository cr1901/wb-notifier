use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature="client")] {
        use wb_notifier_client::Client;
        use wb_notifier_proto::{Echo, EchoResponse, SetLed};
    }
}

use std::error::Error;

#[cfg(feature="client")]
fn main() -> Result<(), Box<dyn Error>> {
    let mut client = Client::new();
    client.connect("127.0.0.1:12000")?;

    let mut buf = vec![0; 1024];
    let resp = client.echo("hello!", &mut buf)?;
    println!("{}", resp);

    client.set_led(SetLed { row: 0, col: 0 }, &mut buf)?;
    println!("Server claims LED was set.");

    let res: Result<String, _> = client.raw::<Echo, EchoResponse, _, _, _>("bad/path", "hello!", &mut buf);
    println!("{:?}", res);

    Ok(())
}

#[cfg(not(feature="client"))]
fn main() -> Result<(), Box<dyn Error>> {
    println!("client feature not enabled");

    Ok(())
}
