pub mod default;

use wasmer::{
  Array, Exports, Function, HostFunction, Instance, Memory, NativeFunc, RuntimeError, Store,
  Universal, WasmPtr, WasmTypeList,
};
use wasmer_wasi::WasiEnv;

use log::{error, info};

pub type WasmerStringPtr = WasmPtr<u8, Array>;

#[derive(Debug, Clone)]
pub struct PluginOptions {
  store: Store,
  module_name: String,
  file: String,
  envs: Vec<(String, String)>,
  args: Vec<String>,
  start_function_name: String,
  init_function_name: String,
  allocate_utf8array_function_name: String,
  execute_function_name: String,
  memory_name: String,
  custom_exports: Exports,
}

impl PluginOptions {
  pub fn new(module_name: &String, file: &String, execute_function_name: &String) -> Self {
    let engine = Universal::headless().engine();
    let store = Store::new(&engine);
    let custom_exports = Exports::new();

    let start_function_name = String::from("_start");
    let init_function_name = String::from("init");
    let allocate_utf8array_function_name = String::from("malloc");
    let memory_name = String::from("memory");
    Self {
      store,
      custom_exports,
      module_name: module_name.clone(),
      file: file.clone(),
      envs: vec![],
      args: vec![],
      start_function_name,
      init_function_name,
      allocate_utf8array_function_name,
      execute_function_name: execute_function_name.clone(),
      memory_name,
    }
  }

  pub fn add_host_function<
    F: HostFunction<Args, Rets, wasmer::internals::WithoutEnv, Env>,
    Args: WasmTypeList,
    Rets: WasmTypeList,
    Env: Sized + 'static,
  >(
    &mut self,
    name: String,
    value: F,
  ) -> &mut Self {
    let c = Function::new_native(&self.store, value);
    self.custom_exports.insert(name.clone(), c);
    self
  }

  pub fn set_start_function_name(&mut self, name: &String) -> &mut Self {
    self.start_function_name = name.clone();
    self
  }

  pub fn set_init_function_name(&mut self, name: &String) -> &mut Self {
    self.init_function_name = name.clone();
    self
  }

  pub fn set_allocate_utf8array_function_name(&mut self, name: &String) -> &mut Self {
    self.allocate_utf8array_function_name = name.clone();
    self
  }

  pub fn set_memory_name(&mut self, name: &String) -> &mut Self {
    self.memory_name = name.clone();
    self
  }

  pub fn add_env(&mut self, key: &String, value: &String) -> &mut Self {
    self.envs.push((key.clone(), value.clone()));
    self
  }

  pub fn add_arg(&mut self, arg: &String) -> &mut Self {
    self.args.push(arg.clone());
    self
  }
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum PluginError {
  LoadingError,
  InitWasiEnvFailed,
  InstanceInitFailed,
  WasiImportObjectFailed,
  RuntimeError,
  FunctionNotFound,
  FunctionInvalidParameter,
}

pub fn helper_get_function<T: WasmTypeList, O: WasmTypeList>(
  instance: &Instance,
  options: &PluginOptions,
  name: &String,
) -> Result<NativeFunc<T, O>, PluginError> {
  match instance.exports.get_function(name) {
    Ok(result) => match result.native::<T, O>() {
      Ok(f) => Ok(f),
      Err(error) => {
        error!("WASM:{}:{} parameter missmatch", options.module_name, name,);
        error!("{:?}", error);
        return Err(PluginError::FunctionInvalidParameter);
      }
    },
    Err(error) => {
      error!(
        "WASM:{}:{} getting function failed",
        options.module_name, name,
      );
      error!("{:?}", error);
      return Err(PluginError::FunctionNotFound);
    }
  }
}

pub trait Plugin {
  fn create(options: PluginOptions) -> Result<Self, PluginError>
  where
    Self: Sized;

  fn get_environment(&self) -> &WasiEnv;
  fn get_instance(&self) -> &Instance;

  fn get_malloc_fn(&self) -> &NativeFunc<u32, WasmerStringPtr>;
  fn get_options(&self) -> &PluginOptions;

  fn get_memory(&self) -> &Memory {
    self
      .get_instance()
      .exports
      .get_memory(&self.get_options().memory_name)
      .unwrap()
  }

  fn get_function<T: WasmTypeList, O: WasmTypeList>(
    &self,
    name: &String,
  ) -> Result<NativeFunc<T, O>, PluginError> {
    helper_get_function(self.get_instance(), self.get_options(), name)
  }

  fn log_and_transform_error(&self, error: RuntimeError, name: &String) -> PluginError {
    error!(
      "WASM:{}:{} {:?}",
      self.get_options().module_name,
      name,
      error
    );
    match self.read_from_stderr() {
      Some(out) => error!("{}", out),
      None => (),
    };
    PluginError::RuntimeError
  }

  fn write_to_stdin(&self, payload: &String) {
    let mut state = self.get_environment().state();
    let wasi_stdin = state.fs.stdin_mut().unwrap().as_mut().unwrap();
    writeln!(wasi_stdin, "{}", payload).unwrap();
  }

  fn read_from_stdout(&self) -> Option<String> {
    let mut state = self.get_environment().state();
    let wasi_stdout = state.fs.stdout_mut().unwrap().as_mut().unwrap();
    let mut buf = String::new();
    match wasi_stdout.read_to_string(&mut buf) {
      Ok(_) => {
        let result = String::from(buf.trim());
        if result.len() > 0 {
          Some(result)
        } else {
          None
        }
      }
      Err(_) => None,
    }
  }

  fn read_from_stderr(&self) -> Option<String> {
    let mut state = self.get_environment().state();
    let wasi_stderror = state.fs.stderr_mut().unwrap().as_mut().unwrap();
    let mut buf = String::new();
    match wasi_stderror.read_to_string(&mut buf) {
      Ok(_) => {
        let result = String::from(buf.trim());
        if result.len() > 0 {
          Some(result)
        } else {
          None
        }
      }
      Err(_) => None,
    }
  }

  fn get_string(&self, ptr: WasmerStringPtr) -> String {
    let memory = self.get_memory();
    let length = memory
      .view::<u32>()
      .get(ptr.offset() as usize / (32 / 8) - 1)
      .unwrap()
      .get();

    let buf = ptr.deref(memory, 0, length).unwrap();
    let input: Vec<u8> = buf.iter().map(|b| b.get()).collect();
    return String::from(String::from_utf8_lossy(&input));
  }

  fn allocate_string(&self, input: &String) -> WasmerStringPtr {
    let length = input.len();
    let ptr = match self.get_malloc_fn().call(u32::try_from(length).unwrap()) {
      Ok(result) => result,
      Err(error) => {
        error!("{}", error);
        panic!(
          "WASM:{} Unable to allocate string",
          self.get_options().module_name
        );
      }
    };

    let memory = self.get_memory();

    let new_str = input.as_bytes();
    let values = ptr.deref(memory, 0, new_str.len() as u32).unwrap();
    for i in 0..new_str.len() {
      values[i].set(new_str[i]);
    }

    return ptr;
  }

  fn init(&self, config: &String) -> Result<(), PluginError> {
    let start = self.get_function::<(), ()>(&self.get_options().start_function_name)?;

    match start.call() {
      Ok(_) => {
        match self.read_from_stdout() {
          Some(out) => info!(
            "WASM:{}:{} {}",
            self.get_options().module_name,
            self.get_options().start_function_name,
            out
          ),
          None => (),
        };
      }
      Err(error) => {
        return Err(self.log_and_transform_error(error, &self.get_options().start_function_name));
      }
    };

    self.run_init(&config)
  }

  fn run_init(&self, config: &String) -> Result<(), PluginError> {
    let config_ptr = self.allocate_string(config);

    let init = self.get_function::<WasmerStringPtr, ()>(&self.get_options().init_function_name)?;
    match init.call(config_ptr) {
      Ok(_) => {
        match self.read_from_stdout() {
          Some(out) => info!(
            "WASM:{}:{} {}",
            self.get_options().module_name,
            self.get_options().init_function_name,
            out
          ),
          None => (),
        };
        return Ok(());
      }
      Err(error) => {
        Err(self.log_and_transform_error(error, &self.get_options().init_function_name))
      }
    }
  }
}
