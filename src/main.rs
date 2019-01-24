fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.first() {
        None => {
            eprintln!("You must specify at least one argument (the program to run).");
            std::process::exit(127);
        },
        Some(program) => {
            let args = &arguments[1..];
            let sentry = sentry_init(program, args);
            if sentry.is_enabled() {
                run_program(program, args);
            } else {
                eprintln!("Cannot enable Sentry integration.");
                std::process::exit(1);
            }
        },
    }
}

fn sentry_init(program: &str, args: &[String]) -> sentry::internals::ClientInitGuard {
    let dsn = std::env::var("SENTRY_DSN").expect("Env variable 'SENTRY_DSN' not set.");

    static DEFAULT_SENTRY_PROCESS_NAME: &str = "sentry-process";
    static DEFAULT_SENTRY_PROCESS_VERSION: &str = "???";

    let user_agent: std::borrow::Cow<'static, _> = match (option_env!("CARGO_PKG_NAME"), option_env!("CARGO_PKG_VERSION")) {
        (Some(name), Some(version)) => {
            let mut agent_string = String::new();
            agent_string.push_str(name);
            agent_string.push_str("@");
            agent_string.push_str(version);
            agent_string.into()
        },
        (Some(name), None) => name.into(),
        (None, _) => "sentry-process".into(),
    };

    let guard = sentry::init((dsn, sentry::ClientOptions {
        before_send: Some(std::sync::Arc::new(Box::new(|mut event | {
            let packages = event.sdk.clone().map(|old_sdk| old_sdk.packages.clone());
            let sdk: sentry::protocol::ClientSdkInfo = sentry::protocol::ClientSdkInfo {
                name: option_env!("CARGO_PKG_NAME").unwrap_or(DEFAULT_SENTRY_PROCESS_NAME).to_string(),
                version: option_env!("CARGO_PKG_VERSION").unwrap_or(DEFAULT_SENTRY_PROCESS_VERSION).to_string(),
                integrations: vec!(),
                packages: packages.unwrap_or_else(|| vec!()),
            };
            event.sdk.replace(std::borrow::Cow::Owned(sdk));
            Some(event)
        }))),
        user_agent,
        debug: false,
        shutdown_timeout: core::time::Duration::from_secs(10),
        attach_stacktrace: false,
        ..Default::default()
    }));

    sentry::configure_scope(|scope| {
        scope.set_tag("process", program);
        scope.set_tag("arguments", args.join(" "));
    });

    guard
}

fn run_program(program: &str, args: &[String]) {
    use std::process::{Command, Stdio};
    use std::io::{self, BufRead, BufReader, BufWriter, Write};
    use std::thread;

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
        },
        Ok(mut child) => {
            // Inspired from: https://andres.svbtle.com/convert-subprocess-stdout-stream-into-non-blocking-iterator-in-rust

            let stderr = child.stderr.take().unwrap();
            let stderr_thread = thread::spawn(move || {
                let reader = BufReader::new(stderr);
                let stderr = io::stderr();
                let stderr = stderr.lock();
                let mut stderr = BufWriter::new(stderr);
                let mut final_stderr = String::new();

                for line in reader.lines() {
                    let data = format!("{}\n", line.unwrap());
                    stderr.write_all(data.as_bytes()).unwrap();
                    stderr.flush().unwrap();
                    final_stderr.push_str(data.as_str());
                }

                final_stderr
            });

            let stdout = child.stdout.take().unwrap();
            let stdout_thread = thread::spawn(move || {
                let reader = BufReader::new(stdout);
                let stdout = io::stdout();
                let stdout = stdout.lock();
                let mut stdout = BufWriter::new(stdout);
                let mut final_stdout = String::new();

                for line in reader.lines() {
                    let data = format!("{}\n", line.unwrap());
                    stdout.write_all(data.as_bytes()).unwrap();
                    stdout.flush().unwrap();
                    final_stdout.push_str(data.as_str());
                }

                final_stdout
            });

            let result = child.wait_with_output();
            analyze_result(program, &result, stdout_thread.join().unwrap().as_str(), stderr_thread.join().unwrap().as_str());
        },
    }
}

fn analyze_result(program: &str, result: &Result<std::process::Output, std::io::Error>, stdout: &str, stderr: &str) {
    match result {
        Err(e) => {
            eprintln!("An error occurred: {}", e);
            std::process::exit(1);
        },
        Ok(output) => {
            if !output.status.success() {
                let hub = sentry::Hub::current();
                hub.configure_scope(|scope| {
                    scope.set_extra("stdout", stdout.to_owned().into());
                    scope.set_extra("stderr", stderr.to_owned().into());
                });
                hub.capture_message(format!("Process '{}' failed", program).as_str(), sentry::Level::Fatal);
                if !hub.client().unwrap().close(None) {
                    eprintln!("Could not send events to Sentry.");
                    std::process::exit(1);
                }
            }
            match output.status.code() {
                None => {
                    eprintln!("Process terminated by signal.");
                    std::process::exit(1);
                },
                Some(code) => {
                    std::process::exit(code);
                },
            }
        },
    }
}
