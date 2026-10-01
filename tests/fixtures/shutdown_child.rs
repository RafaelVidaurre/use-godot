use std::io::{self, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code: i32 = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(0);
    let mode = args.get(2).map(String::as_str).unwrap_or("normal");
    let capture = include_bytes!("windows-shutdown.txt");
    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    // Exceed pipe capacity on both streams to expose sequential-drain deadlocks.
    if mode == "large" {
        for _ in 0..2048 {
            out.write_all(&[b'o'; 128]).unwrap();
            out.write_all(b"\n").unwrap();
            err.write_all(&[b'e'; 128]).unwrap();
            err.write_all(b"\n").unwrap();
        }
    }
    out.write_all(b"stdout marker\r\n").unwrap();
    err.write_all(b"SCRIPT ERROR: unrelated failure remains visible\n").unwrap();
    out.write_all(capture).unwrap();
    err.write_all(capture).unwrap();
    if mode == "after" {
        out.write_all(b"ordinary output after diagnostics\n").unwrap();
        err.write_all(b"ERROR: unrelated final error\n").unwrap();
    }
    out.flush().unwrap();
    err.flush().unwrap();
    std::process::exit(code);
}
