extern crate nix;
extern crate getopts;

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{stdin, stdout};
use std::os::unix::io::{AsRawFd, RawFd};
use std::process;
use std::convert;

use nix::fcntl::{tee, splice, SpliceFFlags};
use nix::unistd::pipe;
use getopts::Options;

const BUF_SIZE: usize = 1024 * 64;

struct FilePipePair {
    offset: i64,
    file: File,
    pipe_rd: RawFd,
    pipe_wr: RawFd
}
impl convert::From<File> for FilePipePair {
    fn from(file: File) -> Self {
        let (pipe_rd, pipe_wr) = pipe().unwrap();
        FilePipePair {
            offset: file.metadata().unwrap().len() as i64,
            file,
            pipe_rd,
            pipe_wr
        }
    }
}

fn instanttee(files: Vec<String>, append: bool) {
    // We create two pipes
    let (main_pipe_rd, main_pipe_wr) = pipe().unwrap();

    let mut fpps = Vec::new();
    for file in files {
        fpps.push(FilePipePair::from(
            OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            // Truncate file to 0 bytes if it exists and `append` is not true
            .truncate(!append)
            .open(&file)
            .unwrap_or_else(|_| {
                eprintln!("Error when attempting to create file '{}'", file);
                process::exit(1);
            })
        ));
    }

    let stdin = stdin();
    let _handle0 = stdin.lock();
    let stdout = stdout();
    let _handle1 = stdout.lock();

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
            eprintln!("Error when attempting to splice stdin to pipe: {}", err);
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
                eprintln!("Error when attempting to tee stdin to pipe: {}", err);
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
            eprintln!("Error when attempting to splice stdin to stdout: {}", err);
            process::exit(1);
        });
        // Copy from the FilePipePair pipes to FilePipePair files
        for fpp in &mut fpps {
            splice(
                fpp.pipe_rd,
                None,
                fpp.file.as_raw_fd(),
                Some(&mut fpp.offset),
                BUF_SIZE,
                SpliceFFlags::empty(),
            ).unwrap_or_else(|err| {
                eprintln!("Error when attempting to splice to file: {}", err);
                process::exit(1);
            });
        }
    }
}

fn print_help(called_program: &str, opts: Options) {
    let brief = format!(
r#"Usage: {} [OPTION]... [FILE]...
Copy standard input to each FILE, and also to standard output."#,
        called_program
    );
    print!("{}", opts.usage(&brief));
    process::exit(0);
}

fn main() {
    // Argument handling
    let args: Vec<String> = env::args().collect();
    let called_program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("h", "help", "display this help message");
    opts.optflag("a", "append", "append to the given FILES, do not overwrite");

    let matches = match opts.parse(args) {
        Ok(matches) => matches,
        Err(failure) => {
            eprintln!("{}", failure.to_string());
            process::exit(1);
        }
    };
    if matches.opt_present("h") {
        print_help(&called_program, opts);
    }

    instanttee(matches.free[1..].to_vec(), matches.opt_present("a"));
}
