use crate::prelude::SessionContext;
use libloading::Library;
use std::io::ErrorKind;
use std::{
    alloc::System, collections::HashMap, ffi::OsStr, io, path::PathBuf,
    rc::Rc,
};
use crate::execution::context::plugin_core::{Function, InvocationError, PluginDeclaration, PluginRegistrar};

#[global_allocator]
static ALLOCATOR: System = System;

pub fn load_plugin(plugin_library: PathBuf, function: String, mut context: SessionContext) {

    // create our functions table and load the plugin
    let mut functions = ExternalFunctions::new();

    unsafe {
        functions
            .load(plugin_library)
            .expect("Function loading failed");
    }

    // then call the function
    functions
        .call(&function, &mut context)
        .expect("Invocation failed");

}

/// A map of all externally provided functions.
#[derive(Default)]
pub struct ExternalFunctions {
    functions: HashMap<String, FunctionProxy>,
    libraries: Vec<Rc<Library>>,
}

impl ExternalFunctions {
    pub fn new() -> ExternalFunctions { ExternalFunctions::default() }

    pub fn call(
        &self,
        function: &str,
        argument: &mut SessionContext,
    ) -> Result<(), InvocationError> {
        self.functions
            .get(function)
            .ok_or_else(|| format!("\"{}\" not found", function))?
            .call(argument)
    }

    /// Load a plugin library and add all contained functions to the internal
    /// function table.
    ///
    /// # Safety
    ///
    /// A plugin library **must** be implemented using the
    /// [`plugins_core::plugin_declaration!()`] macro. Trying manually implement
    /// a plugin without going through that macro will result in undefined
    /// behaviour.
    pub unsafe fn load<P: AsRef<OsStr>>(
        &mut self,
        library_path: P,
    ) -> io::Result<()> {
        // load the library into memory
        let library = Rc::new(Library::new(library_path).map_err(|e| { io::Error::new(ErrorKind::Other, e.to_string()) })?);

        // get a pointer to the plugin_declaration symbol.
        let decl = library
            .get::<*mut PluginDeclaration>(b"plugin_declaration\0").map_err(|e| { io::Error::new(ErrorKind::Other, e.to_string()) })?
            .read();

        let mut registrar = AppPluginRegistrar::new(Rc::clone(&library));

        (decl.register)(&mut registrar);

        // add all loaded plugins to the functions map
        self.functions.extend(registrar.functions);
        // and make sure ExternalFunctions keeps a reference to the library
        self.libraries.push(library);

        Ok(())
    }
}

struct AppPluginRegistrar {
    functions: HashMap<String, FunctionProxy>,
    lib: Rc<Library>,
}

impl AppPluginRegistrar {
    fn new(lib: Rc<Library>) -> AppPluginRegistrar {
        AppPluginRegistrar {
            lib,
            functions: HashMap::default(),
        }
    }
}

impl PluginRegistrar for AppPluginRegistrar {
    fn register_function(&mut self, name: &str, function: Box<dyn Function>) {
        let proxy = FunctionProxy {
            function,
            _lib: Rc::clone(&self.lib),
        };
        self.functions.insert(name.to_string(), proxy);
    }
}

/// A proxy object which wraps a [`Function`] and makes sure it can't outlive
/// the library it came from.
pub struct FunctionProxy {
    function: Box<dyn Function>,
    _lib: Rc<Library>,
}

impl Function for FunctionProxy {
    fn call(&self, context: &mut SessionContext) -> Result<(), InvocationError> {
        self.function.call(context)
    }

    fn help(&self) -> Option<&str> {
        self.function.help()
    }
}
