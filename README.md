# Sentry Process

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![pipeline status](https://gitlab.com/dsferruzza/sentry-process/badges/master/pipeline.svg)](https://gitlab.com/dsferruzza/sentry-process/commits/master)
[![Crates.io Version](https://img.shields.io/crates/v/sentry-process.svg)](https://crates.io/crates/sentry-process)
[![Minimum rustc version](https://img.shields.io/badge/rustc-1.40+-lightgray.svg)](#rust-version-requirements)
[![Get help on Codementor](https://cdn.codementor.io/badges/get_help_github.svg)](https://www.codementor.io/dsferruzza?utm_source=github&utm_medium=button&utm_term=dsferruzza&utm_campaign=github)

Report failed processes/scripts to Sentry.

⚠️ _Main repository is here: https://gitlab.com/dsferruzza/sentry-process_ ⚠️

## Motivation

[Sentry](https://sentry.io) is a cool system that helps developers to aggregate, monitor and fix errors/exceptions in their own apps. It has integrations for many technologies so that reported errors can contain relevant informations.

But sometimes it can be desirable to plug Sentry to an arbitrary process. Such process can be a binary that was built without Sentry support, or a Bash script that runs every night on a server, for example.

With **Sentry Process** it is now easy to launch a process that will send an event to a Sentry instance whenever it fails!

## How to use

- compile or install the project
- set the `SENTRY_DSN` environment variable (use the URL given in `Your project > Settings > Client Keys (DSN)` in Sentry)
- run `sentry-process COMMAND [ARGUMENTS]...` _(wrap your process call like you would do with `sudo`)_

If running `COMMAND [ARGUMENTS]...` results in an exit code that is not `0`, Sentry Process considers it as a failure and reports. For example, `sentry-process false` will report whereas `sentry-process true` will not.

The exit code, standard output and standard error of `COMMAND [ARGUMENTS]...` are re-emmited by `sentry-process COMMAND [ARGUMENTS]...`, so that it can be piped or used by other programs. In fact, standard output and standard error are streamed so you can run (for example) `sentry-process wget https://somebigfileurl` and see the progress just like if you had run `wget https://somebigfileurl`.

## Rust Version Requirements

Sentry Process needs Rust 1.40+ to be compiled.

## Limitations

- The process you want to run must terminate eventually. It may not make sense to use Sentry Process with processes that are designed to never die (like servers).
- You will **not** get much details on _what_ failed (i.e. no stacktraces). This is the tradeoff here.
- Sentry Process will send to your Sentry instance both standard output (`stdout`) and standard error (`stderr`) of the process you want to run (if it fails). Your Sentry instance might reject the report if this data is more than 200 kB (see [Sentry's documentation](https://docs.sentry.io/platforms/rust/enriching-events/context/)), so Sentry Process uses a circular buffer to ensure that this limit cannot be reached and the most recent outputs are sent with the report.

## License

MIT License Copyright (c) 2019-2020 David Sferruzza
