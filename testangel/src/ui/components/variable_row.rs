use std::fmt::Debug;
use std::marker::PhantomData;

use adw::prelude::*;
use relm4::{
    Component, ComponentController, Controller, FactorySender, adw, factory::FactoryVecDeque, gtk,
    prelude::FactoryComponent,
};
use testangel_ipc::prelude::{ParameterKind, ParameterValue};

use crate::lang_args;
use crate::ui::{
    components::literal_input::{LiteralInput, LiteralInputOutput},
    lang,
};

#[derive(Debug)]
pub struct VariableRow<PS, T, I>
where
    PS: Debug + Clone + 'static,
    I: VariableRowParentInput<T, PS>,
{
    idx: T,
    name: String,
    kind: ParameterKind,
    source: PS,
    value: ParameterValue,

    literal_input: Controller<LiteralInput>,
    potential_sources_raw: Vec<(String, PS)>,
    potential_sources: FactoryVecDeque<SourceSearchResult<PS>>,
    _input_marker: PhantomData<I>,
}

pub struct VariableRowInit<T, PS>
where
    PS: ParameterSourceTrait + Debug + std::fmt::Display + PartialEq<PS> + Clone + 'static,
{
    pub index: T,
    pub name: String,
    pub kind: ParameterKind,
    pub current_source: PS,
    pub current_value: ParameterValue,
    pub potential_sources: Vec<(String, PS)>,
}

pub trait VariableRowParentInput<T, PS> {}

pub trait ParameterSourceTrait {
    fn literal() -> Self;
    fn data_spreadsheet(column: String) -> Self;
    fn is_from_data_spreadsheet(&self) -> bool;
}

impl<PS: PartialEq<PS> + ToString + Clone + Debug, T, I: VariableRowParentInput<T, PS>>
    VariableRow<PS, T, I>
{
    fn get_nice_name_for(&self, source: &PS) -> String {
        for (name, src) in &self.potential_sources_raw {
            if *src == *source {
                return name.clone();
            }
        }

        source.to_string()
    }
}

#[derive(Debug)]
pub enum VariableRowInput<PS> {
    SourceSelected(PS, bool),
    ChangeValue(ParameterValue),
}

#[derive(Debug)]
pub enum VariableRowOutput<T, PS> {
    NewSourceFor(T, PS),
    NewValueFor(T, ParameterValue),
}

#[relm4::factory(pub)]
impl<PS, I, T> FactoryComponent for VariableRow<PS, T, I>
where
    PS: ParameterSourceTrait + Debug + std::fmt::Display + Send + PartialEq<PS> + Clone + 'static,
    I: Debug + VariableRowParentInput<T, PS> + 'static,
    T: Clone + Debug + 'static,
{
    type Init = VariableRowInit<T, PS>;
    type Input = VariableRowInput<PS>;
    type Output = VariableRowOutput<T, PS>;
    type CommandOutput = ();
    type ParentWidget = adw::PreferencesGroup;

    view! {
        adw::ActionRow {
            set_title: &self.name,
            #[watch]
            set_subtitle: &if self.source == PS::literal() {
                lang::lookup_with_args(
                    "variable-row-subtitle-with-value",
                    lang_args!("kind", lang::lookup(match self.kind {
                            ParameterKind::String => "kind-string",
                            ParameterKind::Integer => "kind-integer",
                            ParameterKind::Decimal => "kind-decimal",
                            ParameterKind::Boolean => "kind-boolean",
                        }), "source", self.source.to_string(),
                        "value", self.value.to_string())
                )
            } else {
                lang::lookup_with_args(
                    "variable-row-subtitle",
                    lang_args!("kind", lang::lookup(match self.kind {
                            ParameterKind::String => "kind-string",
                            ParameterKind::Integer => "kind-integer",
                            ParameterKind::Decimal => "kind-decimal",
                            ParameterKind::Boolean => "kind-boolean",
                        }))
                )
            },
            set_use_markup: false,

            add_suffix = &gtk::Box {
                set_spacing: 15,
                set_orientation: gtk::Orientation::Horizontal,

                if self.source == PS::literal() {
                    adw::Bin {
                        self.literal_input.widget(),
                    }
                } else {
                    gtk::Label {
                        #[watch]
                        set_label: &self.get_nice_name_for(&self.source),
                    }
                },

                gtk::MenuButton {
                    set_icon_name: relm4_icons::icon_names::EDIT,
                    set_tooltip_text: Some(&lang::lookup("variable-row-edit-param")),
                    set_css_classes: &["flat"],
                    set_direction: gtk::ArrowType::Left,

                    #[wrap(Some)]
                    #[name = "popover"]
                    set_popover = &gtk::Popover {
                        gtk::ScrolledWindow {
                            set_hscrollbar_policy: gtk::PolicyType::Never,
                            set_min_content_height: 150,

                            gtk::Box {
                                set_spacing: 5,
                                set_orientation: gtk::Orientation::Vertical,

                                gtk::Button {
                                    set_label: &lang::lookup("source-literal"),
                                    add_css_class: "flat",
                                    connect_clicked => VariableRowInput::SourceSelected(PS::literal(), true),
                                },
                                #[name = "data_column"]
                                gtk::Entry {
                                    set_placeholder_text: Some(&lang::lookup("source-data")),
                                    connect_changed[sender] => move |entry| {
                                        sender.input(VariableRowInput::SourceSelected(PS::data_spreadsheet(entry.text().to_string()), false));
                                    } @data_column_changed,
                                },
                                #[local_ref]
                                potential_sources -> gtk::Box {
                                    set_spacing: 5,
                                    set_orientation: gtk::Orientation::Vertical,
                                },
                            }
                        }
                    },
                },
            },
        }
    }

    fn init_model(
        init: Self::Init,
        _index: &Self::Index,
        sender: relm4::FactorySender<Self>,
    ) -> Self {
        let mut potential_sources = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |v| VariableRowInput::SourceSelected(v, true));
        {
            // populate sources
            let mut potential_sources = potential_sources.guard();
            for (label, source) in init.potential_sources.clone() {
                potential_sources.push_back((label, source));
            }
        }

        let literal_input = LiteralInput::builder()
            .launch(init.current_value.clone())
            .forward(sender.input_sender(), |msg| match msg {
                LiteralInputOutput::ValueChanged(new_value) => {
                    VariableRowInput::ChangeValue(new_value)
                }
            });

        Self {
            idx: init.index,
            name: init.name,
            kind: init.kind,
            source: init.current_source,
            value: init.current_value,
            literal_input,
            potential_sources_raw: init.potential_sources,
            potential_sources,
            _input_marker: PhantomData,
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let potential_sources = self.potential_sources.widget();
        let widgets = view_output!();
        widgets
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: FactorySender<Self>,
    ) {
        match message {
            VariableRowInput::SourceSelected(new_source, close) => {
                self.source = new_source.clone();
                if close {
                    widgets.popover.popdown();
                }

                if !new_source.is_from_data_spreadsheet() {
                    widgets.data_column.block_signal(&widgets.data_column_changed);
                    widgets.data_column.set_text("");
                    widgets.data_column.unblock_signal(&widgets.data_column_changed);
                }

                sender
                    .output(VariableRowOutput::NewSourceFor(
                        self.idx.clone(),
                        new_source,
                    ))
                    .unwrap();
            }
            VariableRowInput::ChangeValue(new_value) => {
                self.value = new_value.clone();
                sender
                    .output(VariableRowOutput::NewValueFor(self.idx.clone(), new_value))
                    .unwrap();
            }
        }
        self.update_view(widgets, sender);
    }
}

#[derive(Debug)]
struct SourceSearchResult<PS> {
    label: String,
    source: PS,
}

#[derive(Debug)]
enum SourceSearchResultInput {
    Select,
}

#[relm4::factory]
impl<PS: Debug + Clone + 'static> FactoryComponent for SourceSearchResult<PS> {
    type Init = (String, PS);
    type Input = SourceSearchResultInput;
    type Output = PS;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        #[root]
        gtk::Button {
            set_label: &self.label,
            add_css_class: "flat",
            connect_clicked => SourceSearchResultInput::Select,
        }
    }

    fn init_model(init: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        Self {
            label: init.0,
            source: init.1,
        }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            SourceSearchResultInput::Select => sender.output(self.source.clone()).unwrap(),
        }
    }
}
