use glib::prelude::*;
use gtk::subclass::prelude::*;
use relm4::gtk::{self, gio::ListModel, glib};
use sourceview5::{CompletionProvider, prelude::TextBufferExt, subclass::prelude::*};

use crate::ui::actions::completion_proposal_list::CompletionProposalListModel;

use super::item::DescriptorCompletionProposal;

#[derive(Default)]
pub struct CompletionProviderDescriptors;

#[glib::object_subclass]
impl ObjectSubclass for CompletionProviderDescriptors {
    const NAME: &'static str = "TestAngelCompletionProviderDescriptors";
    type Type = super::CompletionProviderDescriptors;
    type ParentType = glib::Object;
    type Interfaces = (CompletionProvider,);
}

impl ObjectImpl for CompletionProviderDescriptors {}

impl CompletionProviderImpl for CompletionProviderDescriptors {
    fn activate(
        &self,
        context: &sourceview5::CompletionContext,
        proposal: &sourceview5::CompletionProposal,
    ) {
        if let Ok(proposal) = proposal.clone().downcast::<DescriptorCompletionProposal>() {
            if let Some((mut start, mut end)) = context.bounds() {
                let buffer = start.buffer();
                let descriptor = proposal.descriptor();
                let mut len_to_insert = descriptor.len();
                let mut end_mark = None;

                // If the insertion cursor is within a word and the trailing
                // characters of the word match the suffix of the proposal, then
                // limit how much text we insert so that the word is completed
                // properly.
                if !end.ends_line() && !end.char().is_whitespace() && !end.ends_word() {
                    let mut word_end = end;
                    if word_end.forward_word_end() {
                        let text = end.slice(&word_end).to_string();

                        if descriptor.ends_with(&text) {
                            assert!(descriptor.len() >= text.len());
                            len_to_insert = descriptor.len() - text.len();
                            end_mark = Some(buffer.create_mark(None, &word_end, false));
                        }
                    }
                }

                buffer.begin_user_action();
                buffer.delete(&mut start, &mut end);
                buffer.insert(&mut start, &descriptor[0..len_to_insert]);
                buffer.end_user_action();

                if let Some(end_mark) = end_mark {
                    let new_end = buffer.iter_at_mark(&end_mark);
                    buffer.select_range(&new_end, &new_end);
                    buffer.delete_mark(&end_mark);
                }
            }
        }
    }

    fn display(
        &self,
        _context: &sourceview5::CompletionContext,
        proposal: &sourceview5::CompletionProposal,
        cell: &sourceview5::CompletionCell,
    ) {
        if let Ok(proposal) = proposal.clone().downcast::<DescriptorCompletionProposal>() {
            match cell.column() {
                sourceview5::CompletionColumn::Icon => {
                    cell.set_icon_name(relm4_icons::icon_names::TAG);
                }
                sourceview5::CompletionColumn::Before | sourceview5::CompletionColumn::After => {
                    cell.set_text(None);
                }
                sourceview5::CompletionColumn::TypedText => {
                    cell.set_text(Some(&proposal.descriptor()));
                }
                sourceview5::CompletionColumn::Comment => {
                    cell.set_text(proposal.documentation().lines().next());
                }
                sourceview5::CompletionColumn::Details => {
                    cell.set_text(Some(&proposal.documentation()));
                }
                _ => (),
            }
        }
    }

    fn title(&self) -> Option<glib::GString> {
        None
    }

    fn priority(&self, _context: &sourceview5::CompletionContext) -> i32 {
        0
    }

    fn is_trigger(&self, _iter: &gtk::TextIter, c: char) -> bool {
        [':'].contains(&c)
    }

    fn key_activates(
        &self,
        _context: &sourceview5::CompletionContext,
        _proposal: &sourceview5::CompletionProposal,
        keyval: gtk::gdk::Key,
        _state: gtk::gdk::ModifierType,
    ) -> bool {
        [gtk::gdk::Key::space].contains(&keyval)
    }

    fn refilter(&self, context: &sourceview5::CompletionContext, model: &gtk::gio::ListModel) {
        let word = context.word().to_string();
        if let Ok(model) = model.clone().downcast::<CompletionProposalListModel>() {
            model.retain(|item| {
                item.clone()
                    .downcast::<DescriptorCompletionProposal>()
                    .is_ok_and(|item| {
                        item.descriptor()
                            .to_ascii_lowercase()
                            .starts_with(&word.to_ascii_lowercase())
                    })
            });
        }
    }

    fn list_alternates(
        &self,
        _context: &sourceview5::CompletionContext,
        _proposal: &sourceview5::CompletionProposal,
    ) -> Vec<sourceview5::CompletionProposal> {
        vec![]
    }

    fn populate_future(
        &self,
        _context: &sourceview5::CompletionContext,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<gtk::gio::ListModel, glib::Error>>>> {
        Box::pin(async move {
            let list = CompletionProposalListModel::new();
            list.append(DescriptorCompletionProposal::new(
                "param Integer",
                "An integer parameter",
            ));
            list.append(DescriptorCompletionProposal::new(
                "param Decimal",
                "A decimal parameter",
            ));
            list.append(DescriptorCompletionProposal::new(
                "param Boolean",
                "An boolean (yes/no) parameter",
            ));
            list.append(DescriptorCompletionProposal::new(
                "param Text",
                "A text parameter",
            ));
            list.append(DescriptorCompletionProposal::new(
                "return Integer",
                "An integer return value",
            ));
            list.append(DescriptorCompletionProposal::new(
                "return Decimal",
                "A decimal return value",
            ));
            list.append(DescriptorCompletionProposal::new(
                "return Boolean",
                "An boolean (yes/no) return value",
            ));
            list.append(DescriptorCompletionProposal::new(
                "return Text",
                "A text return value",
            ));
            list.append(DescriptorCompletionProposal::new(
                "name",
                "Specify the name of this action",
            ));
            list.append(DescriptorCompletionProposal::new(
                "group",
                "Specify the group this action should be displayed as part of",
            ));
            list.append(DescriptorCompletionProposal::new(
                "creator",
                "Specify the creator of this action",
            ));
            list.append(DescriptorCompletionProposal::new(
                "description",
                "Describe the purpose of this action",
            ));
            list.append(DescriptorCompletionProposal::new(
                "hide-in-flow-editor",
                "Hide this action in the flow editor.\nIt is only possible to add it with the 'Add to flow' button in the top left once this is done",
            ));
            Ok(list.upcast::<ListModel>())
        })
    }
}
