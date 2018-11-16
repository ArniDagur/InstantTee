extern crate getopts;
extern crate nix;
extern crate rayon;
extern crate hurdles;

use std::convert;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{stdin, stdout};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::process;
use std::thread;
use std::sync::Arc;

use getopts::Options;
use nix::fcntl::{splice, tee, SpliceFFlags};
use nix::unistd::pipe;
use rayon::scope;
use hurdles::Barrier;

const BUF_SIZE: usize = 1024 * 64;

#[derive(Debug)]
struct FilePipePair {
    offset: i64,
    file: File,
    pipe_rd: RawFd,
    pipe_wr: RawFd,
}
impl convert::From<File> for FilePipePair {
    fn from(file: File) -> Self {
        let (pipe_rd, pipe_wr) = pipe().unwrap();
        FilePipePair {
            offset: file.metadata().unwrap().len() as i64,
            file,
            pipe_rd,
            pipe_wr,
        }
    }
}
impl FilePipePair {
    fn write_from_pipe(&mut self, fd_in: RawFd) -> usize {
        let bytes_teed = tee(
            fd_in,
            self.pipe_wr,
            BUF_SIZE,
            SpliceFFlags::empty(),
        ).unwrap_or_else(|err| {
            eprintln!("Error when attempting to tee stdin to pipe: {}", err);
            process::exit(1);
        });
        if bytes_teed == 0 {
            // If we only tee 0 bytes, we're bound to only splice 0 bytes too.
            // This check also helps because splice() will block if we attempt
            // to splice 0 bytes, as self.pipe_wr has not been dropped.
            return 0;
        }
        let bytes_spliced = splice(
            self.pipe_rd,
            None,
            self.file.as_raw_fd(),
            Some(&mut self.offset),
            BUF_SIZE,
            SpliceFFlags::empty(),
        ).unwrap_or_else(|err| {
            eprintln!("Error when attempting to splice to file: {}", err);
            process::exit(1);
        });
        return bytes_spliced
    }
}

fn splice_all(fd_in: RawFd, fd_out: RawFd) {
    loop {
        let bytes_spliced = splice(
            fd_in,
            None,
            fd_out,
            None,
            BUF_SIZE,
            SpliceFFlags::empty(),
        ).unwrap_or_else(|err| {
            eprintln!("Error when attempting to splice stdin to pipe: {}", err);
            process::exit(1);
        });

        if bytes_spliced == 0 {
            // We've spliced everything; return from function.
            break;
        }
    }
}

fn instanttee(files: Vec<String>, append: bool) {
    let num_files = files.len();
    // We create a main pipe
    let (main_pipe_rd, main_pipe_wr) = pipe().unwrap();
    let main_pipe_rd = unsafe { File::from_raw_fd(main_pipe_rd) };
    let main_pipe_wr = unsafe { File::from_raw_fd(main_pipe_wr) };
    let main_pipe_rd = Arc::new(main_pipe_rd);

    let mut fpps: Vec<FilePipePair> = Vec::with_capacity(num_files);
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
                }),
        ));
    }

    // Input thread
    thread::spawn(move || {
        let stdin = stdin();
        let _handle = stdin.lock();
        splice_all(stdin.as_raw_fd(), main_pipe_wr.as_raw_fd());
        drop(main_pipe_wr);
    });

    let barrier = Barrier::new(num_files + 1); // We have one worker for each file,
                                               // and another for stdout.
    scope(move |scope| {
        for mut fpp in fpps {
            let mut barrier = barrier.clone();
            let main_pipe_rd = main_pipe_rd.clone();
            scope.spawn(move |_| {
                loop {
                    let res = fpp.write_from_pipe(main_pipe_rd.as_raw_fd());
                    if res == 0 {
                        // We wrote 0 bytes to fpp, which means that we've done
                        // our job successfully.
                        process::exit(0);
                    }
                    barrier.wait();
                }
            });
        }
        let mut barrier = barrier.clone();
        let main_pipe_rd = main_pipe_rd.clone();
        scope.spawn(move |_| {
            let stdout = stdout();
            let _handle = stdout.lock();
            loop {
                barrier.wait();
                splice(
                    main_pipe_rd.as_raw_fd(),
                    None,
                    stdout.as_raw_fd(),
                    None,
                    BUF_SIZE,
                    SpliceFFlags::empty()
                ).unwrap_or_else(|err| {
                    eprintln!("Error when attempting to splice to stdout: {}", err);
                    process::exit(1);
                });
            }
        });
    });
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
    opts.optflag("a", "append", "append to the given FILES; do not overwrite");

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
