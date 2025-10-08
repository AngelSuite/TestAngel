use std::{collections::HashMap, fmt, sync::Arc};

use mlua::{Lua, ObjectLike};
use serde::{Deserialize, Serialize};
use testangel_ipc::prelude::*;
use thiserror::Error;

use crate::{
    action_loader::ActionMap,
    action_syntax::{Descriptor, FlagDescriptorKind, KeyValueDescriptorKind, TypedDescriptorKind},
    ipc::{self, EngineList, IpcError},
};

pub mod action_v1;
pub mod action_v2;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct VersionedFile {
    version: usize,
}

impl VersionedFile {
    /// Get the version of the file
    #[must_use]
    pub fn version(&self) -> usize {
        self.version
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// The data version of this action.
    version: usize,
    /// The internal ID of this action. Must be unique.
    pub id: String,
    /// The Lua code driving this action.
    pub script: String,
    /// A vector of required instruction IDs for this action to work.
    pub required_instructions: Vec<String>,
}

impl Default for Action {
    fn default() -> Self {
        Self {
            version: 3,
            id: uuid::Uuid::new_v4().to_string(),
            script: include_str!("new_action.lua").to_string(),
            required_instructions: Vec::new(),
        }
    }
}

impl Action {
    /// Get the version of this action.
    #[must_use]
    pub fn version(&self) -> usize {
        self.version
    }

    /// Generate a new ID for this action.
    pub fn new_id(&mut self) {
        self.id = uuid::Uuid::new_v4().to_string();
    }

    /// Check that all the instructions this action uses are available. Returns
    /// Ok if all instructions are available, otherwise returns a list of
    /// missing instructions.
    ///
    /// ## Errors
    ///
    /// This function returns a list of instructions that are unavailable as an error.
    pub fn check_instructions_available(
        &self,
        engine_list: &Arc<EngineList>,
    ) -> Result<(), Vec<String>> {
        let mut missing = vec![];
        for instruction in &self.required_instructions {
            if engine_list.get_instruction_by_id(instruction).is_none()
                && !missing.contains(instruction)
            {
                missing.push(instruction.clone());
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }

    /// Get the name of the action
    #[must_use]
    pub fn name(&self) -> Option<String> {
        let descriptors = Descriptor::parse_all(&self.script);
        for d in descriptors {
            if let Descriptor::KeyValueDescriptor {
                descriptor_kind,
                value,
            } = d
                && descriptor_kind == KeyValueDescriptorKind::Name
            {
                return Some(value);
            }
        }
        None
    }

    /// Get the group of the action
    #[must_use]
    pub fn group(&self) -> Option<String> {
        let descriptors = Descriptor::parse_all(&self.script);
        for d in descriptors {
            if let Descriptor::KeyValueDescriptor {
                descriptor_kind,
                value,
            } = d
                && descriptor_kind == KeyValueDescriptorKind::Group
            {
                return Some(value);
            }
        }
        None
    }

    /// Get the creator of the action
    #[must_use]
    pub fn creator(&self) -> Option<String> {
        let descriptors = Descriptor::parse_all(&self.script);
        for d in descriptors {
            if let Descriptor::KeyValueDescriptor {
                descriptor_kind,
                value,
            } = d
                && descriptor_kind == KeyValueDescriptorKind::Creator
            {
                return Some(value);
            }
        }
        None
    }

    /// Get the description of the action
    #[must_use]
    pub fn description(&self) -> Option<String> {
        let descriptors = Descriptor::parse_all(&self.script);
        for d in descriptors {
            if let Descriptor::KeyValueDescriptor {
                descriptor_kind,
                value,
            } = d
                && descriptor_kind == KeyValueDescriptorKind::Description
            {
                return Some(value);
            }
        }
        None
    }

    /// Should the action be hidden in the flow editor?
    #[must_use]
    pub fn hide_in_flow_editor(&self) -> bool {
        let descriptors = Descriptor::parse_all(&self.script);
        for d in descriptors {
            if let Descriptor::FlagDescriptor(flag) = d
                && flag == FlagDescriptorKind::HideInFlowEditor
            {
                return true;
            }
        }
        false
    }

    /// Get a list of parameters that need to be provided to this action.
    #[must_use]
    pub fn parameters(&self) -> Vec<(String, ParameterKind)> {
        let descriptors = Descriptor::parse_all(&self.script);
        let mut params = vec![];
        for d in descriptors {
            if let Descriptor::TypedDescriptor {
                descriptor_kind,
                kind,
                name,
            } = d
                && descriptor_kind == TypedDescriptorKind::Parameter
            {
                params.push((name.clone(), kind));
            }
        }
        params
    }

    /// Get a list of outputs provided by this action.
    #[must_use]
    pub fn outputs(&self) -> Vec<(String, ParameterKind)> {
        let descriptors = Descriptor::parse_all(&self.script);
        let mut outputs = vec![];
        for d in descriptors {
            if let Descriptor::TypedDescriptor {
                descriptor_kind,
                kind,
                name,
            } = d
                && descriptor_kind == TypedDescriptorKind::Return
            {
                outputs.push((name.clone(), kind));
            }
        }
        outputs
    }
}

#[derive(Debug, Error)]
pub enum FlowError {
    #[error("An instruction returned an error: {error_kind:?}: {reason}")]
    FromInstruction {
        error_kind: ErrorKind,
        reason: String,
    },
    #[error("An action script error occurred:\n{0}")]
    Lua(String),
    #[error("An IPC call failed ({0:?}).")]
    IPCFailure(IpcError),
    #[error("The action didn't return the correct amount of values.")]
    ActionDidntReturnCorrectArgumentCount,
    #[error("The action didn't return valid values.")]
    ActionDidntReturnValidArguments,
    #[error("An instruction was called with the wrong number of parameters.")]
    InstructionCalledWithWrongNumberOfParams,
    #[error("An instruction was called with the wrong parameter type.")]
    InstructionCalledWithInvalidParamType,
    #[error("The column '{0}' was missing from the spreadsheet")]
    SpreadsheetColumnMissing(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationFlow {
    /// The version of this automation flow file
    version: usize,
    /// The actions called by this flow
    pub actions: Vec<ActionConfiguration>,
}

impl Default for AutomationFlow {
    fn default() -> Self {
        Self {
            version: 1,
            actions: vec![],
        }
    }
}

impl AutomationFlow {
    /// Get the version of this flow.
    #[must_use]
    pub fn version(&self) -> usize {
        self.version
    }

    /// Do any steps of this flow require a datafile to be loaded to run?
    #[must_use]
    pub fn needs_datafile_to_run(&self) -> bool {
        for step in &self.actions {
            for src in step.parameter_sources.values() {
                if matches!(src, ActionParameterSource::FromSpreadsheetColumn(_)) {
                    return true;
                }
            }
        }
        false
    }
}

pub type ActionExecutionSuccess = (HashMap<usize, ParameterValue>, Vec<Evidence>);
pub type ActionExecutionFailure = (FlowError, Vec<Evidence>);

#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ActionConfiguration {
    pub action_id: String,
    pub parameter_sources: HashMap<usize, ActionParameterSource>,
    pub parameter_values: HashMap<usize, ParameterValue>,
}
impl ActionConfiguration {
    /// Execute this action
    ///
    /// ## Errors
    ///
    /// Returns an error if execution failed, and the current evidence.
    ///
    /// ## Panics
    ///
    /// Panics if the action does not exist, or if it is malformed.
    pub fn execute(
        &self,
        action_map: &Arc<ActionMap>,
        engine_map: &Arc<EngineList>,
        previous_action_outputs: &[HashMap<usize, ParameterValue>],
        spreadsheet_row: &HashMap<String, ParameterValue>,
    ) -> Result<ActionExecutionSuccess, ActionExecutionFailure> {
        // Find action by ID
        let action = action_map.get_action_by_id(&self.action_id).unwrap();
        // Build action parameters
        let mut action_parameters = HashMap::new();
        for (id, src) in &self.parameter_sources {
            let value = match src {
                ActionParameterSource::Literal => self.parameter_values.get(id).unwrap().clone(),
                ActionParameterSource::FromSpreadsheetColumn(col) => spreadsheet_row
                    .get(col)
                    .ok_or((FlowError::SpreadsheetColumnMissing(col.clone()), vec![]))?
                    .clone(),
                ActionParameterSource::FromOutput(step, id) => previous_action_outputs
                    .get(*step)
                    .unwrap()
                    .get(id)
                    .unwrap()
                    .clone(),
            };
            action_parameters.insert(*id, value);
        }
        let mut param_vec = vec![];
        for i in 0..action_parameters.len() {
            param_vec.push(action_parameters[&i].clone());
        }
        Self::execute_directly(engine_map, &action, &param_vec)
    }

    #[allow(clippy::type_complexity)]
    /// Directly execute an action with a set of parameters.
    ///
    /// ## Errors
    ///
    /// Returns an error if execution failed.
    ///
    /// ## Panics
    ///
    /// Panics if the action does not exist, or if it is malformed.
    #[allow(clippy::too_many_lines)]
    pub fn execute_directly(
        engine_map: &Arc<EngineList>,
        action: &Action,
        action_parameters: &[ParameterValue],
    ) -> Result<ActionExecutionSuccess, ActionExecutionFailure> {
        let mut output = HashMap::new();

        // Prepare Lua environment
        let lua_env = Lua::new();
        lua_env.set_app_data::<Vec<Evidence>>(vec![]);

        // SAFETY: this will only fail under memory issues
        for engine in &***engine_map {
            let engine_lua_name = engine.lua_name.clone();
            let engine_tbl = lua_env.create_table().unwrap();
            for instruction in engine.instructions.clone() {
                let instruction_lua_name = instruction.lua_name().clone();
                let engine = engine.clone();
                let instruction_fn = lua_env
                    .create_function(move |lua, args: mlua::MultiValue| {
                        // Check we have the correct number of parameters.
                        if args.len() != instruction.parameters().len() {
                            return Err(mlua::Error::external(
                                FlowError::InstructionCalledWithWrongNumberOfParams,
                            ));
                        }

                        // Check we have the correct parameter types and convert to parameter map
                        let mut param_map = HashMap::new();
                        for (
                            idx,
                            InstructionNamedKind {
                                id: param_id, kind, ..
                            },
                        ) in instruction.parameters().iter().enumerate()
                        {
                            // Get argument and coerce
                            let arg = args[idx].clone();
                            match kind {
                                ParameterKind::Boolean => {
                                    if let mlua::Value::Boolean(b) = arg {
                                        param_map
                                            .insert(param_id.clone(), ParameterValue::Boolean(b));
                                    } else {
                                        return Err(mlua::Error::external(
                                            FlowError::InstructionCalledWithInvalidParamType,
                                        ));
                                    }
                                }
                                ParameterKind::String => {
                                    let maybe_str = lua.coerce_string(arg)?;
                                    if let Some(s) = maybe_str {
                                        param_map.insert(
                                            param_id.clone(),
                                            ParameterValue::String(s.to_str()?.to_string()),
                                        );
                                    } else {
                                        return Err(mlua::Error::external(
                                            FlowError::InstructionCalledWithInvalidParamType,
                                        ));
                                    }
                                }
                                ParameterKind::Decimal => {
                                    let maybe_dec = lua.coerce_number(arg)?;
                                    if let Some(d) = maybe_dec {
                                        param_map
                                            .insert(param_id.clone(), ParameterValue::Decimal(d));
                                    } else {
                                        return Err(mlua::Error::external(
                                            FlowError::InstructionCalledWithInvalidParamType,
                                        ));
                                    }
                                }
                                ParameterKind::Integer => {
                                    let maybe_int = lua.coerce_integer(arg)?;
                                    if let Some(i) = maybe_int {
                                        param_map.insert(
                                            param_id.clone(),
                                            ParameterValue::Integer(i.try_into().unwrap()),
                                        );
                                    } else {
                                        return Err(mlua::Error::external(
                                            FlowError::InstructionCalledWithInvalidParamType,
                                        ));
                                    }
                                }
                            }
                        }

                        // Trigger instruction behaviour
                        let response = unsafe {
                            ipc::ipc_call(
                                &engine,
                                &Request::RunInstruction {
                                    instruction: InstructionWithParameters {
                                        instruction: instruction.id().clone(),
                                        dry_run: false,
                                        parameters: param_map,
                                    },
                                },
                            )
                        }
                        .map_err(|e| mlua::Error::external(FlowError::IPCFailure(e)))?;

                        match response {
                            Response::ExecutionOutput { output, evidence } => {
                                // Add evidence
                                let mut ev = lua.app_data_mut::<Vec<Evidence>>().unwrap();
                                for item in &evidence {
                                    ev.push(item.clone());
                                }

                                // Convert output back to Lua values
                                let mut outputs = vec![];
                                for InstructionNamedKind { id, .. } in instruction.outputs() {
                                    let o = output[id].clone();
                                    match o {
                                        ParameterValue::Boolean(b) => {
                                            tracing::debug!("Boolean {b} returned to Lua");
                                            outputs.push(mlua::Value::Boolean(b));
                                        }
                                        ParameterValue::String(s) => {
                                            tracing::debug!("String {s:?} returned to Lua");
                                            outputs
                                                .push(mlua::Value::String(lua.create_string(s)?));
                                        }
                                        ParameterValue::Integer(i) => {
                                            tracing::debug!("Integer {i} returned to Lua");
                                            outputs.push(mlua::Value::Integer(i.into()));
                                        }
                                        ParameterValue::Decimal(n) => {
                                            tracing::debug!("Decimal {n} returned to Lua");
                                            outputs.push(mlua::Value::Number(n));
                                        }
                                    }
                                }

                                Ok(mlua::MultiValue::from_vec(outputs))
                            }
                            Response::Error { kind, reason } => {
                                Err(mlua::Error::external(FlowError::FromInstruction {
                                    error_kind: kind,
                                    reason,
                                }))
                            }
                            _ => unreachable!(),
                        }
                    })
                    .unwrap();
                engine_tbl
                    .set(instruction_lua_name.as_str(), instruction_fn)
                    .unwrap();
            }
            lua_env
                .globals()
                .set(engine_lua_name.as_str(), engine_tbl)
                .unwrap();
        }

        // Execute Lua script
        // Add parameters via type coercion and get results
        let mut params = vec![];
        for (idx, param) in action_parameters.iter().enumerate() {
            let (_, expected_type) = action.parameters()[idx];
            match expected_type {
                ParameterKind::Boolean => match param {
                    ParameterValue::Boolean(b) => params.push(mlua::Value::Boolean(*b)),
                    _ => {
                        return Err((
                            FlowError::InstructionCalledWithInvalidParamType,
                            lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                        ));
                    }
                },
                ParameterKind::Decimal => match param {
                    ParameterValue::Decimal(d) => params.push(mlua::Value::Number(*d)),
                    ParameterValue::Integer(i) => params.push(mlua::Value::Number((*i).into())),
                    _ => {
                        return Err((
                            FlowError::InstructionCalledWithInvalidParamType,
                            lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                        ));
                    }
                },
                ParameterKind::Integer => match param {
                    ParameterValue::Integer(i) => params.push(mlua::Value::Integer((*i).into())),
                    _ => {
                        return Err((
                            FlowError::InstructionCalledWithInvalidParamType,
                            lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                        ));
                    }
                },
                ParameterKind::String => match param {
                    ParameterValue::Boolean(b) => params.push(mlua::Value::String(
                        lua_env.create_string(format!("{b:?}")).map_err(|e| {
                            (
                                FlowError::Lua(e.to_string()),
                                lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                            )
                        })?,
                    )),
                    ParameterValue::String(s) => params.push(mlua::Value::String(
                        lua_env.create_string(s).map_err(|e| {
                            (
                                FlowError::Lua(e.to_string()),
                                lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                            )
                        })?,
                    )),
                    ParameterValue::Integer(i) => params.push(mlua::Value::String(
                        lua_env.create_string(format!("{i}")).map_err(|e| {
                            (
                                FlowError::Lua(e.to_string()),
                                lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                            )
                        })?,
                    )),
                    ParameterValue::Decimal(n) => params.push(mlua::Value::String(
                        lua_env.create_string(format!("{n}")).map_err(|e| {
                            (
                                FlowError::Lua(e.to_string()),
                                lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                            )
                        })?,
                    )),
                },
            }
        }

        lua_env
            .load(&action.script)
            .set_name(action.name().unwrap_or("Unnamed Action".to_string()))
            .exec()
            .map_err(|e| {
                (
                    FlowError::Lua(e.to_string()),
                    lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                )
            })?;

        let res: mlua::MultiValue = lua_env
            .globals()
            .call_function("run_action", mlua::MultiValue::from_vec(params))
            .map_err(|e| {
                (
                    FlowError::Lua(e.to_string()),
                    lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                )
            })?;
        let res = res.into_vec();

        // Process return values
        let ao = action.outputs();
        if ao.len() != res.len() {
            return Err((
                FlowError::ActionDidntReturnCorrectArgumentCount,
                lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
            ));
        }
        for i in 0..ao.len() {
            let (_name, kind) = ao[i].clone();
            let out = res[i].clone();
            let ta_out = match out {
                mlua::Value::Boolean(b) => ParameterValue::Boolean(b),
                mlua::Value::String(s) => ParameterValue::String(s.to_str().unwrap().to_owned()),
                mlua::Value::Integer(i) => ParameterValue::Integer(i.try_into().unwrap()),
                mlua::Value::Number(n) => ParameterValue::Decimal(n),
                _ => {
                    return Err((
                        FlowError::ActionDidntReturnValidArguments,
                        lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                    ));
                }
            };
            if ta_out.kind() != kind {
                return Err((
                    FlowError::ActionDidntReturnValidArguments,
                    lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone(),
                ));
            }
            output.insert(i, ta_out);
        }

        let evidence = lua_env.app_data_ref::<Vec<Evidence>>().unwrap().clone();

        Ok((output, evidence))
    }

    /// Update this action configuration to match the inputs and outputs of the provided action.
    /// Return true if this configuration has changed.
    ///
    /// # Panics
    /// This will panic if the action's ID doesn't match the ID of this configuration already set.
    pub fn update(&mut self, action: Action) -> bool {
        assert!(
            self.action_id == action.id,
            "ActionConfiguration tried to be updated with a different action!"
        );

        // If number of parameters has changed
        if self.parameter_sources.len() != action.parameters().len() {
            *self = Self::from(action);
            return true;
        }

        for (n, value) in &self.parameter_values {
            let (_, action_param_kind) = &action.parameters()[*n];
            if value.kind() != *action_param_kind {
                // Reset parameters
                *self = Self::from(action);
                return true;
            }
        }

        false
    }
}

impl From<Action> for ActionConfiguration {
    fn from(value: Action) -> Self {
        let mut parameter_sources = HashMap::new();
        let mut parameter_values = HashMap::new();
        for (id, (_friendly_name, kind)) in value.parameters().iter().enumerate() {
            parameter_sources.insert(id, ActionParameterSource::Literal);
            parameter_values.insert(id, kind.default_value());
        }
        Self {
            action_id: value.id.clone(),
            parameter_sources,
            parameter_values,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionParameterSource {
    #[default]
    Literal,
    FromSpreadsheetColumn(String),
    FromOutput(usize, usize),
}

impl fmt::Display for ActionParameterSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FromOutput(step, id) => {
                write!(f, "From Step {}: Output {}", step + 1, id + 1)
            }
            Self::FromSpreadsheetColumn(col) => {
                write!(f, "From Spreadsheet Column: {col}")
            }
            Self::Literal => write!(f, "Literal"),
        }
    }
}
