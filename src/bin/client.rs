use eyre::Result;

#[cfg(feature = "client")]
mod client {
    pub use wb_notifier_client::Client;
    pub use wb_notifier_proto::*;
}

#[cfg(feature = "client")]
use client::*;

#[cfg(feature = "client")]
fn main() -> Result<()> {
    let mut client = Client::new();
    client.connect("127.0.0.1:12000")?;

    let mut buf = vec![0; 1024];
    let resp = client.echo("hello!", &mut buf)?;
    println!("{}", resp);

    client.set_led(SetLed { num: 0, color: LedColor::Yellow }, &mut buf)?;
    println!("Server claims LED was set.");

    client.notify(Notify { num: 1, status: Status::Ok }, &mut buf)?;
    println!("Server claims LED was set/blink task started");

    client.set_dimming(SetDimming::Hi, &mut buf)?;
    println!("Server claims dimming was set.");

    let res: Result<String, _> =
        client.raw::<Echo, EchoResponse, _, _, _>("bad/path", "hello!", &mut buf);
    println!("{:?}", res);

    Ok(())
}

#[cfg(not(feature = "client"))]
fn main() -> Result<()> {
    println!("client feature not enabled");

    Ok(())
}
