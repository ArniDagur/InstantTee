extern crate nix;

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{stdin, stdout, stderr, Write};
use std::os::unix::io::{AsRawFd, RawFd};
use std::process;
use std::convert;

use nix::fcntl::{tee, splice, SpliceFFlags};
use nix::unistd::pipe;

const BUF_SIZE: usize = 1024 * 16;
const HELP: &'static str = r#"InstantTee ~ tee but a little faster
Copy standard input to each FILE, and also to standard output.

Options:
    -h, --help | Display this help message

Contact:
    Árni Dagur <arnidg@protonmail.ch>
    https://github.com/ArniDagur/InstantTee
"#;

struct FilePipePair {
    file: File,
    pipe_rd: RawFd,
    pipe_wr: RawFd
}
impl convert::From<File> for FilePipePair {
    fn from(file: File) -> Self {
        let (pipe_rd, pipe_wr) = pipe().unwrap();
        FilePipePair {
            file,
            pipe_rd,
            pipe_wr
        }
    }
}

fn instanttee(files: Vec<String>) {
    // We create two pipes
    let (main_pipe_rd, main_pipe_wr) = pipe().unwrap();

    let mut fpps = Vec::new();
    for file in files {
        fpps.push(FilePipePair::from(
            OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(file)
            .unwrap()
        ));
    }

    let stdin = stdin();
    let _handle0 = stdin.lock();
    let stdout = stdout();
    let _handle1 = stdout.lock();
    let stderr = stderr();
    let mut stderr = stderr.lock();

    loop {
        // Copy stdin to main pipe
        let bytes_copied = splice(
            stdin.as_raw_fd(),
            None,
            main_pipe_wr,
            None,
            BUF_SIZE,
            SpliceFFlags::empty(),
        ).unwrap_or_else(|err| {
            write!(stderr,
                "Error when attempting to splice stdin to pipe:\n{}", err
            ).unwrap();
            stderr.flush().unwrap();
            process::exit(1);
        });
        if bytes_copied == 0 {
            // We read 0 bytes from the input,
            // which means we're done copying.
            break;
        }
        // Copy stdin from main pipe to FilePipePair pipes
        for fpp in &fpps {
            tee(
                main_pipe_rd,
                fpp.pipe_wr,
                bytes_copied,
                SpliceFFlags::empty()
            ).unwrap_or_else(|err| {
                write!(stderr,
                    "Error when attempting to tee stdin to pipe:\n{}", err
                ).unwrap();
                stderr.flush().unwrap();
                process::exit(1);
            });
        }
        // Copy stdin from main pipe to stdout
        splice(
            main_pipe_rd,
            None,
            stdout.as_raw_fd(),
            None,
            BUF_SIZE,
            SpliceFFlags::empty(),
        ).unwrap_or_else(|err| {
            write!(stderr,
                "Error when attempting to splice stdin to stdout:\n{}", err
            ).unwrap();
            stderr.flush().unwrap();
            process::exit(1);
        });
        // Copy from the FilePipePair pipes to FilePipePair files
        for fpp in &fpps {
            splice(
                fpp.pipe_rd,
                None,
                fpp.file.as_raw_fd(),
                None,
                BUF_SIZE,
                SpliceFFlags::empty(),
            ).unwrap_or_else(|err| {
                write!(stderr,
                    "Error when attempting to splice to file:\n{}", err
                ).unwrap();
                stderr.flush().unwrap();
                process::exit(1);
            });
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", HELP);
        process::exit(0);
    }
    instanttee(args);
}
