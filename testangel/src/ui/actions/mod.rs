use std::{fmt, fs, path::PathBuf, rc::Rc, sync::Arc};

use adw::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    adw,
    gtk::{self, glib::SignalHandlerId},
};
use sourceview::StyleSchemeManager;
use testangel::{
    action_loader::ActionMap,
    ipc::EngineList,
    types::{Action, VersionedFile, action_v1::ActionV1, action_v2::ActionV2},
};
use testangel_engine::InstructionNamedKind;

use crate::lang_args;

use super::{file_filters, lang};
use sourceview5::{self as sourceview, prelude::ViewExt};

mod completion_proposal_list;
mod completion_provider_descriptors;
mod completion_provider_engines;
mod completion_provider_instructions;
pub mod header;

pub enum SaveOrOpenActionError {
    IoError(std::io::Error),
    ParsingError(ron::error::SpannedError),
    SerializingError(ron::Error),
    ActionNotVersionCompatible,
    MissingInstruction(String),
}

impl fmt::Display for SaveOrOpenActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::IoError(e) => lang::lookup_with_args(
                    "action-save-open-error-io-error",
                    lang_args!("error", e.to_string())
                ),
                Self::ParsingError(e) => {
                    lang::lookup_with_args(
                        "action-save-open-error-parsing-error",
                        lang_args!("error", e.to_string()),
                    )
                }
                Self::SerializingError(e) => {
                    lang::lookup_with_args(
                        "action-save-open-error-serializing-error",
                        lang_args!("error", e.to_string()),
                    )
                }
                Self::ActionNotVersionCompatible => {
                    lang::lookup("action-save-open-error-action-not-version-compatible")
                }
                Self::MissingInstruction(e) => {
                    lang::lookup_with_args(
                        "action-save-open-error-missing-instruction",
                        lang_args!("error", e.to_string()),
                    )
                }
            }
        )
    }
}

#[derive(Clone, Debug)]
pub enum ActionInputs {
    /// Do nothing
    NoOp,
    /// Request that the application is closed
    RequestProgramExit,
    /// Prompt to save before adding this action to the open flow
    AddOpenActionToFlow,
    /// Add this action to the open flow
    AddOpenActionToFlow_,
    /// The map of actions has changed and should be updated
    ActionsMapChanged(Arc<ActionMap>),
    /// Create a new action
    NewAction,
    /// Actually create the new action
    NewAction_,
    /// Prompt the user to open an action. This will ask to save first if needed.
    OpenAction,
    /// Actually show the user the open file dialog
    OpenAction_,
    /// Actually open an action after the user has finished selecting
    OpenAction__(PathBuf),
    /// Save the action, prompting if needed to set file path
    SaveAction,
    /// Save the action as a new file, always prompting for a file path
    SaveAsAction,
    /// Ask where to save if needed, then save
    SaveActionThen_(Box<ActionInputs>),
    /// Actually write the action to disk, then emit then input. First bool is whether a new ID should be used.
    SaveActionThen__(bool, PathBuf, Box<ActionInputs>),
    /// Close the action, prompting if needing to save first
    CloseActionThen(Box<ActionInputs>),
    /// Actually close the action
    CloseActionThen_(Box<ActionInputs>),
    /// Add the step with the ID provided
    AddStep(String),
    /// The contents of the text buffer have changed
    TextBufferChanged,
}
#[derive(Clone, Debug)]
pub enum ActionOutputs {
    /// Request that the application is closed.
    RequestProgramExit,
    /// Inform other parts that actions may have changed, reload them!
    ReloadActions,
    /// Add the open action to the open flow
    AddOpenActionToFlow(String),
    /// Updates if the flow needs saving or not
    SetNeedsSaving(bool),
}

#[derive(Debug)]
pub struct ActionsModel {
    action_map: Arc<ActionMap>,
    engine_list: Arc<EngineList>,

    open_action: Option<Action>,
    open_path: Option<PathBuf>,
    needs_saving: bool,
    header: Rc<Controller<header::ActionsHeader>>,
    source_view: sourceview::View,

    signal_text_buffer_changed: Option<SignalHandlerId>,
}

impl ActionsModel {
    /// Get an [`Rc`] clone of the header controller
    pub fn header_controller_rc(&self) -> Rc<Controller<header::ActionsHeader>> {
        self.header.clone()
    }

    /// Set whether the open flow needs saving
    pub fn set_needs_saving(
        &mut self,
        needs_saving: bool,
        sender: &relm4::ComponentSender<ActionsModel>,
    ) {
        self.needs_saving = needs_saving;
        sender
            .output(ActionOutputs::SetNeedsSaving(needs_saving))
            .unwrap();
    }

    /// Just open a brand new action
    fn new_action(&mut self, sender: &relm4::ComponentSender<ActionsModel>) {
        self.open_path = None;
        self.set_needs_saving(true, sender);
        let action = Action::default();
        self.source_view.buffer().set_text(&action.script);
        self.open_action = Some(action);
        self.header
            .emit(header::ActionsHeaderInput::ChangeActionOpen(
                self.open_action.is_some(),
            ));
    }

    /// Open an action. This does not ask to save first.
    fn open_action(
        &mut self,
        file: PathBuf,
        sender: &relm4::ComponentSender<ActionsModel>,
    ) -> Result<(), SaveOrOpenActionError> {
        let mut data = fs::read_to_string(&file).map_err(SaveOrOpenActionError::IoError)?;

        let versioned_file: VersionedFile =
            ron::from_str(&data).map_err(SaveOrOpenActionError::ParsingError)?;
        if versioned_file.version() == 1 {
            // Upgrade from instruction list to lua script
            // This doesn't save anything, just changes what loads to something compatible
            let action_v1: ActionV1 =
                ron::from_str(&data).map_err(SaveOrOpenActionError::ParsingError)?;
            let action_v2 = action_v1.upgrade_action(&self.engine_list);
            let action_upgraded = action_v2.upgrade_action();
            data = ron::to_string(&action_upgraded)
                .map_err(SaveOrOpenActionError::SerializingError)?;
        } else if versioned_file.version() == 2 {
            // This doesn't save anything, just changes what loads to something compatible
            let action_v2: ActionV2 =
                ron::from_str(&data).map_err(SaveOrOpenActionError::ParsingError)?;
            let action_upgraded = action_v2.upgrade_action();
            data = ron::to_string(&action_upgraded)
                .map_err(SaveOrOpenActionError::SerializingError)?;
        } else if versioned_file.version() != 3 {
            return Err(SaveOrOpenActionError::ActionNotVersionCompatible);
        }

        let action: Action = ron::from_str(&data).map_err(SaveOrOpenActionError::ParsingError)?;
        // Validate that all instructions used in the script are available, or return a MissingInstruction err
        action
            .check_instructions_available(&self.engine_list)
            .map_err(|missing| SaveOrOpenActionError::MissingInstruction(missing[0].clone()))?;

        self.source_view
            .buffer()
            .block_signal(self.signal_text_buffer_changed.as_ref().unwrap());
        self.source_view.buffer().set_text(&action.script);
        self.source_view
            .buffer()
            .unblock_signal(self.signal_text_buffer_changed.as_ref().unwrap());

        self.open_action = Some(action.clone());
        self.header
            .emit(header::ActionsHeaderInput::ChangeActionOpen(
                self.open_action.is_some(),
            ));
        self.open_path = Some(file);
        self.set_needs_saving(false, sender);
        tracing::debug!("New action open.");
        tracing::debug!("Action: {:?}", self.open_action);
        Ok(())
    }

    /// Ask the user if they want to save this file. If they response yes, this will also trigger the save function.
    /// This function will only ask the user if needed, otherwise it will emit immediately.
    fn prompt_to_save(
        &self,
        sender: &relm4::Sender<ActionInputs>,
        then: ActionInputs,
        transient_for: &impl IsA<gtk::Window>,
    ) {
        if self.needs_saving {
            let question = create_message_dialog_skeleton(
                lang::lookup("action-save-before"),
                lang::lookup("action-save-before-message"),
                transient_for,
            );
            question.add_response("discard", &lang::lookup("discard"));
            question.add_response("save", &lang::lookup("save"));
            question.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
            question.set_default_response(Some("save"));
            question.set_close_response("discard");
            let sender_c = sender.clone();
            let then_c = then.clone();
            question.connect_response(Some("save"), move |_, _| {
                sender_c.emit(ActionInputs::SaveActionThen_(Box::new(then_c.clone())));
            });
            let sender_c = sender.clone();
            question.connect_response(Some("discard"), move |_, _| {
                sender_c.emit(then.clone());
            });
            question.set_visible(true);
        } else {
            sender.emit(then);
        }
    }

    /// Ask the user where to save the flow, or just save if that's good enough
    fn ask_where_to_save(
        &mut self,
        sender: &relm4::Sender<ActionInputs>,
        transient_for: &impl IsA<gtk::Window>,
        always_ask_where: bool,
        then: ActionInputs,
    ) {
        if always_ask_where || self.open_path.is_none() {
            // Ask where
            let dialog = gtk::FileDialog::builder()
                .modal(true)
                .title(lang::lookup("header-save"))
                .initial_folder(&gtk::gio::File::for_path(
                    testangel::action_loader::get_action_directory(),
                ))
                .filters(&file_filters::filter_list(&[
                    file_filters::actions(),
                    file_filters::all(),
                ]))
                .build();

            let sender_c = sender.clone();
            dialog.save(
                Some(transient_for),
                Some(&relm4::gtk::gio::Cancellable::new()),
                move |res| {
                    if let Ok(file) = res {
                        let path = file.path().unwrap();
                        sender_c.emit(ActionInputs::SaveActionThen__(
                            true,
                            path,
                            Box::new(then.clone()),
                        ));
                    }
                },
            );
        } else {
            sender.emit(ActionInputs::SaveActionThen__(
                false,
                self.open_path.clone().unwrap(),
                Box::new(then),
            ));
        }
    }

    /// Just save the action to disk with the current `open_path` as the destination
    fn save_action(
        &mut self,
        sender: &relm4::ComponentSender<ActionsModel>,
    ) -> Result<(), SaveOrOpenActionError> {
        // Get content
        let buffer = self.source_view.buffer();
        let script = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);

        // Update script
        let action = self.open_action.as_mut().unwrap();
        action.script = script.to_string();

        // Loop through all possible instruction luanames in the environment, then save a vector of which are used by this action
        action.required_instructions.clear();
        for engine in &**self.engine_list {
            let engine_lua_name = engine.lua_name.clone();
            for instruction in engine.instructions.clone() {
                let instruction_lua_name = instruction.lua_name().clone();
                let built_call = format!("{engine_lua_name}.{instruction_lua_name}");
                if script.contains(&built_call) {
                    action.required_instructions.push(instruction.id().clone());
                }
            }
        }

        let save_path = self.open_path.as_ref().unwrap();
        let data = ron::to_string(self.open_action.as_ref().unwrap())
            .map_err(SaveOrOpenActionError::SerializingError)?;
        fs::write(save_path, data).map_err(SaveOrOpenActionError::IoError)?;
        self.set_needs_saving(false, sender);
        Ok(())
    }

    /// Close this action without checking first
    fn close_action(&mut self, sender: &relm4::ComponentSender<ActionsModel>) {
        self.open_action = None;
        self.open_path = None;
        self.set_needs_saving(false, sender);
        self.header
            .emit(header::ActionsHeaderInput::ChangeActionOpen(
                self.open_action.is_some(),
            ));
    }
}

#[relm4::component(pub)]
impl Component for ActionsModel {
    type Init = (Arc<ActionMap>, Arc<EngineList>);
    type Input = ActionInputs;
    type Output = ActionOutputs;
    type CommandOutput = ();

    view! {
        #[root]
        toast_target = adw::ToastOverlay {
            if model.open_action.is_none() {
                adw::StatusPage {
                    set_title: &lang::lookup("nothing-open"),
                    set_description: Some(&lang::lookup("action-nothing-open-description")),
                    set_icon_name: Some(crate::ui::icon_names::LIGHTBULB),
                    set_vexpand: true,
                }
            } else {
                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hscrollbar_policy: gtk::PolicyType::Never,

                    #[local_ref]
                    source_view -> sourceview::View {
                        set_vexpand: true,

                        // Look and Feel
                        set_show_line_numbers: true,
                        set_monospace: true,

                        // Visual Spacing
                        set_pixels_below_lines: 2,
                        set_bottom_margin: 200,
                        set_wrap_mode: gtk::WrapMode::WordChar,

                        // Behaviour
                        set_indent_on_tab: true,
                        set_indent_width: 2,
                        set_insert_spaces_instead_of_tabs: true,
                        set_auto_indent: true,
                        set_smart_home_end: sourceview::SmartHomeEndType::Before,
                        set_smart_backspace: true,
                    },
                }
            }
        },
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let header = Rc::new(
            header::ActionsHeader::builder()
                .launch((init.1.clone(), init.0.clone()))
                .forward(sender.input_sender(), |msg| match msg {
                    header::ActionsHeaderOutput::NewAction => ActionInputs::NewAction,
                    header::ActionsHeaderOutput::OpenAction => ActionInputs::OpenAction,
                    header::ActionsHeaderOutput::SaveAction => ActionInputs::SaveAction,
                    header::ActionsHeaderOutput::SaveAsAction => ActionInputs::SaveAsAction,
                    header::ActionsHeaderOutput::CloseAction => {
                        ActionInputs::CloseActionThen(Box::new(ActionInputs::NoOp))
                    }
                    header::ActionsHeaderOutput::AddOpenActionToFlow => {
                        ActionInputs::AddOpenActionToFlow
                    }
                    header::ActionsHeaderOutput::AddStep(step) => ActionInputs::AddStep(step),
                }),
        );

        // Setup source view style manager
        StyleSchemeManager::default().append_search_path("styles");

        let mut model = ActionsModel {
            action_map: init.0,
            engine_list: init.1,
            open_action: None,
            open_path: None,
            needs_saving: false,
            header,
            signal_text_buffer_changed: None,
            source_view: sourceview::View::builder()
                .buffer(
                    &sourceview::Buffer::builder()
                        .highlight_syntax(true)
                        .language(
                            &sourceview::LanguageManager::builder()
                                .search_path(vec![
                                    "share/gtksourceview-5/language-specs/",             // Windows and Local
                                    "/usr/share/gtksourceview-5/language-specs/",        // Linux
                                    &std::env::var("GTKSV_LANGSPEC").unwrap_or_default() // Other environments
                                ])
                                .build()
                                .language("lua")
                                .expect("lua syntax highlighting not found - maybe use GTKSV_LANGSPEC to specify another search path?"),
                        )
                        .build(),
                )
                .build(),
        };

        let source_view = &model.source_view;
        let widgets = view_output!();

        {
            let sender = sender.clone();
            model.signal_text_buffer_changed =
                Some(source_view.buffer().connect_changed(move |_| {
                    sender.input(ActionInputs::TextBufferChanged);
                }));
        }

        // Code completion
        let completion = source_view.completion();
        let provider =
            completion_provider_engines::CompletionProviderEngines::new(model.engine_list.clone());
        completion.add_provider(&provider);
        let provider = completion_provider_descriptors::CompletionProviderDescriptors::new();
        completion.add_provider(&provider);
        let provider = completion_provider_instructions::CompletionProviderEngineInstructions::new(
            model.engine_list.clone(),
        );
        completion.add_provider(&provider);

        ComponentParts { model, widgets }
    }

    #[allow(clippy::too_many_lines)]
    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            ActionInputs::NoOp => (),

            ActionInputs::RequestProgramExit => {
                sender.output(ActionOutputs::RequestProgramExit).unwrap();
            }

            ActionInputs::AddOpenActionToFlow => {
                self.prompt_to_save(
                    sender.input_sender(),
                    ActionInputs::AddOpenActionToFlow_,
                    root.toplevel_window().as_ref().unwrap(),
                );
            }

            ActionInputs::AddOpenActionToFlow_ => {
                if let Some(action) = &self.open_action {
                    sender
                        .output(ActionOutputs::AddOpenActionToFlow(action.id.clone()))
                        .unwrap();
                }
            }

            ActionInputs::ActionsMapChanged(new_map) => {
                self.action_map = new_map.clone();
                self.header
                    .emit(header::ActionsHeaderInput::ActionsMapChanged(new_map));
            }

            ActionInputs::NewAction => {
                self.prompt_to_save(
                    sender.input_sender(),
                    ActionInputs::NewAction_,
                    root.toplevel_window().as_ref().unwrap(),
                );
            }
            ActionInputs::NewAction_ => {
                self.new_action(&sender);
            }
            ActionInputs::OpenAction => {
                self.prompt_to_save(
                    sender.input_sender(),
                    ActionInputs::OpenAction_,
                    root.toplevel_window().as_ref().unwrap(),
                );
            }
            ActionInputs::OpenAction_ => {
                let dialog = gtk::FileDialog::builder()
                    .modal(true)
                    .title(lang::lookup("header-open"))
                    .filters(&file_filters::filter_list(&[
                        file_filters::actions(),
                        file_filters::all(),
                    ]))
                    .initial_folder(&gtk::gio::File::for_path(
                        testangel::action_loader::get_action_directory(),
                    ))
                    .build();

                let sender_c = sender.clone();
                dialog.open(
                    Some(&root.toplevel_window().unwrap()),
                    Some(&relm4::gtk::gio::Cancellable::new()),
                    move |res| {
                        if let Ok(file) = res {
                            let path = file.path().unwrap();
                            sender_c.input(ActionInputs::OpenAction__(path));
                        }
                    },
                );
            }
            ActionInputs::OpenAction__(path) => {
                match self.open_action(path, &sender) {
                    Ok(()) => {
                        // Nothing more to do...
                    }
                    Err(e) => {
                        // Show error dialog
                        create_message_dialog(
                            lang::lookup("action-error-opening"),
                            e.to_string(),
                            root.toplevel_window().as_ref().unwrap(),
                        )
                        .set_visible(true);
                    }
                }
            }
            ActionInputs::SaveAction => {
                if self.open_action.is_some() {
                    // SAFETY: this cannot be triggered if not attached to a window
                    self.ask_where_to_save(
                        sender.input_sender(),
                        &root.toplevel_window().unwrap(),
                        false,
                        ActionInputs::NoOp,
                    );
                }
            }
            ActionInputs::SaveAsAction => {
                if self.open_action.is_some() {
                    // SAFETY: this cannot be triggered if not attached to a window
                    self.ask_where_to_save(
                        sender.input_sender(),
                        &root.toplevel_window().unwrap(),
                        true,
                        ActionInputs::NoOp,
                    );
                }
            }
            ActionInputs::SaveActionThen_(then) => {
                // SAFETY: this cannot be triggered if not attached to a window
                self.ask_where_to_save(
                    sender.input_sender(),
                    &root.toplevel_window().unwrap(),
                    false,
                    *then,
                );
            }
            ActionInputs::SaveActionThen__(new_id, path, then) => {
                self.open_path = Some(path.with_extension("taaction"));
                if new_id && let Some(action) = &mut self.open_action {
                    action.id = uuid::Uuid::new_v4().to_string();
                }
                if let Err(e) = self.save_action(&sender) {
                    create_message_dialog(
                        lang::lookup("action-error-saving"),
                        e.to_string(),
                        root.toplevel_window().as_ref().unwrap(),
                    )
                    .set_visible(true);
                } else {
                    widgets
                        .toast_target
                        .add_toast(adw::Toast::new(&lang::lookup("action-saved")));
                    sender.input_sender().emit(*then);
                }
                let _ = sender.output(ActionOutputs::ReloadActions);
            }
            ActionInputs::CloseActionThen(then) => {
                // Establish if needs_saving needs updating from text change
                if let Some(action) = &self.open_action {
                    let buf = self.source_view.buffer();
                    if action.script != buf.text(&buf.start_iter(), &buf.end_iter(), false) {
                        tracing::debug!("Needs saving due to text change.");
                        self.set_needs_saving(true, &sender);
                    }
                }

                self.prompt_to_save(
                    sender.input_sender(),
                    ActionInputs::CloseActionThen_(then),
                    root.toplevel_window().as_ref().unwrap(),
                );
            }
            ActionInputs::CloseActionThen_(then) => {
                self.close_action(&sender);
                sender.input(*then);
            }
            ActionInputs::TextBufferChanged => {
                self.set_needs_saving(true, &sender);
            }

            ActionInputs::AddStep(step_id) => {
                if self.open_action.is_none() {
                    self.new_action(&sender);
                }

                // SAFETY: the header can't ask to add an action that doesn't exist
                let engine = self
                    .engine_list
                    .get_engine_by_instruction_id(&step_id)
                    .unwrap();
                let instruction = self.engine_list.get_instruction_by_id(&step_id).unwrap();
                // Build LoC
                let mut params = String::new();
                for InstructionNamedKind { friendly_name, .. } in instruction.parameters() {
                    use convert_case::{Case, Casing};

                    // remove invalid chars
                    let mut sanitised_name = String::new();
                    for c in friendly_name.chars() {
                        if c.is_ascii_alphanumeric() || c.is_ascii_whitespace() {
                            sanitised_name.push(c);
                        }
                    }
                    params.push_str(&format!("{}, ", sanitised_name.to_case(Case::Snake)));
                }
                // remove last ", "
                let _ = params.pop();
                let _ = params.pop();

                let loc = if instruction.outputs().is_empty() {
                    format!("{}.{}({})", engine.lua_name, instruction.lua_name(), params)
                } else {
                    let mut returns = String::new();
                    for InstructionNamedKind { friendly_name, .. } in instruction.outputs() {
                        use convert_case::{Case, Casing};
                        returns.push_str(&format!("{}, ", friendly_name.to_case(Case::Snake)));
                    }

                    // Remove last ", "
                    let _ = returns.pop();
                    let _ = returns.pop();

                    format!(
                        "local {} = {}.{}({})",
                        returns,
                        engine.lua_name,
                        instruction.lua_name(),
                        params
                    )
                };
                // Append LoC
                let buffer = self.source_view.buffer();
                let text = buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), false)
                    .to_string();
                let mut newline_after = true;

                // Decide if cursor needs moving down a line (or into function)
                let cursor_pos = buffer.cursor_position();
                tracing::debug!(
                    "Inserting step, cursor pos: {} (text len: {})",
                    cursor_pos,
                    text.len()
                );
                if cursor_pos == 0 || cursor_pos == i32::try_from(text.len()).unwrap() {
                    // Move cursor into function
                    tracing::debug!("Offsetting cursor into function");
                    for (i, l) in text.lines().enumerate() {
                        if l.contains("function run_action") {
                            tracing::debug!("Function on line {}", i + 1);
                            if let Some(text_iter) =
                                buffer.iter_at_line_offset(i32::try_from(i).unwrap() + 1, 2)
                            {
                                buffer.place_cursor(&text_iter);
                            }
                        }
                    }
                } else {
                    // If line is not empty, add new line
                    let mut line_starts_at = 0;
                    let mut line_ends_at = text.len();
                    let mut line_num = 0;

                    for (idx, c) in text.char_indices() {
                        if c == '\n' {
                            if idx < usize::try_from(cursor_pos).unwrap() {
                                line_starts_at = idx + 1;
                                line_num += 1;
                            } else {
                                line_ends_at = idx;
                                break;
                            }
                        }
                    }

                    // Move cursor to end and insert newline if needed
                    let line = &text[line_starts_at..line_ends_at];
                    tracing::debug!("cursor on line: {:?}", line);
                    if !line.trim().is_empty() {
                        // Offset cursor to end of line
                        tracing::debug!(
                            "Moving cursor to end of line {} (pos {})",
                            line_num,
                            line.len()
                        );
                        if let Some(iter) =
                            buffer.iter_at_line_offset(line_num, i32::try_from(line.len()).unwrap())
                        {
                            buffer.place_cursor(&iter);
                            buffer.insert_at_cursor("\n  ");
                            newline_after = false;
                        }
                    }
                }

                buffer.insert_interactive_at_cursor(
                    &format!("{}{}", loc, if newline_after { "\n  " } else { "" }),
                    true,
                );

                self.set_needs_saving(true, &sender);
            }
        }
        self.update_view(widgets, sender);
    }
}

/// Create the absolute barebones of a message dialog, allowing for custom button and response mapping.
fn create_message_dialog_skeleton<S>(
    title: S,
    message: S,
    transient_for: &impl IsA<gtk::Window>,
) -> adw::MessageDialog
where
    S: AsRef<str>,
{
    adw::MessageDialog::builder()
        .transient_for(transient_for)
        .title(title.as_ref())
        .heading(title.as_ref())
        .body(message.as_ref())
        .modal(true)
        .build()
}

/// Create a message dialog attached to the toplevel window. This includes default implementations of an 'OK' button.
fn create_message_dialog<S>(
    title: S,
    message: S,
    transient_for: &impl IsA<gtk::Window>,
) -> adw::MessageDialog
where
    S: AsRef<str>,
{
    let dialog = create_message_dialog_skeleton(title, message, transient_for);
    dialog.add_response("ok", &lang::lookup("ok"));
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog
}
