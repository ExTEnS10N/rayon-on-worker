/*
* Copyright 2022 Google Inc. All Rights Reserved.
* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at
*     http://www.apache.org/licenses/LICENSE-2.0
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

#![doc = include_str!("../README.md")]

// Note: `atomics` is whitelisted in `target_feature` detection, but `bulk-memory` isn't,
// so we can check only presence of the former. This should be enough to catch most common
// mistake (forgetting to pass `RUSTFLAGS` altogether).
#[cfg(all(not(doc), not(target_feature = "atomics")))]
compile_error!("Did you forget to enable `atomics` and `bulk-memory` features as outlined in rayon-on-worker README?");

use crossbeam_channel::RecvError;
use crossbeam_channel::{bounded, Receiver, Sender};
use rayon::{ThreadBuilder, ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
#[cfg(feature = "console-panic")]
extern crate console_error_panic_hook;
#[cfg(feature = "console-panic")]
use std::panic;

#[cfg_attr(
    not(feature = "no-bundler"),
    wasm_bindgen(module = "/src/workerHelpers.js")
)]
#[cfg_attr(
    feature = "no-bundler",
    wasm_bindgen(module = "/src/workerHelpers.no-bundler.js")
)]
extern "C" {
    #[wasm_bindgen(js_name = startWorkers)]
    async fn start_workers(
        module: JsValue, memory: JsValue,
        url: &str, num_workers: usize,
        receiver: *const Receiver<ThreadBuilder>,
        is_global: bool,
        share_object: &JsValue, obj_senders: &js_sys::Array
    ) -> JsValue;

    #[wasm_bindgen(js_name = startWorkers)]
    async fn start_global_workers(
        module: JsValue, memory: JsValue,
        url: &str, num_workers: usize,
        receiver: *const Receiver<ThreadBuilder>,
        is_global: bool,
    ) -> JsValue;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(thread_local, js_namespace = ["import", "meta"], js_name = url)]
    static MAIN_JS_URL: String;
}

#[cfg(not(feature = "no-bundler"))]
fn _ensure_worker_emitted() {
    // Just ensure that the worker is emitted into the output folder, but don't actually use the URL.
    wasm_bindgen::link_to!(module = "/src/workerHelpers.worker.js");
}

pub struct WebWorkersBuilder {
    object: JsValue,
    num_workers: usize,
    sender: Option<Sender<ThreadBuilder>>,
    receiver: Option<Receiver<ThreadBuilder>>,
    obj_senders: Vec<Sender<JsValue>>,
    obj_receivers: Vec<WasmReceiver>
}

impl WebWorkersBuilder {
    pub fn new() -> Self {
        WebWorkersBuilder { 
            object: JsValue::undefined(), num_workers: 1,
            sender: None, receiver: None,
            obj_senders: vec![], obj_receivers: vec![]
        }
    }

    /// set how many workers to build
    /// ### Panic
    /// ```
    /// assert_ne!(num_workers, 0);
    /// ```
    pub fn num_workers(mut self, num_workers: usize) -> Self {
        assert_ne!(num_workers, 0);
        self.num_workers = num_workers;
        self
    }

    pub fn share(mut self, object: JsValue) -> Self{
        self.object = object;
        self
    }

    pub fn receivers(&mut self) -> &mut Vec<WasmReceiver> {
        &mut self.obj_receivers
    }

    pub async fn build(&mut self) -> JsValue {
        let (sender, receiver) = bounded(self.num_workers);
        self.sender = Some(sender);
        self.receiver = Some(receiver);

        let array = js_sys::Array::new();
        for _ in 0..self.num_workers {
            let (obj_sender, obj_receiver) = bounded(1);
            let obj_sender_addr: *const Sender<JsValue> = &obj_sender;
            array.push(&(obj_sender_addr.into()));
            self.obj_senders.push(obj_sender);
            self.obj_receivers.push(WasmReceiver::new(obj_receiver));
        }
        start_workers(
            wasm_bindgen::module(), wasm_bindgen::memory(),
            &MAIN_JS_URL.with(|s| s.clone()), self.num_workers,
            self.receiver.as_ref().unwrap(),
            false,
            &self.object, &array
        ).await
    }

    pub async fn build_global(&mut self) -> JsValue {
        let (sender, receiver) = bounded(self.num_workers);
        self.sender = Some(sender);
        self.receiver = Some(receiver);
        start_global_workers(
            wasm_bindgen::module(), wasm_bindgen::memory(),
            &MAIN_JS_URL.with(|s| s.clone()), self.num_workers,
            self.receiver.as_ref().unwrap(),
            true
        ).await
    }
}

pub struct WasmReceiver {
    receiver: Receiver<JsValue>
}
impl WasmReceiver {
    fn new(receiver: Receiver<JsValue>) -> Self {
        WasmReceiver { receiver }
    }

    pub fn recv(&self) -> Result<JsValue, RecvError> {
        self.receiver.recv()
    }
}
unsafe impl Send for WasmReceiver {}

pub trait WebPoolBuildable {
    fn build_on_workers(self, builder: &mut WebWorkersBuilder) -> Result<ThreadPool, ThreadPoolBuildError>;
    fn build_on_workers_global(self, builder: &mut WebWorkersBuilder) -> Result<(), ThreadPoolBuildError>;
}

impl WebPoolBuildable for ThreadPoolBuilder {
    fn build_on_workers(self, builder: &mut WebWorkersBuilder) -> Result<ThreadPool, ThreadPoolBuildError> {
        self.num_threads(builder.num_workers).spawn_handler(move |thread| {
            builder.sender.as_ref().unwrap().send(thread).unwrap_throw();
            Ok(())
        }).build()
    }

    fn build_on_workers_global(self, builder: &mut WebWorkersBuilder) -> Result<(), ThreadPoolBuildError>{
        self.num_threads(builder.num_workers).spawn_handler(move |thread| {
            builder.sender.as_ref().unwrap().send(thread).unwrap_throw();
            Ok(())
        }).build_global()
    }
}

#[wasm_bindgen]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[doc(hidden)]
pub fn wbg_rayon_start_global_worker(
    receiver: *const Receiver<ThreadBuilder>
)
where
    // Statically assert that it's safe to accept `Receiver` from another thread.
    Receiver<ThreadBuilder>: Sync,
{
    #[cfg(feature = "console-panic")]
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    // This is safe, because we know it came from a reference to PoolBuilder,
    // allocated on the heap by wasm-bindgen and dropped only once all the
    // threads are running.
    //
    // The only way to violate safety is if someone externally calls
    // `exports.wbg_rayon_start_worker(garbageValue)`, but then no Rust tools
    // would prevent us from issues anyway.
    let receiver = unsafe { &*receiver };
    // Wait for a task (`ThreadBuilder`) on the channel, and, once received,
    // start executing it.
    //
    // On practice this will start running Rayon's internal event loop.
    receiver.recv().unwrap_throw().run()
}

#[wasm_bindgen]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[doc(hidden)]
pub fn wbg_rayon_start_worker(
    receiver: *const Receiver<ThreadBuilder>,
    object: JsValue, obj_sender: *const Sender<JsValue>
)
where Receiver<ThreadBuilder>: Sync,
{
    #[cfg(feature = "console-panic")]
    panic::set_hook(Box::new(console_error_panic_hook::hook));

    let obj_sender = unsafe { &*obj_sender };
    obj_sender.send(object).unwrap();
    
    let receiver = unsafe { &*receiver };
    receiver.recv().unwrap_throw().run()
}
