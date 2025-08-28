#![warn(clippy::pedantic)]

use std::{collections::HashMap, error::Error, ffi::c_char};

pub use dynamic_plugin::libc::{free, malloc, strcpy};
pub use dynamic_plugin::plugin_impl;
use dynamic_plugin::plugin_interface;
use getset::{Getters, MutGetters};
pub use lazy_static::lazy_static;

pub use testangel_engine_macros::*;
pub use testangel_ipc::ffi::{
    evidence::{ta_evidence, ta_evidence_kind},
    instruction::{ta_instruction_metadata, ta_instruction_named_kind},
    logging::ta_logging_level,
    result::{ta_result, ta_result_code},
    ta_engine_metadata,
    value::{ta_inner_value, ta_named_value, ta_parameter_kind},
};
pub use testangel_ipc::prelude::*;

/// A utility for quickly generating [`InstructionWithParameters`] objects,
/// to make writing unit tests easier.
#[macro_export]
macro_rules! iwp {
    ($instruction_name: expr, $dry_run: expr) => {
        ::testangel_engine::InstructionWithParameters {
            instruction: String::from($instruction_name),
            dry_run: $dry_run,
            parameters: ::std::collections::HashMap::new(),
        }
    };
    ($instruction_name: expr, $dry_run: expr, $($name: expr => $val: expr),*) => {
        ::testangel_engine::InstructionWithParameters {
            instruction: String::from($instruction_name),
            dry_run: $dry_run,
            parameters: {
                let mut map = ::std::collections::HashMap::new();
                $(
                    map.insert(String::from($name), $val.into());
                )*
                map
            },
        }
    };
}

plugin_interface! {
    extern trait EngineInterface {
        /// Register a logger
        fn ta_register_logger(fn_log: unsafe extern "C" fn(ta_logging_level, *const c_char));

        /// Return a list of instructions this engine supports
        fn ta_request_instructions(
            p_output_engine_metadata: *mut ta_engine_metadata,
            parp_output_instructions: *mut *mut *const ta_instruction_metadata,
        ) -> *mut ta_result;

        /// Execute an instruction
        fn ta_execute(
            sz_instruction_id: *const c_char,
            arp_parameter_list: *const *const ta_named_value,
            n_parameter_count: u32,
            b_dry_run: bool,
            parp_output_list: *mut *mut *mut ta_named_value,
            parp_output_evidence_list: *mut *mut *mut ta_evidence,
        ) -> *mut ta_result;

        /// Reset engine state
        fn ta_reset_state() -> *mut ta_result;

        /// Free a result struct
        fn ta_free_result(p_target: *const ta_result);

        /// Free an engine metadata struct
        fn ta_free_engine_metadata(p_target: *const ta_engine_metadata);

        /// Free an array of instruction metadata structs
        fn ta_free_instruction_metadata_array(arp_target: *const *const ta_instruction_metadata);

        /// Free an array of named value structs
        fn ta_free_named_value_array(arp_target: *const *const ta_named_value);

        /// Free an array of evidence structs
        fn ta_free_evidence_array(arp_target: *const *const ta_evidence);
    }
}

pub type ParameterMap = HashMap<String, ParameterValue>;
pub type OutputMap = HashMap<String, ParameterValue>;
pub type EvidenceList = Vec<Evidence>;

pub type FnEngineInstruction<'a, T> = dyn 'a
    + Send
    + Sync
    + Fn(&mut T, ParameterMap, bool, &mut OutputMap, &mut EvidenceList) -> Result<(), Box<dyn Error>>;

#[derive(Getters, MutGetters)]
pub struct Engine<'a, T: Default + Send + Sync> {
    #[getset(get = "pub")]
    name: String,
    #[getset(get = "pub")]
    version: String,
    #[getset(get = "pub")]
    lua_name: String,
    #[getset(get = "pub")]
    description: String,
    #[getset(get = "pub")]
    instructions: Vec<Instruction>,
    #[getset(get = "pub")]
    functions: HashMap<String, Box<FnEngineInstruction<'a, T>>>,
    #[getset(get_mut = "pub")]
    state: T,
}

impl<'a, T: Default + Send + Sync> Engine<'a, T> {
    /// Create a new engine with the given name
    pub fn new<S: AsRef<str>>(name: S, lua_name: S, version: S, description: S) -> Self {
        Self {
            name: name.as_ref().to_string(),
            version: version.as_ref().to_string(),
            lua_name: lua_name.as_ref().to_string(),
            description: description.as_ref().to_string(),
            instructions: vec![],
            state: Default::default(),
            functions: HashMap::new(),
        }
    }

    /// Add an instruction to this engine.
    #[must_use]
    pub fn with_instruction<F>(mut self, instruction: Instruction, execute: F) -> Self
    where
        F: 'a
            + Send
            + Sync
            + Fn(
                &mut T,
                ParameterMap,
                bool,
                &mut OutputMap,
                &mut EvidenceList,
            ) -> Result<(), Box<dyn Error>>,
    {
        self.functions
            .insert(instruction.id().clone(), Box::new(execute));
        self.instructions.push(instruction);
        self
    }

    /// Request an instruction to be run on an engine.
    ///
    /// ## Errors
    ///
    /// Returns a [`String`] representation of an execution error, either from
    /// the engine itself (in the case of trying to find an instruction), or
    /// from an instruction (in the case of a failure or validation issue.)
    pub fn run_instruction(
        &mut self,
        iwp: InstructionWithParameters,
    ) -> Result<(OutputMap, EvidenceList), String> {
        let f = &self.functions[&iwp.instruction];
        let mut this_instruction_output = OutputMap::new();
        let mut this_instruction_evidence = EvidenceList::new();
        let instruction_result = f(
            &mut self.state,
            iwp.parameters,
            iwp.dry_run,
            &mut this_instruction_output,
            &mut this_instruction_evidence,
        );
        if let Err(e) = instruction_result {
            return Err(format!("{e}"));
        }
        Ok((this_instruction_output, this_instruction_evidence))
    }

    /// Reset the state of the engine
    pub fn reset_state(&mut self) {
        self.state = Default::default();
    }
}
