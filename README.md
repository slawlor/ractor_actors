# ractor_actors

<p align="center">
    <img src="https://raw.githubusercontent.com/slawlor/ractor/main/docs/ractor_logo.svg" width="50%" /> 
</p>

Common utility actors built with [Ractor](https://github.com/slawlor/ractor)

A pure-Rust actor framework. Inspired from [Erlang's `gen_server`](https://www.erlang.org/doc/man/gen_server.html), with the speed + performance of Rust!

* [<img alt="github" src="https://img.shields.io/badge/github-slawlor/ractor_actors-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/slawlor/ractor_actors)
* [<img alt="crates.io" src="https://img.shields.io/crates/v/ractor_actors.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/ractor_actors)
* [<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-ractor_actors-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/ractor_actors)
* [![CI/main](https://github.com/slawlor/ractor_actors/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/slawlor/ractor_actors/actions/workflows/ci.yaml)
* [![codecov](https://codecov.io/gh/slawlor/ractor_actors/branch/main/graph/badge.svg?token=61AGYYPWBA)](https://codecov.io/gh/slawlor/ractor_actors)
* `ractor_actors`: ![ractor_actor Downloads](https://img.shields.io/crates/d/ractor_actors.svg)

This crate contains some utility actors for Ractor-based systems. Additionally because `ractor` is built on `tokio`,
you can often intermingle these utility actors with non-actor async workflows.

**This crate is WIP**

## Installation

```toml
[dependencies]
ractor_actors = "0.1"
```

## What's here?

The following utility actors are defined in this crate (enable with the associated feature in brackets):

1. Filewatcher (feature `filewatcher`) - Watch files and directories for changes. Built with `notify`.
2. Tcp actors (feature `net`) - Listen for incoming connections and handle messages in/out from them as sessions.
3. Cron management actor (feature `time`) - A basic cron-job managing actor, which supports the full cron syntax and will execute operations on a period
4. Stream processing actors (feautre `streams`) - Actors for common tasks processing streams, including infinite/finite loops, stream processing, and stream multiplexing.

## Contributors

To learn more about contributing to `ractor` please see [CONTRIBUTING.md](https://github.com/slawlor/ractor_actors/blob/main/CONTRIBUTING.md).

## License

This project is licensed under [MIT](https://github.com/slawlor/ractor_actors/blob/main/LICENSE).
