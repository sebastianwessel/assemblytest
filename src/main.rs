use log::{debug, error, info};
use wasmer::*;

mod plugin;

use plugin::default::DefaultPlugin;
use plugin::{Plugin, PluginOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let log_modules = format!(
    "{}{}",
    "trace, wasmer_wasi::syscalls=off, wasmer_wasi::state=off", ""
  );
  let logger = flexi_logger::Logger::try_with_env_or_str(log_modules)
    .unwrap()
    .format(flexi_logger::colored_detailed_format)
    .duplicate_to_stderr(flexi_logger::Duplicate::All)
    .print_message();
  logger.start().unwrap();

  // we use ahead-of-time compile .wasm to .so
  // in real world compile should be done only when wasm has changed
  // eg in build pipeline, on docker compose ....
  {
    let compiler_exp = LLVM::new();
    let engine_exp = Universal::new(compiler_exp).engine();
    let store_exp = Store::new(&engine_exp);

    debug!("Compiling module");
    let module_exp = Module::from_file(&store_exp, "./assemblytest/build/optimized.wasm").unwrap();

    debug!("serialize compiled module to file");
    module_exp.serialize_to_file("./optimized.so").unwrap();
  };

  // two simple host function we will call in our webassembly plugin
  fn tests(i: i32) -> i32 {
    info!("host function called from wasm with param {}", i);
    i + 1
  }

  fn tests2(i: i64) -> i64 {
    info!("host function2 called from wasm with param {}", i);
    i + 2
  }

  // load compiled webassembly .so file
  let plugin_name = String::from("test_plugin");
  let plugin_file_name = String::from("./optimized.so");
  let plugin_function_name = String::from("transform");

  let mut options = PluginOptions::new(&plugin_name, &plugin_file_name, &plugin_function_name);
  // Register host functions which are available in guest wasm here
  options.add_host_function("tests".into(), tests);
  options.add_host_function("tests2".into(), tests2);

  let plugin = match DefaultPlugin::create(options) {
    Ok(p) => p,
    Err(_error) => panic!("WASM:{} fatal error", &plugin_name),
  };

  // call function of webassembly plugin with string parameter
  let init_config = String::from("Some input we have");
  match plugin.init(&init_config) {
    Ok(()) => (),
    Err(_error) => panic!("WASM:{} fatal error", &plugin_name),
  }

  // we do some simple performance check
  // we have a loop which calls a webassembly function with string input and string output
  info!("start");
  for x in 0..1_00 {
    let input_key = format!("/some/test/{}", x);
    let input_payload = format!("{{\"temperature\": {} }}", x);
    match plugin.execute(&input_key, &input_payload) {
      Ok(result) => {
        assert_eq!(
          result,
          format!(
            "transform: /some/test/{} for payload {{\"temperature\": {} }}",
            x, x
          )
        );
      }
      Err(error) => {
        error!("{:?}", error);
        panic!("ups")
      }
    }
  }
  info!("done");
  Ok(())
}
