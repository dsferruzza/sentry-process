fn main() {
    if let Ok(sentry_dsn) = std::env::var("SENTRY_DSN") {
        let arguments = std::env::args().skip(1).collect::<Vec<_>>();
        match arguments.first() {
            None => {
                eprintln!("You must specify at least one argument (the program to run).");
                std::process::exit(127);
            }
            Some(program) => {
                let args = &arguments[1..];
                let sentry = sentry_init(sentry_dsn.as_str(), program, args);
                if sentry.is_enabled() {
                    run_program(program, args);
                } else {
                    eprintln!("Cannot enable Sentry integration.");
                    std::process::exit(1);
                }
            }
        }
    } else {
        eprintln!(
            "Environment variable 'SENTRY_DSN' must be set in order for Sentry Process to work."
        );
        std::process::exit(1);
    }
}

fn sentry_init(dsn: &str, program: &str, args: &[String]) -> sentry::ClientInitGuard {
    static DEFAULT_SENTRY_PROCESS_NAME: &str = "sentry-process";
    static DEFAULT_SENTRY_PROCESS_VERSION: &str = "???";

    let user_agent: std::borrow::Cow<'static, _> = match (
        option_env!("CARGO_PKG_NAME"),
        option_env!("CARGO_PKG_VERSION"),
    ) {
        (Some(name), Some(version)) => format!("{}@{}", name, version).into(),
        (Some(name), None) => name.into(),
        (None, _) => "sentry-process".into(),
    };

    let guard = sentry::init((
        dsn,
        sentry::ClientOptions {
            before_send: Some(std::sync::Arc::new(Box::new(
                |mut event: sentry::protocol::Event<'static>| {
                    let packages = event.sdk.clone().map(|old_sdk| old_sdk.packages.clone());
                    let sdk: sentry::protocol::ClientSdkInfo = sentry::protocol::ClientSdkInfo {
                        name: option_env!("CARGO_PKG_NAME")
                            .unwrap_or(DEFAULT_SENTRY_PROCESS_NAME)
                            .to_owned(),
                        version: option_env!("CARGO_PKG_VERSION")
                            .unwrap_or(DEFAULT_SENTRY_PROCESS_VERSION)
                            .to_owned(),
                        integrations: vec![],
                        packages: packages.unwrap_or_default(),
                    };
                    event.sdk.replace(std::borrow::Cow::Owned(sdk));
                    Some(event)
                },
            ))),
            user_agent,
            debug: false,
            shutdown_timeout: core::time::Duration::from_secs(10),
            attach_stacktrace: false,
            ..Default::default()
        },
    ));

    sentry::configure_scope(|scope| {
        scope.set_tag("process", program);
        if !args.is_empty() {
            scope.set_tag("arguments", args.join(" "));
        }
    });

    guard
}

fn run_program(program: &str, args: &[String]) {
    use rbl_circular_buffer::CircularBuffer;
    use std::io::{self, BufRead, BufReader, BufWriter, Write};
    use std::process::{Command, Stdio};
    use std::thread;

    #[cfg(windows)]
    const LINE_ENDING: &str = "\r\n";
    #[cfg(not(windows))]
    const LINE_ENDING: &str = "\n";

    const MAXIMUM_CHARACTERS: usize = 16_365;
    const REMOVED_MESSAGE: &str = "[previous content removed because of size limits]\n";
    const ACTUAL_MAXIMUM_CHARACTERS: usize = MAXIMUM_CHARACTERS - REMOVED_MESSAGE.as_bytes().len();

    let child_process = Command::new(program)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    match child_process {
        Err(e) => {
            eprintln!("Could not start the '{}' command: {}.", program, e);
            std::process::exit(127);
        }
        Ok(mut child) => {
            // Inspired from: https://andres.svbtle.com/convert-subprocess-stdout-stream-into-non-blocking-iterator-in-rust

            let stderr = child.stderr.take().unwrap();
            let stderr_thread = thread::spawn(move || {
                let reader = BufReader::new(stderr);
                let stderr = io::stderr();
                let stderr = stderr.lock();
                let mut stderr = BufWriter::new(stderr);

                let mut buffer_counter: usize = 0;
                let mut buffer = CircularBuffer::new(ACTUAL_MAXIMUM_CHARACTERS);

                for line in reader.lines() {
                    let data = format!("{}{}", line.unwrap(), LINE_ENDING);
                    stderr.write_all(data.as_bytes()).unwrap();
                    stderr.flush().unwrap();

                    for byte in data.as_bytes() {
                        let number_of_new_bytes = buffer.push(byte.to_owned());
                        if buffer_counter < ACTUAL_MAXIMUM_CHARACTERS {
                            buffer_counter += number_of_new_bytes;
                        }
                    }
                }

                let mut last_output_bytes = Vec::with_capacity(ACTUAL_MAXIMUM_CHARACTERS);
                buffer.fill(&mut last_output_bytes);
                format!(
                    "{}{}",
                    if buffer_counter > ACTUAL_MAXIMUM_CHARACTERS {
                        REMOVED_MESSAGE
                    } else {
                        ""
                    },
                    String::from_utf8_lossy(&last_output_bytes)
                )
            });

            let stdout = child.stdout.take().unwrap();
            let stdout_thread = thread::spawn(move || {
                let reader = BufReader::new(stdout);
                let stdout = io::stdout();
                let stdout = stdout.lock();
                let mut stdout = BufWriter::new(stdout);

                let mut buffer_counter: usize = 0;
                let mut buffer = CircularBuffer::new(ACTUAL_MAXIMUM_CHARACTERS);

                for line in reader.lines() {
                    let data = format!("{}{}", line.unwrap(), LINE_ENDING);
                    stdout.write_all(data.as_bytes()).unwrap();
                    stdout.flush().unwrap();

                    for byte in data.as_bytes() {
                        let number_of_new_bytes = buffer.push(byte.to_owned());
                        if buffer_counter < ACTUAL_MAXIMUM_CHARACTERS {
                            buffer_counter += number_of_new_bytes;
                        }
                    }
                }

                let mut last_output_bytes = Vec::with_capacity(ACTUAL_MAXIMUM_CHARACTERS);
                buffer.fill(&mut last_output_bytes);
                format!(
                    "{}{}",
                    if buffer_counter > ACTUAL_MAXIMUM_CHARACTERS {
                        REMOVED_MESSAGE
                    } else {
                        ""
                    },
                    String::from_utf8_lossy(&last_output_bytes)
                )
            });

            let result = child.wait_with_output();
            analyze_result(
                program,
                &result,
                stdout_thread.join().unwrap().as_str(),
                stderr_thread.join().unwrap().as_str(),
            );
        }
    }
}

fn analyze_result(
    program: &str,
    result: &Result<std::process::Output, std::io::Error>,
    stdout: &str,
    stderr: &str,
) {
    match result {
        Err(e) => {
            eprintln!("An error occurred: {}", e);
            std::process::exit(1);
        }
        Ok(output) => {
            if !output.status.success() {
                let hub = sentry::Hub::current();
                hub.configure_scope(|scope| {
                    scope.set_extra("stdout", stdout.to_owned().into());
                    scope.set_extra("stderr", stderr.to_owned().into());
                });
                hub.capture_message(
                    format!("Process '{}' failed", program).as_str(),
                    sentry::Level::Fatal,
                );
                if !hub.client().unwrap().close(None) {
                    eprintln!("Could not send events to Sentry.");
                    std::process::exit(1);
                }
            }
            match output.status.code() {
                None => {
                    eprintln!("Process terminated by signal.");
                    std::process::exit(1);
                }
                Some(code) => {
                    std::process::exit(code);
                }
            }
        }
    }
}
