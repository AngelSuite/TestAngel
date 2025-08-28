use std::{cell::RefCell, sync::Arc};

use convert_case::Casing;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use glib::prelude::*;
use gtk::subclass::prelude::*;
use relm4::gtk::{self, gio::ListModel, glib};
use sourceview5::{CompletionProvider, prelude::TextBufferExt, subclass::prelude::*};
use testangel::ipc::EngineList;

use crate::ui::actions::completion_proposal_list::{CompletionProposalListModel, ProposalSource};

use super::item::EngineInstructionCompletionProposal;

#[derive(Default)]
pub struct CompletionProviderEngineInstructions {
    pub(super) engine_list: RefCell<Arc<EngineList>>,
}

#[glib::object_subclass]
impl ObjectSubclass for CompletionProviderEngineInstructions {
    const NAME: &'static str = "TestAngelCompletionProviderEngineInstructions";
    type Type = super::CompletionProviderEngineInstructions;
    type ParentType = glib::Object;
    type Interfaces = (CompletionProvider,);
}

impl ObjectImpl for CompletionProviderEngineInstructions {}

impl CompletionProviderImpl for CompletionProviderEngineInstructions {
    fn activate(
        &self,
        context: &sourceview5::CompletionContext,
        proposal: &sourceview5::CompletionProposal,
    ) {
        if let Ok(proposal) = proposal
            .clone()
            .downcast::<EngineInstructionCompletionProposal>()
            && let Some((mut start, mut end)) = context.bounds()
        {
            let buffer = start.buffer();
            let instruction_lua_name = proposal.instruction_lua_name();
            let mut len_to_insert = instruction_lua_name.len();

            // Move start iter to beginning of engine name
            start.backward_word_start();

            // Determine if we should add `local _ =`
            let mut start_of_line = start;
            while !start_of_line.starts_line() {
                start_of_line.backward_char();
            }
            let need_to_create_variable = buffer
                .slice(&start_of_line, &start, false)
                .to_string()
                .trim()
                .is_empty();

            // If the insertion cursor is within a word and the trailing
            // characters of the word match the suffix of the proposal, then
            // limit how much text we insert so that the word is completed
            // properly.
            if !end.ends_line() && !end.char().is_whitespace() && !end.ends_word() {
                let mut word_end = end;
                if word_end.forward_word_end() {
                    let text = end.slice(&word_end).to_string();

                    if instruction_lua_name.ends_with(&text) {
                        assert!(instruction_lua_name.len() >= text.len());
                        len_to_insert = instruction_lua_name.len() - text.len();
                    }
                }
            }

            buffer.begin_user_action();
            buffer.delete(&mut start, &mut end);

            let param_list = proposal.parameters().join(", ");
            let insertion_text = format!(
                "{}{}.{}({})",
                if need_to_create_variable && !proposal.returns().is_empty() {
                    format!("local {} = ", proposal.returns().join(", "),)
                } else {
                    String::new()
                },
                proposal.engine_lua_name(),
                &instruction_lua_name[0..len_to_insert],
                param_list,
            );
            buffer.insert(&mut start, &insertion_text);
            buffer.end_user_action();

            // Focus first parameter if needed
            if !proposal.parameters().is_empty() {
                // At this point, `start` is actually *after* the newly inserted text.
                let mut start_of_params = start;

                // Move cursor back by param_list + 1
                start_of_params.backward_chars(i32::try_from(param_list.len()).unwrap() + 1);

                // Select region from insert_end to (insert_end + len(first param))
                let mut after_first_param = start_of_params;
                after_first_param.forward_chars(
                    i32::try_from(proposal.parameters().first().unwrap().len()).unwrap(),
                );
                buffer.select_range(&start_of_params, &after_first_param);
            }
        }
    }

    fn display(
        &self,
        _context: &sourceview5::CompletionContext,
        proposal: &sourceview5::CompletionProposal,
        cell: &sourceview5::CompletionCell,
    ) {
        if let Ok(proposal) = proposal
            .clone()
            .downcast::<EngineInstructionCompletionProposal>()
        {
            match cell.column() {
                sourceview5::CompletionColumn::Icon => {
                    cell.set_icon_name(relm4_icons::icon_names::PUZZLE_PIECE);
                }
                sourceview5::CompletionColumn::Before => {
                    cell.set_text(Some(&format!("{}.", proposal.engine_lua_name())));
                }
                sourceview5::CompletionColumn::TypedText => {
                    let params = proposal.parameters();
                    let returns = proposal.returns();
                    cell.set_text(Some(&format!(
                        "{}{}",
                        if params.is_empty() {
                            format!("{}()", proposal.instruction_lua_name())
                        } else {
                            format!("{}({})", proposal.instruction_lua_name(), params.join(", "))
                        },
                        if returns.is_empty() {
                            String::new()
                        } else {
                            format!(" -> {}", returns.join(", "))
                        }
                    )));
                }
                sourceview5::CompletionColumn::After => {
                    cell.set_text(None);
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
        1
    }

    fn is_trigger(&self, _iter: &gtk::TextIter, c: char) -> bool {
        c == '.'
    }

    fn key_activates(
        &self,
        _context: &sourceview5::CompletionContext,
        _proposal: &sourceview5::CompletionProposal,
        keyval: gtk::gdk::Key,
        _state: gtk::gdk::ModifierType,
    ) -> bool {
        [gtk::gdk::Key::Tab, gtk::gdk::Key::parenleft].contains(&keyval)
    }

    fn refilter(&self, context: &sourceview5::CompletionContext, model: &gtk::gio::ListModel) {
        let word = context.word().to_string();
        if let Ok(model) = model.clone().downcast::<CompletionProposalListModel>() {
            model.retain(|item| {
                item.clone()
                    .downcast::<EngineInstructionCompletionProposal>()
                    .is_ok_and(|item| {
                        item.instruction_lua_name()
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

        let context = context.clone();
        Box::pin(async move {
            let engines = engines_arc;
            let list = CompletionProposalListModel::new();
            if let Some((mut start, end)) = context.bounds() {
                let word = context.word().to_string();
                let buffer = start.buffer();
                // Move to start of engine name
                start.backward_word_start();

                let line = buffer.slice(&start, &end, false).to_string();
                if let Some(engine_lua_name) = line.trim().split('.').next() {
                    let matcher = SkimMatcherV2::default();
                    for engine in &**engines {
                        if engine.lua_name.eq_ignore_ascii_case(engine_lua_name) {
                            for instruction in &engine.instructions {
                                let proposal = EngineInstructionCompletionProposal::new(
                                    engine.lua_name.clone(),
                                    instruction.lua_name().clone(),
                                    instruction.description().clone(),
                                    instruction
                                        .parameters()
                                        .iter()
                                        .map(|p| {
                                            p.friendly_name()
                                                .chars()
                                                .filter(|c| {
                                                    c.is_ascii_alphanumeric()
                                                        || c.is_ascii_whitespace()
                                                })
                                                .collect::<String>()
                                                .to_case(convert_case::Case::Snake)
                                        })
                                        .collect(),
                                    instruction
                                        .outputs()
                                        .iter()
                                        .map(|p| {
                                            p.friendly_name()
                                                .chars()
                                                .filter(|c| {
                                                    c.is_ascii_alphanumeric()
                                                        || c.is_ascii_whitespace()
                                                })
                                                .collect::<String>()
                                                .to_case(convert_case::Case::Snake)
                                        })
                                        .collect(),
                                );

                                if instruction
                                    .lua_name()
                                    .to_ascii_lowercase()
                                    .starts_with(&word.to_ascii_lowercase())
                                {
                                    proposal.set_source(ProposalSource::Exact);
                                    list.append(proposal);
                                } else if let Some(score) = matcher.fuzzy_match(
                                    &instruction.lua_name().to_ascii_lowercase(),
                                    &word.to_ascii_lowercase(),
                                ) {
                                    proposal.set_source(ProposalSource::Fuzzy { score });
                                    list.append(proposal);
                                }
                            }
                        }
                    }
                }
            }

            list.sort(|prop1, prop2| {
                if let Ok(prop1) = prop1
                    .clone()
                    .downcast::<EngineInstructionCompletionProposal>()
                    && let Ok(prop2) = prop2
                        .clone()
                        .downcast::<EngineInstructionCompletionProposal>()
                {
                    // Sort by source first, then alphabetical
                    return match prop1.source().cmp(&prop2.source()) {
                        std::cmp::Ordering::Equal => prop1
                            .instruction_lua_name()
                            .cmp(&prop2.instruction_lua_name()),
                        ord => ord,
                    };
                }
                std::cmp::Ordering::Equal
            });
            Ok(list.upcast::<ListModel>())
        })
    }
}
