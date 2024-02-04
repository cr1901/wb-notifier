# `wb-notifier`

This repo is a collection of code to control [I2C](https://en.wikipedia.org/wiki/I%C2%B2C)
sensors from my workbench using [Remote Procedure Calls](https://github.com/jamesmunns/postcard-rpc).
An [async executor](https://github.com/smol-rs/smol) for the server binary has
been thrown in for good measure.

This application is mainly meant for me, so documentation is lacking. However,
I need to pull it to multiple computers, so might as well release it. It is
_absolutely_ overengineered for its purpose to blink LEDs, print to an LCD, and
read sensors. However, it gives me an excuse to play with a bunch of new crates
I've never used, _along with just being fun to write_. I have no regrets.
