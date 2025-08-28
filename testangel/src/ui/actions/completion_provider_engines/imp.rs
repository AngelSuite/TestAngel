use std::{cell::RefCell, sync::Arc};

use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use glib::prelude::*;
use gtk::subclass::prelude::*;
use relm4::gtk::{self, gio::ListModel, glib};
use sourceview5::{CompletionProvider, prelude::TextBufferExt, subclass::prelude::*};
use testangel::ipc::EngineList;

use crate::ui::actions::completion_proposal_list::{CompletionProposalListModel, ProposalSource};

use super::item::EngineCompletionProposal;

#[derive(Default)]
pub struct CompletionProviderEngines {
    pub(super) engine_list: RefCell<Arc<EngineList>>,
}

#[glib::object_subclass]
impl ObjectSubclass for CompletionProviderEngines {
    const NAME: &'static str = "TestAngelCompletionProviderEngines";
    type Type = super::CompletionProviderEngines;
    type ParentType = glib::Object;
    type Interfaces = (CompletionProvider,);
}

impl ObjectImpl for CompletionProviderEngines {}

impl CompletionProviderImpl for CompletionProviderEngines {
    fn activate(
        &self,
        context: &sourceview5::CompletionContext,
        proposal: &sourceview5::CompletionProposal,
    ) {
        if let Ok(proposal) = proposal.clone().downcast::<EngineCompletionProposal>()
            && let Some((mut start, mut end)) = context.bounds()
        {
            let buffer = start.buffer();
            let engine_lua_name = proposal.engine_lua_name();
            let mut len_to_insert = engine_lua_name.len();
            let mut end_mark = None;

            // If the insertion cursor is within a word and the trailing
            // characters of the word match the suffix of the proposal, then
            // limit how much text we insert so that the word is completed
            // properly.
            if !end.ends_line() && !end.char().is_whitespace() && !end.ends_word() {
                let mut word_end = end;
                if word_end.forward_word_end() {
                    let text = end.slice(&word_end).to_string();

                    if engine_lua_name.ends_with(&text) {
                        assert!(engine_lua_name.len() >= text.len());
                        len_to_insert = engine_lua_name.len() - text.len();
                        end_mark = Some(buffer.create_mark(None, &word_end, false));
                    }
                }
            }

            buffer.begin_user_action();
            buffer.delete(&mut start, &mut end);
            buffer.insert(&mut start, &engine_lua_name[0..len_to_insert]);
            buffer.end_user_action();

            if let Some(end_mark) = end_mark {
                let new_end = buffer.iter_at_mark(&end_mark);
                buffer.select_range(&new_end, &new_end);
                buffer.delete_mark(&end_mark);
            }
        }
    }

    fn display(
        &self,
        _context: &sourceview5::CompletionContext,
        proposal: &sourceview5::CompletionProposal,
        cell: &sourceview5::CompletionCell,
    ) {
        if let Ok(proposal) = proposal.clone().downcast::<EngineCompletionProposal>() {
            match cell.column() {
                sourceview5::CompletionColumn::Icon => {
                    cell.set_icon_name(relm4_icons::icon_names::GEAR);
                }
                sourceview5::CompletionColumn::Before | sourceview5::CompletionColumn::After => {
                    cell.set_text(None);
                }
                sourceview5::CompletionColumn::TypedText => {
                    cell.set_text(Some(&proposal.engine_lua_name()));
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
        ['='].contains(&c) || c.is_alphabetic()
    }

    fn key_activates(
        &self,
        _context: &sourceview5::CompletionContext,
        _proposal: &sourceview5::CompletionProposal,
        keyval: gtk::gdk::Key,
        _state: gtk::gdk::ModifierType,
    ) -> bool {
        [gtk::gdk::Key::Tab, gtk::gdk::Key::period].contains(&keyval)
    }

    fn refilter(&self, context: &sourceview5::CompletionContext, model: &gtk::gio::ListModel) {
        let word = context.word().to_string();
        if let Ok(model) = model.clone().downcast::<CompletionProposalListModel>() {
            model.retain(|item| {
                item.clone()
                    .downcast::<EngineCompletionProposal>()
                    .is_ok_and(|item| {
                        item.engine_lua_name()
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
        context: &sourceview5::CompletionContext,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<gtk::gio::ListModel, glib::Error>>>> {
        let engines_box = self.engine_list.borrow();
        let engines_arc = (*engines_box).clone();
        let word = context.word().to_string();

        Box::pin(async move {
            let engines = engines_arc;
            let list = CompletionProposalListModel::new();

            let matcher = SkimMatcherV2::default();
            for engine in &**engines {
                if engine
                    .lua_name
                    .to_ascii_lowercase()
                    .starts_with(&word.to_ascii_lowercase())
                {
                    list.append(EngineCompletionProposal::new(engine, ProposalSource::Exact));
                } else if let Some(score) = matcher.fuzzy_match(
                    &engine.lua_name.to_ascii_lowercase(),
                    &word.to_ascii_lowercase(),
                ) {
                    list.append(EngineCompletionProposal::new(
                        engine,
                        ProposalSource::Fuzzy { score },
                    ));
                }
            }

            list.sort(|prop1, prop2| {
                if let Ok(prop1) = prop1.clone().downcast::<EngineCompletionProposal>()
                    && let Ok(prop2) = prop2.clone().downcast::<EngineCompletionProposal>()
                {
                    // Sort by source first, then alphabetical
                    return match prop1.source().cmp(&prop2.source()) {
                        std::cmp::Ordering::Equal => {
                            prop1.engine_lua_name().cmp(&prop2.engine_lua_name())
                        }
                        ord => ord,
                    };
                }
                std::cmp::Ordering::Equal
            });
            Ok(list.upcast::<ListModel>())
        })
    }
}
