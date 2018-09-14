extern crate nix;
#[macro_use]
extern crate log;

use std::env;
use std::fs::File;
use std::io::{stdin, stdout, stderr};
use std::os::unix::io::{AsRawFd, RawFd};

use nix::fcntl::{tee, splice, SpliceFFlags};
use nix::unistd::pipe;

use std::process;

const BUF_SIZE: usize = 1024 * 16;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let file_fd = File::open(&args[0])
        .expect("No such path exists")
        .as_raw_fd();
    // We create two pipes
    let (pipe0_rd, pipe0_wr) = pipe().unwrap();
    let (pipe1_rd, pipe1_wr) = pipe().unwrap();

    let stdin = stdin();
    let _handle0 = stdin.lock();
    let stdout = stdout();
    let _handle1 = stdout.lock();

    loop {
        // stdin -> pipe
        let bytes_copied = splice(
            stdin.as_raw_fd(),
            None,
            pipe0_wr,
            None,
            BUF_SIZE,
            SpliceFFlags::empty(),
        ).unwrap();
        if bytes_copied == 0 {
            // We read 0 bytes from the input,
            // which means we're done copying.
            break;
        }
        // Make sure pipe0 and pipe1 have the same data
        let n = tee(
            pipe0_rd,
            pipe1_wr,
            bytes_copied,
            SpliceFFlags::empty()
        ).unwrap_or_else(|_| {
            println!("Error at tee");
            process::exit(1);
        });
        // Copy to standard output
        splice(
            pipe0_rd,
            None,
            stdout.as_raw_fd(),
            None,
            // What should I put here? n or BUF_SIZE?
            BUF_SIZE,
            SpliceFFlags::empty(),
        ).unwrap_or_else(|err| {
            println!("Error at splice1 {}", err);
            process::exit(1);
        });
        // Copy to file
        splice(
            pipe1_rd,
            None,
            file_fd,
            None,
            BUF_SIZE,
            SpliceFFlags::empty(),
        ).unwrap_or_else(|_| {
            println!("Error at splice2");
            process::exit(1);
        });
    }
}
