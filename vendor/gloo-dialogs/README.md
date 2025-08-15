<div align="center">

  <h1><code>gloo-dialogs</code></h1>

  <p>
    <a href="https://crates.io/crates/gloo-dialogs"><img src="https://img.shields.io/crates/v/gloo-dialogs.svg?style=flat-square" alt="Crates.io version" /></a>
    <a href="https://crates.io/crates/gloo-dialogs"><img src="https://img.shields.io/crates/d/gloo-dialogs.svg?style=flat-square" alt="Download" /></a>
    <a href="https://docs.rs/gloo-dialogs"><img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square" alt="docs.rs docs" /></a>
  </p>

  <h3>
    <a href="https://docs.rs/gloo-dialogs">API Docs</a>
    <span> | </span>
    <a href="https://github.com/rustwasm/gloo/blob/master/CONTRIBUTING.md">Contributing</a>
    <span> | </span>
    <a href="https://discordapp.com/channels/442252698964721669/443151097398296587">Chat</a>
  </h3>

  <sub>Built with 🦀🕸 by <a href="https://rustwasm.github.io/">The Rust and WebAssembly Working Group</a></sub>
</div>

This crate provides wrappers for the following functions.
- [`alert`](https://developer.mozilla.org/en-US/docs/Web/API/Window/alert)
- [`confirm`](https://developer.mozilla.org/en-US/docs/Web/API/Window/confirm)
- [`prompt`](https://developer.mozilla.org/en-US/docs/Web/API/Window/prompt)

`web-sys` provides a raw API which is hard to use. This crate provides an easy-to-use,
idiomatic Rust API for these functions.

See the [API documentation](https://docs.rs/gloo-dialogs) to learn more.
