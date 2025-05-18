use glib::subclass::prelude::*;
use relm4::gtk::glib::{self, property::PropertySet};
use sourceview5::CompletionProposal;

mod imp;

glib::wrapper! {
    pub struct DescriptorCompletionProposal(ObjectSubclass<imp::DescriptorCompletionProposal>)
        @implements CompletionProposal;
}

impl DescriptorCompletionProposal {
    /// Create a new proposal.
    pub fn new<S>(descriptor: S, documentation: S) -> Self
    where
        S: Into<String>,
    {
        let o: DescriptorCompletionProposal = glib::Object::builder().build();
        o.imp().descriptor.set(descriptor.into());
        o.imp().documentation.set(documentation.into());
        o
    }

    /// Get the descriptor from this proposal.
    pub fn descriptor(&self) -> String {
        let imp::DescriptorCompletionProposal { descriptor, .. } = self.imp();
        descriptor.borrow().clone()
    }

    /// Get the documentation from this proposal.
    pub fn documentation(&self) -> String {
        let imp::DescriptorCompletionProposal { documentation, .. } = self.imp();
        documentation.borrow().clone()
    }
}
