# rxRust: a Rust implementation of Reactive Extensions
[![](https://docs.rs/rxrust/badge.svg)](https://docs.rs/rxrust/)
[![codecov](https://codecov.io/gh/rxRust/rxRust/branch/master/graph/badge.svg)](https://codecov.io/gh/rxRust/rxRust)
![](https://github.com/rxRust/rxRust/workflows/test/badge.svg)
[![](https://img.shields.io/crates/v/rxrust.svg)](https://crates.io/crates/rxrust)
[![](https://img.shields.io/crates/d/rxrust.svg)](https://crates.io/crates/rxrust)

## Usage

Add this to your Cargo.toml:

```toml
[dependencies]
rxrust = "1.0.0-beta.0"
```

## Example 

```rust
use rxrust:: prelude::*;

let mut numbers = observable::from_iter(0..10);
// create an even stream by filter
let even = numbers.clone().filter(|v| v % 2 == 0);
// create an odd stream by filter
let odd = numbers.clone().filter(|v| v % 2 != 0);

// merge odd and even stream again
even.merge(odd).subscribe(|v| print!("{} ", v, ));
// "0 2 4 6 8 1 3 5 7 9" will be printed.

```

## Clone Stream

In `rxrust` almost all extensions consume the upstream. So when you try to subscribe a stream twice, the compiler will complain. 

```rust ignore
 # use rxrust::prelude::*;
 let o = observable::from_iter(0..10);
 o.subscribe(|_| println!("consume in first"));
 o.subscribe(|_| println!("consume in second"));
```

In this case, we must clone the stream.

```rust
 # use rxrust::prelude::*;
 let o = observable::from_iter(0..10);
 o.clone().subscribe(|_| println!("consume in first"));
 o.clone().subscribe(|_| println!("consume in second"));
```

If you want to share the same observable, you can use `Subject`.

## Scheduler

`rxrust` use the runtime of the `Future` as the scheduler, `LocalPool` and `ThreadPool` in `futures::executor` can be used as schedulers directly, and `tokio::runtime::Runtime` is also supported, but need to enable the feature `futures-scheduler`. Across `Scheduler` to implement custom `Scheduler`.
Some Observable Ops (such as `delay`, and `debounce`) need the ability to delay, futures-time supports this ability when set with the `timer` feature, but you can also customize it by setting the new_timer function to NEW_TIMER_FN variant and removing the `timer` feature.
```rust 
use rxrust::prelude::*;

// `FuturesThreadPoolScheduler` is the alias of `futures::executor::ThreadPool`.
let threads_scheduler = FuturesThreadPoolScheduler::new().unwrap();

observable::from_iter(0..10)
  .subscribe_on(threads_scheduler.clone())
  .map(|v| v*2)
  .observe_on_threads(threads_scheduler)
  .subscribe(|v| println!("{},", v));
```

Also, `rxrust` supports WebAssembly by enabling the feature `wasm-scheduler` and using the crate `wasm-bindgen`. A simple example is [here](https://github.com/utilForever/rxrust-with-wasm). 

## Converts from a Future

Just use `observable::from_future` to convert a `Future` to an observable sequence.

```rust
use rxrust::prelude::*;

let mut scheduler_pool = FuturesLocalSchedulerPool::new();
observable::from_future(std::future::ready(1), scheduler_pool.spawner())
  .subscribe(move |v| println!("subscribed with {}", v));

// Wait `task` finish.
scheduler_pool.run();
```

A `from_future_result` function is also provided to propagate errors from `Future``.

## Missing Features List
See [missing features](missing_features.md) to know what rxRust does not have yet.

## All contributions are welcome

We are looking for contributors! Feel free to open issues for asking questions, suggesting features or other things!

Help and contributions can be any of the following:

- use the project and report issues to the project issues page
- documentation and README enhancement (VERY important)
- continuous improvement in a ci Pipeline
- implement any unimplemented operator, remember to create a pull request before you start your code, so other people know you are working on it.

you can enable the default timer by `timer` feature, or set a timer across function `new_timer_fn`