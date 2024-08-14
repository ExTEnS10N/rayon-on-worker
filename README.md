# Why the fork?
In origin crate wasm-bindgen-rayon, once you call init_thread_pool(), the rayon thread pool will be built globally, without any customization allowed,
I can not postMessage to those workers since the rayon run loop will never stop and the global pool will nerver drop. 

Whenever I need to send some JS Object across thread, it is just impossible, since those objects can only be sent via postMessage and not SharedArrayBuffer.

Using the origin wasm-bindgen-rayon will totally limits the whole application function to some specific scenes that we can only do cpu calculations, and other types of multithread tasks will become impossible to run.

So here goes the fork, which aims to let me use the rayon as flexible as it originally is.

# Statements

- Due to personally lack of time, I won't make any pull request, nor publish to crates.io.

- **As long as it works for me, everything is ok!**

# Overview

`rayon-on-worker` is an adapter for enabling [Rayon](https://github.com/rayon-rs/rayon)-based concurrency on the Web with WebAssembly (via [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen), Web Workers and SharedArrayBuffer support).

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
<!-- param::isNotitle::true:: -->

- [Usage](#usage)
  - [Setting up](#setting-up)
  - [Using Rayon](#using-rayon)
  - [Send Special JsValue Across threads](#send-special-jsvalue-across-threads)
  - [Building Rust code](#building-rust-code)
    - [Using config files](#using-config-files)
    - [Using command-line params](#using-command-line-params)
  - [Usage with various bundlers](#usage-with-various-bundlers)
    - [Usage with Webpack](#usage-with-webpack)
    - [Usage with Parcel](#usage-with-parcel)
    - [Usage with Rollup](#usage-with-rollup)
    - [Usage without bundlers](#usage-without-bundlers)
  - [Feature detection](#feature-detection)
- [License](#license)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Usage

WebAssembly thread support is not yet a first-class citizen in Rust - it's still only available in nightly - so there are a few things to keep in mind when using this crate.

<table width="100%">
  <tr>
  <td width="50%">

![Drawn using a single thread: 273ms](https://github.com/RReverser/wasm-bindgen-rayon/assets/557590/665cb157-8734-460d-8a0a-a67370e00cb7)
    
  </td>
  <td width="50%">

![Drawn using all available threads via rayon-on-worker: 87ms](https://github.com/RReverser/wasm-bindgen-rayon/assets/557590/db32a88a-0e77-4974-94fc-1b993030ca92)
    
  </td>
  </tr>
</table>

## Setting up

In order to use `SharedArrayBuffer` on the Web, you need to enable [cross-origin isolation policies](https://web.dev/coop-coep/). Check out the linked article for details.

Then, add `wasm-bindgen`, `rayon`, and this crate as dependencies to your `Cargo.toml`:

```toml
[dependencies]
wasm-bindgen = "0.2"
rayon = "1.8"
wasm-bindgen-futures = "0.4.42"
rayon-on-worker = { path = "path/to/rayon-on-worker" }
```

Then, reexport the `init_thread_pool` function:

```rust
let mut workers_builder = WebWorkersBuilder::new()
  .num_workers(16);
let _workers = workers_builder.build().await;
let pool = ThreadPoolBuilder::new()
    .build_on_workers(&mut workers_builder).unwrap_throw();
```

## Using Rayon

Use [Rayon](https://github.com/rayon-rs/rayon) iterators as you normally would, e.g.

```rust
#[wasm_bindgen]
pub fn sum(numbers: &[i32]) -> i32 {
    numbers.par_iter().sum()
}
```

will accept an `Int32Array` from JavaScript side and calculate the sum of its values using all available threads.

## Send Special JsValue Across threads
Some Javascript objects are impossible be sent across thread through SharedArrayBuffer, for example, a File selected by user:

```rust
let file: web_sys::File = user_select().await;
let num_workers = 16;
let mut workers_builder = WebWorkersBuilder::new()
  .num_workers(num_workers)
  .share(file.slice().into());
let _workers = workers_builder.build().await;
let pool = ThreadPoolBuilder::new()
    .build_on_workers(&mut workers_builder).unwrap_throw();

let receivers = workers_builder.object_receivers();
for _ in 0..num_workers{
  let receiver = receivers.pop().unwrap();
  pool.spawn(move || {
    let file: JsValue = receiver.recv().unwrap();
    // Now do whatever you need to multihreading processing the file.
  }
}

// You need to make sure your pool isn't dropped
// before all jobs done yourself. 
```

## Building Rust code

First limitation to note is that you'll have to use `wasm-bindgen`/`wasm-pack`'s `web` target (`--target web`).

<details>
<summary><i>Why?</i></summary>

This is because the Wasm code needs to take its own object (the `WebAssembly.Module`) and share it with other threads when spawning them. This object is only accessible from the `--target web` and `--target no-modules` outputs, but we further restrict it to only `--target web` as we also use [JS snippets feature](https://rustwasm.github.io/wasm-bindgen/reference/js-snippets.html).

</details>

The other issue is that the Rust standard library for the WebAssembly target is built without threads support to ensure maximum portability.

Since we want standard library to be thread-safe and [`std::sync`](https://doc.rust-lang.org/std/sync/) APIs to work, you'll need to use the nightly compiler toolchain and pass some flags to rebuild the standard library in addition to your own code.

In order to reduce risk of breakages, it's strongly recommended to use a fixed nightly version. For example, the latest stable Rust at the moment of writing is version 1.66, which corresponds to `nightly-2022-12-12`, which was tested and works with this crate.

### Using config files

The easiest way to configure those flags is:

1. Put a string `nightly-2022-12-12` in a `rust-toolchain` file in your project directory. This tells Rustup to use nightly toolchain by default for your project.
2. Put the following in a `.cargo/config.toml` file in your project directory:

   ```toml
   [target.wasm32-unknown-unknown]
   rustflags = ["-C", "target-feature=+atomics,+bulk-memory,+mutable-globals"]

   [unstable]
   build-std = ["panic_abort", "std"]
   ```

   This tells Cargo to rebuild the standard library with support for Wasm atomics.

Then, run [`wasm-pack`](https://rustwasm.github.io/wasm-pack/book/) as you normally would with `--target web`:

```sh
wasm-pack build --target web [...normal wasm-pack params...]
```

### Using command-line params

If you prefer not to configure those parameters by default, you can pass them as part of the build command itself.

In that case, the whole command looks like this:

```sh
RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+mutable-globals' \
  rustup run nightly-2022-12-12 \
  wasm-pack build --target web [...] \
  -- -Z build-std=panic_abort,std
```

It looks a bit scary, but it takes care of everything - choosing the nightly toolchain, enabling the required features as well as telling Cargo to rebuild the standard library. You only need to copy it once and hopefully forget about it :)

## Usage with various bundlers

WebAssembly threads use Web Workers under the hood for instantiating other threads with the same WebAssembly module & memory.

rayon-on-worker provides the required JS code for those Workers internally, and [uses a syntax that is recognised across various bundlers](https://web.dev/bundling-non-js-resources/).

### Usage with Webpack

If you're using Webpack v5 (version >= 5.25.1), you don't need to do anything special, as it already supports [bundling Workers](https://webpack.js.org/guides/web-workers/) out of the box.

### Usage with Parcel

Parcel v2 also recognises the used syntax and works out of the box.

### Usage with Rollup

For Rollup, you'll need [`@surma/rollup-plugin-off-main-thread`](https://github.com/surma/rollup-plugin-off-main-thread) plugin (version >= 2.1.0) which brings the same functionality and was tested with this crate.

Alternatively, you can use [Vite](https://vitejs.dev/) which has necessary plugins built-in.

### Usage without bundlers

The default JS glue was designed in a way that works great with bundlers and code-splitting, but, sadly, not in browsers due to different treatment of import paths (see [`WICG/import-maps#244`](https://github.com/WICG/import-maps/issues/244)).

If you want to build this library for usage without bundlers, enable the `no-bundler` feature for `rayon-on-worker` in your `Cargo.toml`:

```toml
rayon-on-worker = { path = "path/to/rayon-on-worker", features = ["no-bundler"] }
```

## Feature detection

If you're targeting [older browser versions that didn't support WebAssembly threads yet](https://webassembly.org/roadmap/), you'll likely want to make two builds - one with threads support and one without - and use feature detection to choose the right one on the JavaScript side.

You can use [`wasm-feature-detect`](https://github.com/GoogleChromeLabs/wasm-feature-detect) library for this purpose. The code will look roughly like this:

```js
import { threads } from 'wasm-feature-detect';

let wasmPkg;

if (await threads()) {
  wasmPkg = await import('./pkg-with-threads/index.js');
  await wasmPkg.default();
  await wasmPkg.initThreadPool(navigator.hardwareConcurrency);
} else {
  wasmPkg = await import('./pkg-without-threads/index.js');
  await wasmPkg.default();
}

wasmPkg.nowCallAnyExportedFuncs();
```

# License

As a forked repository, using everything from the origin repo must follow their own license (The Apache License 2.0), as included in file named LICENSE;

Codes that are writen by me (ExTEnS10N), are BSD-0-Clause licensed, and fallbacks to the license of origin repo if applicable.