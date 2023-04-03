# ractor_actors

<p align="center">
    <img src="https://raw.githubusercontent.com/slawlor/ractor/main/docs/ractor_logo.svg" width="50%" /> 
</p>

Common utility actors built with [Ractor](https://github.com/slawlor/ractor)

This crate contains some utility actors for Ractor-based systems. Additionally because `ractor` is built on `tokio`,
you can often intermingle these utility actors with non-actor async workflows.

**This crate is WIP**

## Installation

```toml

[dependencies]
ractor_actors = { version = "0.1", features = ["filewatcher"] }
```

## What's here?

The following utility actors are defined in this crate (enable with the associated feature in brackets):

1. Filewatcher (feature `filewatcher`) - Watch files and directories for changes. Built with `notify`.

## Contributors

To learn more about contributing to `ractor` please see [CONTRIBUTING.md](https://github.com/slawlor/ractor_actors/blob/main/CONTRIBUTING.md).

## License

This project is licensed under [MIT](https://github.com/slawlor/ractor_actors/blob/main/LICENSE).