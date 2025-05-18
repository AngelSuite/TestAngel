use std::fmt::Debug;

use gtk::prelude::*;
use relm4::gtk;
use relm4::prelude::*;

pub struct AddStepInit {
    pub label: String,
    pub value: String,
}

#[derive(Debug)]
pub struct AddStepResult {
    label: String,
    value: String,
}

impl AddStepResult {
    /// Get the instruction ID this result references
    pub fn value(&self) -> String {
        self.value.clone()
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for AddStepResult {
    type Init = AddStepInit;
    type Input = ();
    type Output = String;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        root = gtk::Button {
            set_label: &self.label,
            add_css_class: "flat",

            connect_clicked[sender, id] => move |_| {
                sender.output(id.clone()).unwrap();
            }
        }
    }

    fn init_model(init: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        Self {
            label: init.label,
            value: init.value,
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let id = self.value.clone();
        let widgets = view_output!();
        widgets
    }
}
