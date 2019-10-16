use failure::{self as f, Error};
use std::io::{self, Write};
use std::process;
use std::thread;
use std::time::Duration;

use libsrt_rs::net::Builder;
use libsrt_rs::net::Connect;
use libsrt_rs::net::{EventKind, Events, Poll, Token};

fn main() {
    if let Err(err) = run() {
        eprintln!("{}", err);
        process::exit(1);
    }
}

fn run() -> Result<(), Error> {
    const TOKEN: Token = Token(0);

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 1 {
        return Err(f::err_msg("Usage: test-client"));
    }

    let poll = Poll::new()?;
    let mut events = Events::with_capacity(2);

    let addr = args[0].parse()?;
    let mut stream = Builder::new()
        .nonblocking(true)
        .connect(&addr)?;

    poll.register(&stream, TOKEN, EventKind::writable())?;
    poll.poll(&mut events, Some(Duration::from_millis(1000)))?;
    if events.iter().next().is_none() {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "connection timeout").into());
    }
    println!("connection established to {}", stream.peer_addr()?);

    poll.reregister(&stream, TOKEN, EventKind::writable() | EventKind::error())?;

    let message = format!("This message should be sent to the other side");
    'outer: for i in 0..100 {
        println!("write #{} {}", i, message);

        let mut _nsent = 0;
        loop {
            events.clear();

            poll.poll(&mut events, None)?;

            for event in &events {
                match event.token() {
                    TOKEN => {
                        if event.kind().is_error() {
                            println!("connection closed");
                            break 'outer;
                        }
                    }
                    _ => unreachable!(),
                }
            }

            match stream.write(&message.as_bytes()[_nsent..]) {
                Ok(len) => {
                    _nsent += len;
                    if _nsent == message.len() {
                        _nsent = 0;
                        break;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }

        thread::sleep(Duration::from_millis(100));
    }

    // XXX To avoid the error message:
    // SRT:RcvQ:worker!!FATAL!!:SRT.c: CChannel reported ERROR DURING TRANSMISSION - IPE. INTERRUPTING worker anyway.
    poll.reregister(&stream, TOKEN, EventKind::error())?;
    events.clear();
    poll.poll(&mut events, Some(Duration::from_millis(1000)))?;

    poll.deregister(&stream)?;

    Ok(())
}