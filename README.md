# Simple webassembly plugin poc

Plugin-module is written in [AssemblyScript](https://www.assemblyscript.org) which is close to typescript.

The source code is in `assemblytest/assembly` and `npm run asbuild` will create the .wasm file as `assemblytest/build/optimized.wasm`.

The main rust program compiles the `assemblytest/build/optimized.wasm` file ahead-of-time and stores the compiled native code as `optimized.so`.  
This compile step is only needed any time the wasm file has changed and needs LLVM.  
So in real world the compile process would be done in some ci pipeline or during docker build within some build layer.

Afterwards the real plugin mechanism is only using the compiled `optimized.so` file and there we don't need any build step or LLVM any more.

This example has to host functions which are provided by the rust program to be used within the AssemblyScript webassembly plugin.

The webassembly part provides two function which can be called by the host system - `init` and `transform`.

The init function is demonstrates something like "Init the plugin with some config" and the transform function acts as example for something like "Compute somthing from that input and give me the result".

In `src/plugin/default.rs` this is abstracted, so that we only have `let my_plugin = plugin.init(&init_config)` and `let result = my_plugin.execute(&input_key, &input_payload)`

**FYI:**  
As AssemblyScript is using garbage collector the webassembly part provides a function `__collect` which invokes the garbage collector.  
See [AssemblyScript website](https://www.assemblyscript.org) for more information.  
In our sample we invoke `__collect` each time we call the `plugin.execute` function.

**FYI:**  
If we outsource the initial compile part we are also able to reduce the size of this executable dramatically

## How

Just Clone the repo and run `cargo run` and have fun playing around with assemblyscript, webassembly and rust.

## How does it work

In general webassembly does not provide some easy methods for strings or other more complex structures than single numbers/booleans.

Because of this the flow is:

- the webassembly provides some method to allocate memory of given size
- the webassembly provides access to "own" memory
- the host calls the webassembly function to allocate memory of size X
- webassembly function returns the start offset in (linear) memory
- the host copies his (string utf-8-bytes) into the webassembly memory
- the host calls the webassembly transform function with the memory offset as parameter
- the webassembly takes the memory with given offset and converts the byte-array back into string
- the webassembly is doing stuff with the string and is returning a new string.

...the returned new string is "some offset in linear memory"...  
So the whole thing is done in reverse...

The host copies from the webassembly memory starting at given offset ... transforms bytes to string...

...and after the host has the returned string value, the garbage collector is called.

Super duper easy - isn't it?

It feels a bit wired and not super save with this wild copy into memory stuff and I also expected some dramatic performance drop.

## Performance

Well, there was no exact measurement, but the loop in this example on Apple M1 Max was around +/- 1_000_000 iterations per sec.  
So not too bad, not to bad I would say.
