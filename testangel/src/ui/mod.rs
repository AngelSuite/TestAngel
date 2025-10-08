use std::{fmt, rc::Rc, sync::Arc};

use gtk::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, Controller, RelmApp, actions::RelmActionGroup,
    adw, gtk,
};
use testangel::{
    action_loader::{self, ActionMap},
    ipc::{self, EngineList},
};

use self::header_bar::HeaderBarInput;

mod about;
mod actions;
mod components;
mod file_filters;
mod flows;
mod header_bar;
pub(crate) mod lang;

mod icon_names {
    pub use custom::*;
    pub use shipped::*;
    include!(concat!(env!("OUT_DIR"), "/icon_names.rs"));
}

/// Initialise and open the UI.
pub fn initialise_ui() {
    tracing::info!("Starting Next UI...");
    let app = RelmApp::new("uk.hpkns.testangel");
    relm4_icons::initialize_icons(icon_names::GRESOURCE_BYTES, icon_names::RESOURCE_PREFIX);

    let display = gtk::gdk::Display::default().unwrap();
    let theme = gtk::IconTheme::for_display(&display);
    theme.add_resource_path("/uk/hpkns/testangel/icons/");
    theme.add_resource_path("/uk/hpkns/testangel/icons/scalable/actions/");

    sourceview5::init();

    let engines = Arc::new(ipc::get_engines());
    let actions = Arc::new(action_loader::get_actions(&engines));
    app.run::<AppModel>(AppInit { engines, actions });
}

pub struct AppInit {
    engines: Arc<EngineList>,
    actions: Arc<ActionMap>,
}

#[derive(Debug)]
enum AppInput {
    /// The view has changed and should be read from `visible_child_name`, then components updated as needed.
    ChangedView(Option<String>),
    /// The actions might have changed and should be reloaded
    ReloadActionsMap,
    /// Attach the action group to the window
    AttachGeneralActionGroup(RelmActionGroup<header_bar::GeneralActionGroup>),
    /// Attach the action group to the window
    AttachFileActionGroup(RelmActionGroup<header_bar::FileActionGroup>),
    /// Add the given action to the flow
    AddActionToFlow(String),
    /// Set a page needs attention
    SetPageNeedsSaving(&'static str, bool),
    /// Check and then close the application
    CheckAndCloseProgram,
}

struct AppModel {
    stack: Rc<adw::ViewStack>,
    header: Controller<header_bar::HeaderBarModel>,

    flows: Controller<flows::FlowsModel>,
    actions: Controller<actions::ActionsModel>,

    engines_list: Arc<EngineList>,
    actions_map: Arc<ActionMap>,

    flow_needs_saving: bool,
    action_needs_saving: bool,
}

impl fmt::Debug for AppModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppModel")
            .field("engines_list", &self.engines_list)
            .field("actions_map", &self.actions_map)
            .field("flow_needs_saving", &self.flow_needs_saving)
            .field("action_needs_saving", &self.action_needs_saving)
            .finish_non_exhaustive()
    }
}

#[relm4::component]
impl Component for AppModel {
    type Init = AppInit;
    type Input = AppInput;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        main_window = adw::ApplicationWindow {
            set_title: Some(&lang::lookup("app-name")),
            set_default_width: 800,
            set_default_height: 600,

            connect_close_request[sender] => move |_| {
                sender.input(AppInput::CheckAndCloseProgram);
                gtk::glib::Propagation::Stop
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,

                model.header.widget(),

                #[local_ref]
                stack -> adw::ViewStack {
                    connect_visible_child_name_notify[sender] => move |st| {
                        sender.input(AppInput::ChangedView(st.visible_child_name().map(Into::into)));
                    },
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        // Initialise the sub-components (pages)
        let flows = flows::FlowsModel::builder()
            .launch((init.actions.clone(), init.engines.clone()))
            .forward(sender.input_sender(), |msg| match msg {
                flows::FlowOutputs::RequestProgramExit => AppInput::CheckAndCloseProgram,
                flows::FlowOutputs::SetNeedsSaving(needs_saving) => {
                    AppInput::SetPageNeedsSaving("flows", needs_saving)
                }
            });
        let actions = actions::ActionsModel::builder()
            .launch((init.actions.clone(), init.engines.clone()))
            .forward(sender.input_sender(), |msg| match msg {
                actions::ActionOutputs::RequestProgramExit => AppInput::CheckAndCloseProgram,
                actions::ActionOutputs::ReloadActions => AppInput::ReloadActionsMap,
                actions::ActionOutputs::SetNeedsSaving(needs_saving) => {
                    AppInput::SetPageNeedsSaving("actions", needs_saving)
                }
                actions::ActionOutputs::AddOpenActionToFlow(action_id) => {
                    AppInput::AddActionToFlow(action_id)
                }
            });

        let stack = Rc::new(adw::ViewStack::new());
        gtk::Window::set_default_icon_name("testangel");

        // Initialise the headerbar
        let header = header_bar::HeaderBarModel::builder()
            .launch((
                actions.model().header_controller_rc(),
                flows.model().header_controller_rc(),
                stack.clone(),
                init.engines.clone(),
                init.actions.clone(),
            ))
            .forward(sender.input_sender(), |msg| match msg {
                header_bar::HeaderBarOutput::AttachGeneralActionGroup(group) => {
                    AppInput::AttachGeneralActionGroup(group)
                }
                header_bar::HeaderBarOutput::AttachFileActionGroup(group) => {
                    AppInput::AttachFileActionGroup(group)
                }
            });

        // Build model
        let model = AppModel {
            actions_map: init.actions,
            engines_list: init.engines,
            stack,
            header,
            flows,
            actions,
            flow_needs_saving: false,
            action_needs_saving: false,
        };

        // Render window parts
        let stack = &*model.stack;

        // Add pages
        stack.add_titled_with_icon(
            model.flows.widget(),
            Some("flows"),
            &lang::lookup("tab-flows"),
            crate::ui::icon_names::PAPYRUS_VERTICAL,
        );
        if !std::env::var("TA_HIDE_ACTION_EDITOR")
            .unwrap_or("no".to_string())
            .eq_ignore_ascii_case("yes")
        {
            stack.add_titled_with_icon(
                model.actions.widget(),
                Some("actions"),
                &lang::lookup("tab-actions"),
                crate::ui::icon_names::PUZZLE_PIECE,
            );
        }

        let widgets = view_output!();
        tracing::debug!("Initialised model: {model:?}");

        // Trigger initial header bar update
        sender.input(AppInput::ChangedView(
            stack.visible_child_name().map(Into::into),
        ));

        ComponentParts { model, widgets }
    }

    fn update(
        &mut self,
        message: Self::Input,
        _sender: relm4::ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            AppInput::CheckAndCloseProgram => {
                if self.action_needs_saving {
                    // Deal with that
                    self.stack.set_visible_child_name("actions");
                    // Here we explicitly tell the header bar to change NOW to
                    // make sure it's mounted before we show a dialog.
                    self.actions
                        .emit(actions::ActionInputs::CloseActionThen(Box::new(
                            actions::ActionInputs::RequestProgramExit,
                        )));
                } else if self.flow_needs_saving {
                    // Deal with that
                    self.stack.set_visible_child_name("flows");
                    self.flows.emit(flows::FlowInputs::CloseFlowThen(Box::new(
                        flows::FlowInputs::RequestProgramExit,
                    )));
                } else {
                    relm4::main_application().quit();
                }
            }
            AppInput::AttachGeneralActionGroup(group) => {
                group.register_for_widget(root);
            }
            AppInput::AttachFileActionGroup(group) => {
                group.register_for_widget(root);
            }
            AppInput::ChangedView(new_view) => {
                self.header
                    .emit(HeaderBarInput::ChangedView(new_view.unwrap_or_default()));
            }
            AppInput::AddActionToFlow(action_id) => {
                self.stack.set_visible_child_name("flows");
                self.flows.emit(flows::FlowInputs::AddStep(action_id));
            }
            AppInput::SetPageNeedsSaving(page, needs_saving) => {
                match page {
                    "flows" => self.flow_needs_saving = needs_saving,
                    "actions" => self.action_needs_saving = needs_saving,
                    _ => (),
                }
                if let Some(page) = self.stack.child_by_name(page) {
                    self.stack.page(&page).set_needs_attention(needs_saving);
                }
            }
            AppInput::ReloadActionsMap => {
                self.actions_map = Arc::new(action_loader::get_actions(&self.engines_list));
                self.flows.emit(flows::FlowInputs::ActionsMapChanged(
                    self.actions_map.clone(),
                ));
                self.actions.emit(actions::ActionInputs::ActionsMapChanged(
                    self.actions_map.clone(),
                ));
                self.header
                    .emit(header_bar::HeaderBarInput::ActionsMapChanged(
                        self.actions_map.clone(),
                    ));
            }
        }
    }
}
