use std::cell::RefCell;

use glib::subclass::prelude::*;
use relm4::gtk::glib;
use sourceview5::{CompletionProposal, subclass::prelude::CompletionProposalImpl};

#[derive(Debug, Default)]
pub struct DescriptorCompletionProposal {
    pub(super) descriptor: RefCell<String>,
    pub(super) documentation: RefCell<String>,
}

#[glib::object_subclass]
impl ObjectSubclass for DescriptorCompletionProposal {
    const NAME: &'static str = "TestAngelDescriptorCompletionProposal";
    type Type = super::DescriptorCompletionProposal;
    type ParentType = glib::Object;
    type Interfaces = (CompletionProposal,);
}

impl ObjectImpl for DescriptorCompletionProposal {}

impl CompletionProposalImpl for DescriptorCompletionProposal {}
