use log::{debug, error, info};
use wasmer::{Instance, Module, NativeFunc};
use wasmer_wasi::{Pipe, WasiEnv, WasiState};

use crate::plugin::{helper_get_function, Plugin, PluginError, PluginOptions, WasmerStringPtr};

#[derive(Clone)]
pub struct DefaultPlugin {
  options: PluginOptions,
  instance: Instance,
  environment: WasiEnv,
  execute_fn: NativeFunc<(WasmerStringPtr, WasmerStringPtr), WasmerStringPtr>,
  malloc_fn: NativeFunc<u32, WasmerStringPtr>,
}

impl Plugin for DefaultPlugin {
  fn get_environment(&self) -> &WasiEnv {
    &self.environment
  }
  fn get_instance(&self) -> &Instance {
    &self.instance
  }
  fn get_malloc_fn(&self) -> &NativeFunc<u32, WasmerStringPtr> {
    &self.malloc_fn
  }
  fn get_options(&self) -> &PluginOptions {
    &self.options
  }

  fn create(options: PluginOptions) -> Result<Self, PluginError> {
    info!(
      "WASM:{} start create wasm plugin from \"{}\"",
      &options.module_name, options.file
    );

    debug!("WASM:{} loading module file", options.module_name);
    let module = unsafe {
      match Module::deserialize_from_file(&options.store, &options.file) {
        Ok(m) => {
          debug!("WASM:{} loading done", options.module_name);
          m
        }
        Err(error) => {
          error!("WASM:{} loading module failed", options.module_name);
          error!("{}", error);
          return Err(PluginError::LoadingError);
        }
      }
    };

    let wasi_env_create = WasiState::new(&options.module_name)
      .stdin(Box::new(Pipe::new()))
      .stdout(Box::new(Pipe::new()))
      .stderr(Box::new(Pipe::new()))
      .envs(options.envs.clone())
      .args(options.args.clone())
      .finalize();

    let mut environment = match wasi_env_create {
      Ok(env) => {
        debug!("WASM:{} wasi environment ok", options.module_name);
        env
      }
      Err(error) => {
        error!(
          "WASM:{} create wasi environment failed",
          options.module_name
        );
        error!("{}", error);
        return Err(PluginError::InitWasiEnvFailed);
      }
    };
    let mut import_object = match environment.import_object(&module) {
      Ok(o) => {
        debug!("WASM:{} wasi import object ok", options.module_name);
        o
      }
      Err(error) => {
        error!("WASM:{} wasi import object failed", options.module_name);
        error!("{}", error);
        return Err(PluginError::WasiImportObjectFailed);
      }
    };

    debug!("WASM:{} init custom environment", options.module_name);

    import_object.register("custom", options.custom_exports.clone());

    debug!("WASM:{} create new instance", options.module_name);
    let instance = match Instance::new(&module, &import_object) {
      Ok(i) => {
        debug!("WASM:{} instance created", options.module_name);
        i
      }
      Err(error) => {
        error!("WASM:{} create instance failed", options.module_name);
        error!("{}", error);
        return Err(PluginError::InstanceInitFailed);
      }
    };

    let execute_fn = helper_get_function::<(WasmerStringPtr, WasmerStringPtr), WasmerStringPtr>(
      &instance,
      &options,
      &options.execute_function_name,
    )?;

    let malloc_fn = helper_get_function::<u32, WasmerStringPtr>(
      &instance,
      &options,
      &options.allocate_utf8array_function_name,
    )?;

    Ok(Self {
      options,
      instance,
      environment,
      execute_fn,
      malloc_fn,
    })
  }
}

impl DefaultPlugin {
  pub fn execute(&self, key: &String, payload: &String) -> Result<String, PluginError> {
    let key_ptr = self.allocate_string(key);
    let payload_ptr = self.allocate_string(payload);

    let result = match self.execute_fn.call(key_ptr, payload_ptr) {
      Ok(result_ptr) => {
        match self.read_from_stdout() {
          Some(out) => info!(
            "WASM:{}:{} \"{}\"",
            self.options.module_name, self.options.execute_function_name, out
          ),
          None => (),
        };
        Ok(self.get_string(result_ptr))
      }
      Err(error) => Err(self.log_and_transform_error(error, &self.options.execute_function_name)),
    };

    self.call_garbage_collector()?;

    return result;
  }

  fn call_garbage_collector(&self) -> Result<(), PluginError> {
    let garbage_collector = self.get_function::<(), ()>(&String::from("__collect"))?;

    match garbage_collector.call() {
      Ok(_result) => Ok(()),
      Err(error) => Err(self.log_and_transform_error(error, &String::from("__collect"))),
    }
  }
}
