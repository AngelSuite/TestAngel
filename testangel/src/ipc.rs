use std::{
    env,
    ffi::{CStr, CString, c_char},
    fmt, fs, io,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use libc::malloc;
use testangel_engine::{
    EngineInterface, EvidenceList, OutputMap, ta_engine_metadata, ta_instruction_metadata,
    ta_logging_level, ta_named_value, ta_result_code,
};
use testangel_ipc::{
    ffi::{
        evidence::ta_evidence_kind,
        value::{ta_inner_value, ta_parameter_kind, ta_value},
    },
    prelude::*,
};

unsafe extern "C" fn log_fn(level: ta_logging_level, message: *const c_char) {
    unsafe {
        let message = {
            let cstr = CStr::from_ptr(message);
            let str_slice = cstr.to_str().unwrap();
            str_slice.to_owned()
        };

        match level {
            ta_logging_level::TA_LOG_TRACE => tracing::trace!("[From engine]: {message}"),
            ta_logging_level::TA_LOG_DEBUG => tracing::debug!("[From engine]: {message}"),
            ta_logging_level::TA_LOG_INFO => tracing::info!("[From engine]: {message}"),
            ta_logging_level::TA_LOG_WARN => tracing::warn!("[From engine]: {message}"),
            ta_logging_level::TA_LOG_ERROR => tracing::error!("[From engine]: {message}"),
        }
    }
}

#[derive(Debug)]
pub enum IpcError {
    IoError(io::Error),
    EngineNotStarted,
    EngineNotCompliant,
    CantLockEngineIo,
}

/// Trigger an IPC call to an engine.
///
/// ## Errors
///
/// - [`IpcError::IoError`] if the engine DLL cannot be opened properly
/// - [`IpcError::EngineNotStarted`] if the engine is not ready to communicate
/// - [`IpcError::EngineNotCompliant`] if the engine is not responding in a
///   manner deemed valid.
/// - [`IpcError::CantLockEngineIo`] if the engine library cannot be locked
///   because it is being used elsewhere.
///
/// ## Safety
///
/// As this function crosses the FFI boundary, safety cannot be guaranteed.
/// However, as long as functions adhere to the expected structure of an engine,
/// this should behave correctly.
///
/// ## Panics
///
/// This function can pan
#[allow(clippy::too_many_lines)]
pub unsafe fn ipc_call(engine: &Engine, request: &Request) -> Result<Response, IpcError> {
    unsafe {
        tracing::debug!(
            "Sending request {:?} to engine {} at {:?}.",
            request,
            engine,
            engine.path
        );

        let lib = engine.lib.clone().ok_or(IpcError::EngineNotStarted)?;
        let res = match request {
            Request::Instructions => {
                let mut engine_meta = ta_engine_metadata {
                    iSupportsIpcVersion: 0,
                    szFriendlyName: std::ptr::null(),
                    szVersion: std::ptr::null(),
                    szLuaName: std::ptr::null(),
                    szDescription: std::ptr::null(),
                };

                let mut raw_instructions: *mut *const ta_instruction_metadata =
                    std::ptr::null_mut();

                lib.ta_request_instructions(&mut engine_meta, &mut raw_instructions)
                    .map_err(|_| IpcError::EngineNotCompliant)?;

                if engine_meta.szFriendlyName.is_null() {
                    Err(IpcError::EngineNotCompliant)?;
                }
                if engine_meta.szVersion.is_null() {
                    Err(IpcError::EngineNotCompliant)?;
                }
                if engine_meta.szLuaName.is_null() {
                    Err(IpcError::EngineNotCompliant)?;
                }
                let friendly_name = {
                    let cstr = CStr::from_ptr(engine_meta.szFriendlyName);
                    let str_slice = cstr.to_str().map_err(|_| IpcError::EngineNotCompliant)?;
                    str_slice.to_owned()
                };
                let engine_version = {
                    let cstr = CStr::from_ptr(engine_meta.szVersion);
                    let str_slice = cstr.to_str().map_err(|_| IpcError::EngineNotCompliant)?;
                    str_slice.to_owned()
                };
                let engine_lua_name = {
                    let cstr = CStr::from_ptr(engine_meta.szLuaName);
                    let str_slice = cstr.to_str().map_err(|_| IpcError::EngineNotCompliant)?;
                    str_slice.to_owned()
                };
                let description = {
                    if engine_meta.szDescription.is_null() {
                        String::new()
                    } else {
                        let cstr = CStr::from_ptr(engine_meta.szDescription);
                        let str_slice = cstr.to_str().map_err(|_| IpcError::EngineNotCompliant)?;
                        str_slice.to_owned()
                    }
                };
                lib.ta_free_engine_metadata(&engine_meta)
                    .map_err(|_| IpcError::EngineNotCompliant)?;

                let mut i = 0;
                let mut instructions = vec![];
                loop {
                    let instruction_raw = *raw_instructions.add(i);
                    if instruction_raw.is_null() {
                        break;
                    }
                    instructions.push(
                        Instruction::from_ffi(instruction_raw)
                            .map_err(|_| IpcError::EngineNotCompliant)?,
                    );
                    i += 1;
                }

                let ipc_version = usize::try_from(engine_meta.iSupportsIpcVersion).unwrap();

                lib.ta_free_instruction_metadata_array(raw_instructions)
                    .map_err(|_| IpcError::EngineNotCompliant)?;

                Response::Instructions {
                    friendly_name,
                    engine_version,
                    engine_lua_name,
                    description,
                    ipc_version,
                    instructions,
                }
            }
            Request::RunInstruction {
                instruction: inst_with_params,
            } => {
                let mut r = None;
                for instruction in &engine.instructions {
                    if inst_with_params.instruction == *instruction.id() {
                        tracing::trace!("Found matching instruction in engine");
                        let mut output = OutputMap::new();
                        let mut evidence = EvidenceList::new();

                        // run this instruction
                        // Validate parameters
                        tracing::trace!("Validating parameters");
                        if let Err((kind, reason)) = instruction.validate(inst_with_params) {
                            r = Some(Response::Error { kind, reason });
                            break;
                        }

                        tracing::trace!("Converting parameters to C ABI");
                        let arp_parameter_list = malloc(
                            (inst_with_params.parameters.len() + 1)
                                * size_of::<*mut ta_named_value>(),
                        )
                        .cast::<*const ta_named_value>();
                        *arp_parameter_list.add(inst_with_params.parameters.len()) =
                            std::ptr::null();

                        for (idx, (id, param)) in inst_with_params.parameters.iter().enumerate() {
                            let p_value: ta_inner_value = match param.kind() {
                                ParameterKind::Boolean => {
                                    let val = Box::new(param.value_bool());
                                    let val = Box::into_raw(val);
                                    ta_inner_value { bValue: val }
                                }
                                ParameterKind::Decimal => {
                                    let val = Box::new(param.value_f64());
                                    let val = Box::into_raw(val);
                                    ta_inner_value { fValue: val }
                                }
                                ParameterKind::Integer => {
                                    let val = Box::new(param.value_i32());
                                    let val = Box::into_raw(val);
                                    ta_inner_value { iValue: val }
                                }
                                ParameterKind::String => {
                                    let val = param.value_string();
                                    let val = CString::new(val.as_str()).unwrap();
                                    let p_val = val.into_raw();
                                    ta_inner_value { szValue: p_val }
                                }
                            };

                            let sz_name = CString::new(id.clone()).unwrap();
                            let named_val = Box::new(ta_named_value {
                                szName: sz_name.into_raw(),
                                value: ta_value {
                                    kind: match param.kind() {
                                        ParameterKind::Boolean => {
                                            ta_parameter_kind::TA_PARAMETER_BOOLEAN
                                        }
                                        ParameterKind::Decimal => {
                                            ta_parameter_kind::TA_PARAMETER_DECIMAL
                                        }
                                        ParameterKind::Integer => {
                                            ta_parameter_kind::TA_PARAMETER_INTEGER
                                        }
                                        ParameterKind::String => {
                                            ta_parameter_kind::TA_PARAMETER_STRING
                                        }
                                    },
                                    value: p_value,
                                },
                            });

                            *arp_parameter_list.add(idx) = Box::into_raw(named_val);
                        }

                        let mut parp_output_list = std::ptr::null_mut();
                        let mut parp_output_evidence_list = std::ptr::null_mut();

                        // Execute instruction
                        tracing::trace!("Executing instruction");
                        let sz_instruction_id =
                            CString::new(inst_with_params.instruction.clone()).unwrap();
                        let result = lib
                            .ta_execute(
                                sz_instruction_id.as_ptr(),
                                arp_parameter_list,
                                inst_with_params.parameters.len().try_into().unwrap(),
                                inst_with_params.dry_run,
                                &mut parp_output_list,
                                &mut parp_output_evidence_list,
                            )
                            .map_err(|_| IpcError::EngineNotCompliant)?;

                        // Free parameters, param.szName, param inner value
                        tracing::trace!("Freeing parameters");
                        let mut i = 0;
                        loop {
                            let parameter_raw = *arp_parameter_list.add(i);
                            if parameter_raw.is_null() {
                                break;
                            }

                            let param = Box::from_raw(parameter_raw.cast_mut());
                            let _ = CString::from_raw(param.szName.cast_mut());
                            match param.value.kind {
                                ta_parameter_kind::TA_PARAMETER_STRING => {
                                    let _ = CString::from_raw(param.value.value.szValue.cast_mut());
                                }
                                ta_parameter_kind::TA_PARAMETER_BOOLEAN => {
                                    let _ = Box::from_raw(param.value.value.bValue.cast_mut());
                                }
                                ta_parameter_kind::TA_PARAMETER_DECIMAL => {
                                    let _ = Box::from_raw(param.value.value.fValue.cast_mut());
                                }
                                ta_parameter_kind::TA_PARAMETER_INTEGER => {
                                    let _ = Box::from_raw(param.value.value.iValue.cast_mut());
                                }
                            }

                            i += 1;
                        }

                        // Validate response
                        tracing::trace!("Validating response");
                        if (*result).code != ta_result_code::TESTANGEL_OK {
                            let reason = {
                                if (*result).szReason.is_null() {
                                    String::new()
                                } else {
                                    let cstr = CStr::from_ptr((*result).szReason);
                                    let str_slice =
                                        cstr.to_str().map_err(|_| IpcError::EngineNotCompliant)?;
                                    str_slice.to_owned()
                                }
                            };
                            r = Some(Response::Error {
                                kind: ErrorKind::EngineProcessingError,
                                reason,
                            });
                            break;
                        }

                        // Convert output and evidence back
                        tracing::trace!("Converting output and evidence");
                        let mut i = 0;
                        loop {
                            let output_raw = *parp_output_list.add(i);
                            if output_raw.is_null() {
                                break;
                            }

                            let k = {
                                let cstr = CStr::from_ptr((*output_raw).szName);
                                let str_slice =
                                    cstr.to_str().map_err(|_| IpcError::EngineNotCompliant)?;
                                str_slice.to_owned()
                            };
                            output.insert(
                                k,
                                match (*output_raw).value.kind {
                                    ta_parameter_kind::TA_PARAMETER_BOOLEAN => {
                                        let p_val = (*output_raw).value.value.bValue;
                                        ParameterValue::Boolean(*p_val)
                                    }
                                    ta_parameter_kind::TA_PARAMETER_INTEGER => {
                                        let p_val = (*output_raw).value.value.iValue;
                                        ParameterValue::Integer(*p_val)
                                    }
                                    ta_parameter_kind::TA_PARAMETER_DECIMAL => {
                                        let p_val = (*output_raw).value.value.fValue;
                                        ParameterValue::Decimal(*p_val)
                                    }
                                    ta_parameter_kind::TA_PARAMETER_STRING => {
                                        let p_val = (*output_raw).value.value.szValue;
                                        let val = {
                                            let cstr = CStr::from_ptr(p_val);
                                            let str_slice = cstr
                                                .to_str()
                                                .map_err(|_| IpcError::EngineNotCompliant)?;
                                            str_slice.to_owned()
                                        };
                                        ParameterValue::String(val)
                                    }
                                },
                            );

                            i += 1;
                        }

                        let mut i = 0;
                        loop {
                            let output_evidence = *parp_output_evidence_list.add(i);
                            if output_evidence.is_null() {
                                break;
                            }

                            let label = {
                                let cstr = CStr::from_ptr((*output_evidence).szLabel);
                                let str_slice =
                                    cstr.to_str().map_err(|_| IpcError::EngineNotCompliant)?;
                                str_slice.to_owned()
                            };
                            evidence.push(Evidence {
                                label,
                                content: match (*output_evidence).kind {
                                    ta_evidence_kind::TA_EVIDENCE_TEXTUAL => {
                                        let text = {
                                            let cstr = CStr::from_ptr((*output_evidence).value);
                                            let str_slice = cstr
                                                .to_str()
                                                .map_err(|_| IpcError::EngineNotCompliant)?;
                                            str_slice.to_owned()
                                        };
                                        EvidenceContent::Textual(text)
                                    }
                                    ta_evidence_kind::TA_EVIDENCE_PNGBASE64 => {
                                        let text = {
                                            let cstr = CStr::from_ptr((*output_evidence).value);
                                            let str_slice = cstr
                                                .to_str()
                                                .map_err(|_| IpcError::EngineNotCompliant)?;
                                            str_slice.to_owned()
                                        };
                                        EvidenceContent::ImageAsPngBase64(text)
                                    }
                                    ta_evidence_kind::TA_EVIDENCE_HTTPREQRES => {
                                        let text = {
                                            let cstr = CStr::from_ptr((*output_evidence).value);
                                            let str_slice = cstr
                                                .to_str()
                                                .map_err(|_| IpcError::EngineNotCompliant)?;
                                            str_slice.to_owned()
                                        };
                                        let mut splits = text.splitn(2, '\x1e');
                                        let req = splits.next().unwrap().to_string();
                                        let res = splits.next().unwrap().to_string();
                                        EvidenceContent::HttpRequestResponse(req, res)
                                    }
                                },
                            });

                            i += 1;
                        }

                        // Send output and evidence back to be freed
                        tracing::trace!("Freeing output and evidence");
                        lib.ta_free_named_value_array(parp_output_list.cast())
                            .map_err(|_| IpcError::EngineNotCompliant)?;
                        lib.ta_free_evidence_array(parp_output_evidence_list.cast())
                            .map_err(|_| IpcError::EngineNotCompliant)?;

                        r = Some(Response::ExecutionOutput { output, evidence });
                    }
                }

                // If the requested instruction doesn't match:
                r.unwrap_or_else(|| Response::Error {
                    kind: ErrorKind::InvalidInstruction,
                    reason: format!(
                        "The requested instruction {} could not be handled by this engine.",
                        inst_with_params.instruction
                    ),
                })
            }
            Request::ResetState => {
                lib.ta_reset_state()
                    .map_err(|_| IpcError::EngineNotCompliant)?;
                Response::StateReset
            }
        };

        tracing::debug!("Got response {res:?}");
        Ok(res)
    }
}

#[derive(Clone, Default)]
pub struct Engine {
    path: PathBuf,
    pub name: String,
    pub lua_name: String,
    pub description: String,
    pub instructions: Vec<Instruction>,
    lib: Option<Arc<EngineInterface>>,
}

impl Engine {
    /// Ask the engine to reset it's state for test repeatability.
    ///
    /// ## Errors
    ///
    /// - [`IpcError::IoError`] if the engine DLL cannot be opened properly
    /// - [`IpcError::EngineNotStarted`] if the engine is not ready to communicate
    /// - [`IpcError::EngineNotCompliant`] if the engine is not responding in a
    ///   manner deemed valid.
    /// - [`IpcError::CantLockEngineIo`] if the engine library cannot be locked
    ///   because it is being used elsewhere.
    pub fn reset_state(&self) -> Result<(), IpcError> {
        unsafe { ipc_call(self, &Request::ResetState) }.map(|_| ())
    }
}

impl fmt::Display for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("path", &self.path)
            .field("name", &self.name)
            .field("lua_name", &self.lua_name)
            .field("instructions", &self.instructions)
            .finish_non_exhaustive()
    }
}

#[derive(Default, Debug)]
pub struct EngineList(Vec<Engine>);

impl Deref for EngineList {
    type Target = Vec<Engine>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl EngineList {
    /// Get an instruction from an instruction ID by iterating through available engines.
    #[must_use]
    pub fn get_instruction_by_id(&self, instruction_id: &String) -> Option<Instruction> {
        for engine in &self.0 {
            for inst in &engine.instructions {
                if *inst.id() == *instruction_id {
                    return Some(inst.clone());
                }
            }
        }
        None
    }

    /// Get an instruction and engine from an instruction ID by iterating through available engines.
    #[must_use]
    pub fn get_engine_by_instruction_id(&self, instruction_id: &String) -> Option<&Engine> {
        for engine in &self.0 {
            for inst in &engine.instructions {
                if *inst.id() == *instruction_id {
                    return Some(engine);
                }
            }
        }
        None
    }
}

#[must_use]
fn get_engine_directory() -> PathBuf {
    if let Ok(env_path) = env::var("TA_ENGINE_DIR") {
        PathBuf::from(env_path)
    } else if let Some(mut exe_path) = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        // Fix for #265
        if cfg!(windows) && cfg!(feature = "ui") {
            // Traverse one more level up to get out of `bin`
            exe_path = exe_path.parent().unwrap().to_path_buf();
        }
        exe_path.join("engines")
    } else {
        PathBuf::from(".").join("engines")
    }
}

/// Get the list of available engines.
///
/// ## Panics
///
/// This will panic if the engine directory cannot be created.
pub fn get_engines() -> EngineList {
    let mut engines = Vec::new();
    let engine_dir = get_engine_directory();
    fs::create_dir_all(engine_dir.clone()).unwrap();
    tracing::info!("Searching for engines in {engine_dir:?}");
    let mut lua_names = vec![];
    search_engine_dir(engine_dir, &mut engines, &mut lua_names);
    EngineList(engines)
}

fn search_engine_dir<P: AsRef<Path>>(
    engine_dir: P,
    engines: &mut Vec<Engine>,
    lua_names: &mut Vec<String>,
) {
    for path in fs::read_dir(engine_dir).unwrap() {
        let path = path.unwrap();
        let basename = path.file_name();
        if let Ok(meta) = path.metadata() {
            if meta.is_dir() {
                // Search subdir
                search_engine_dir(
                    path.path()
                        .canonicalize()
                        .unwrap()
                        .as_os_str()
                        .to_os_string()
                        .into_string()
                        .unwrap(),
                    engines,
                    lua_names,
                );
                continue;
            }
        }

        if let Ok(str) = basename.into_string() {
            if Path::new(&str).extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("so")
                    || ext.eq_ignore_ascii_case("dll")
                    || ext.eq_ignore_ascii_case("dylib")
            }) {
                tracing::debug!("Detected possible engine {str}");
                match EngineInterface::load_plugin(path.path(), false) {
                    Ok(lib) => {
                        let mut engine = Engine {
                            name: String::from("newly discovered engine"),
                            path: path.path(),
                            lib: Some(Arc::new(lib)),
                            ..Default::default()
                        };

                        if let Some(lib) = &engine.lib {
                            if let Err(e) = lib.ta_register_logger(log_fn) {
                                tracing::warn!("Failed to register logger with engine: {e}");
                            }
                        }

                        match unsafe { ipc_call(&engine, &Request::Instructions) } {
                            Ok(res) => {
                                if let Response::Instructions {
                                    friendly_name,
                                    engine_version,
                                    engine_lua_name,
                                    description,
                                    ipc_version,
                                    instructions,
                                } = res
                                {
                                    if ipc_version == 3 {
                                        if lua_names.contains(&engine_lua_name) {
                                            tracing::warn!(
                                                "Engine {friendly_name} (v{engine_version}) at {:?} uses a lua name that is already used by another engine!",
                                                path.path()
                                            );
                                            continue;
                                        }
                                        tracing::info!(
                                            "Discovered engine {friendly_name} (v{engine_version}) at {:?}",
                                            path.path()
                                        );
                                        engine.name.clone_from(&friendly_name);
                                        engine.lua_name.clone_from(&engine_lua_name);
                                        engine.description.clone_from(&description);
                                        engine.instructions = instructions;
                                        engines.push(engine);
                                        lua_names.push(engine_lua_name);
                                    } else {
                                        tracing::warn!(
                                            "Engine {friendly_name} (v{engine_version}) at {:?} doesn't speak the right IPC version!",
                                            path.path()
                                        );
                                    }
                                } else {
                                    tracing::error!("Invalid response from engine {str}");
                                }
                            }
                            Err(e) => tracing::warn!("IPC error: {e:?}"),
                        }
                    }
                    Err(e) => tracing::warn!("Failed to load engine {str}: {e}"),
                }
            }
        }
    }
}
