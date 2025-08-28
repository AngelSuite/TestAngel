use std::cell::RefCell;

use relm4::gtk::{gio, glib, prelude::*, subclass::prelude::*};
use sourceview5::CompletionProposal;

#[derive(Debug, Default)]
pub struct CompletionProposalListModel {
    pub(super) inner: RefCell<Vec<CompletionProposal>>,
}

/// Basic declaration of our type for the `GObject` type system
#[glib::object_subclass]
impl ObjectSubclass for CompletionProposalListModel {
    const NAME: &'static str = "TestAngelCompletionProposalListModel";
    type Type = super::CompletionProposalListModel;
    type ParentType = glib::Object;
    type Interfaces = (gio::ListModel,);
}

impl ObjectImpl for CompletionProposalListModel {}

impl ListModelImpl for CompletionProposalListModel {
    fn item_type(&self) -> glib::Type {
        CompletionProposal::static_type()
    }

    fn n_items(&self) -> u32 {
        u32::try_from(self.inner.borrow().len()).unwrap()
    }

    fn item(&self, position: u32) -> Option<glib::Object> {
        self.inner
            .borrow()
            .get(position as usize)
            .map(|o| o.clone().upcast::<glib::Object>())
    }
}
