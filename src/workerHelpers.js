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


export async function startWorkers(module, memory, length, receiver, shareObject, objSenders) {

  const workerInit = {
    module,
    memory,
    receiver,
    shareObject,
  };

  const workers = await Promise.all(
    Array.from({ length }, async () => {
      // Self-spawn into a new Worker.
      //
      // TODO: while `new URL('...', import.meta.url) becomes a semi-standard
      // way to get asset URLs relative to the module across various bundlers
      // and browser, ideally we should switch to `import.meta.resolve`
      // once it becomes a standard.
      const worker = new Worker(
        new URL('./workerHelpers.worker.js', import.meta.url),
        {
          type: 'module'
        }
      );
      workerInit["objSender"] = objSenders[i];
      worker.postMessage(workerInit);
      await new Promise(resolve =>
        worker.addEventListener('message', resolve, { once: true })
      );
      return worker;
    })
  );

  return workers;
}

export const wasmAddr = (addr) => {
  return addr
}