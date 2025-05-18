use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use adw::prelude::*;
use base64::{Engine, prelude::BASE64_STANDARD};
use evidenceangel::{Author, EvidencePackage};
use relm4::{Component, ComponentParts, RelmWidgetExt, adw, gtk};
use testangel::{
    action_loader::ActionMap,
    data_spreadsheet::load_data_spreadsheet,
    ipc::EngineList,
    types::{AutomationFlow, FlowError},
};
use testangel_ipc::prelude::{Evidence, EvidenceContent, ParameterValue};

use crate::{
    lang_args,
    ui::{file_filters, lang},
};

#[derive(Debug)]
pub enum ExecutionDialogCommandOutput {
    /// Execution completed with the resulting evidence
    Complete(Vec<Vec<Evidence>>),

    /// Execution failed at the given step and for the given reason
    Failed(usize, FlowError, Vec<Vec<Evidence>>),

    /// The data spreadsheet was invalud
    InvalidDataSpreadsheet(csv::Error),
}

#[derive(Debug)]
pub struct ExecutionDialogInit {
    pub flow: AutomationFlow,
    pub engine_list: Arc<EngineList>,
    pub action_map: Arc<ActionMap>,
    pub spreadsheet_path: Option<PathBuf>,
}

#[derive(Debug)]
pub enum ExecutionDialogInput {
    Close,
    FailedToGenerateEvidence(evidenceangel::Error),
    SaveEvidence(Vec<Vec<Evidence>>),
}

#[derive(Debug)]
pub struct ExecutionDialog;

/// Create the absolute barebones of a message dialog, allowing for custom button and response mapping.
fn create_message_dialog<S>(title: S, message: S) -> adw::MessageDialog
where
    S: AsRef<str>,
{
    adw::MessageDialog::builder()
        .title(title.as_ref())
        .heading(title.as_ref())
        .body(message.as_ref())
        .modal(true)
        .build()
}

fn add_evidence(mut evp: EvidencePackage, all_case_evidence: Vec<Vec<Evidence>>) -> evidenceangel::Result<()> {
    for evidence in all_case_evidence {
        let tc = evp.create_test_case("TestAngel Test Case")?;
        let tc_evidence = tc.evidence_mut();
        for ev in evidence {
            let Evidence { label, content } = ev;
            let mut ea_ev = match content {
                EvidenceContent::Textual(text) => evidenceangel::Evidence::new(
                    evidenceangel::EvidenceKind::Text,
                    evidenceangel::EvidenceData::Text { content: text },
                ),
                EvidenceContent::ImageAsPngBase64(base64) => evidenceangel::Evidence::new(
                    evidenceangel::EvidenceKind::Image,
                    evidenceangel::EvidenceData::Base64 {
                        data: BASE64_STANDARD
                            .decode(base64)
                            .map_err(|e| evidenceangel::Error::OtherExportError(Box::new(e)))?,
                    },
                ),
            };
            if !label.is_empty() {
                ea_ev.set_caption(Some(label));
            }
            tc_evidence.push(ea_ev);
        }
    }
    evp.save()?;
    Ok(())
}

#[relm4::component(pub)]
impl Component for ExecutionDialog {
    type Init = ExecutionDialogInit;
    type Input = ExecutionDialogInput;
    type Output = ();
    type CommandOutput = ExecutionDialogCommandOutput;

    view! {
        #[root]
        adw::Window {
            set_modal: true,
            set_resizable: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 5,
                set_margin_all: 50,

                gtk::Spinner {
                    set_spinning: true,
                },
                gtk::Label {
                    set_label: &lang::lookup("flow-execution-running"),
                },
            },
        },
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        let model = ExecutionDialog;
        let widgets = view_output!();
        let flow = init.flow;
        let engine_list = init.engine_list.clone();
        let action_map = init.action_map.clone();

        sender.spawn_oneshot_command(move || {
            let spreadsheet_data = if let Some(path) = init.spreadsheet_path {
                load_data_spreadsheet(path).map_err(ExecutionDialogCommandOutput::InvalidDataSpreadsheet)
            } else {
                Ok(vec![HashMap::new()])
            };
            if let Err(e) = spreadsheet_data {
                return e;
            }
            let spreadsheet_data = spreadsheet_data.unwrap();

            let mut all_runs_evidence = vec![];
            for spreadsheet_row in spreadsheet_data {
                let mut outputs: Vec<HashMap<usize, ParameterValue>> = Vec::new();
                let mut evidence = Vec::new();

                for engine in &**engine_list {
                    if engine.reset_state().is_err() {
                        evidence.push(Evidence {
                            label: String::from("WARNING: State Warning"),
                            content: EvidenceContent::Textual(String::from("For this test execution, the state couldn't be correctly reset. Some results may not be accurate."))
                        });
                    }
                }

                for (step, action_config) in flow.actions.iter().enumerate() {
                    tracing::debug!("Output state: {outputs:?}");
                    tracing::debug!("Evidence state: {evidence:?}");
                    tracing::debug!("Executing: {action_config:?}");
                    match action_config.execute(
                        &action_map,
                        &engine_list,
                        &outputs,
                        &spreadsheet_row,
                    ) {
                        Ok((output, ev)) => {
                            outputs.push(output);
                            evidence = [evidence, ev].concat();
                        }
                        Err((e, ev)) => {
                            evidence = [evidence, ev].concat();
                            all_runs_evidence.push(evidence);
                            return ExecutionDialogCommandOutput::Failed(step + 1, e, all_runs_evidence);
                        }
                    }
                }
                all_runs_evidence.push(evidence);
            }

            ExecutionDialogCommandOutput::Complete(all_runs_evidence)
        });

        ComponentParts { model, widgets }
    }

    fn update(
        &mut self,
        message: Self::Input,
        sender: relm4::ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            ExecutionDialogInput::Close => root.destroy(),
            ExecutionDialogInput::FailedToGenerateEvidence(reason) => {
                let dialog = create_message_dialog(
                    lang::lookup("evidence-failed"),
                    lang::lookup_with_args(
                        "evidence-failed-message",
                        lang_args!("reason", reason.to_string()),
                    ),
                );
                dialog.set_transient_for(Some(root));
                dialog.add_response("ok", &lang::lookup("ok"));
                dialog.set_default_response(Some("ok"));
                let sender_c = sender.clone();
                dialog.connect_response(None, move |dlg, _response| {
                    sender_c.input(ExecutionDialogInput::Close);
                    dlg.close();
                });
                dialog.set_visible(true);
            }
            ExecutionDialogInput::SaveEvidence(evidence) => {
                // Present save dialog
                let dialog = gtk::FileDialog::builder()
                    .modal(true)
                    .title(lang::lookup("evidence-save-title"))
                    .initial_name(lang::lookup("evidence-default-name"))
                    .filters(&file_filters::filter_list(&[
                        file_filters::evps(),
                        file_filters::all(),
                    ]))
                    .build();

                let sender_c = sender.clone();
                dialog.save(
                    Some(root),
                    Some(&relm4::gtk::gio::Cancellable::new()),
                    move |res| {
                        if let Ok(file) = res {
                            let path = file.path().unwrap().with_extension("evp");
                            match fs::exists(&path) {
                                Ok(exists) => {
                                    let evp = if exists {
                                        // Open
                                        EvidencePackage::open(path)
                                    } else {
                                        // Create
                                        EvidencePackage::new(
                                            path,
                                            "TestAngel Evidence".to_string(),
                                            vec![Author::new("Anonymous Author")],
                                        )
                                    };

                                    match evp {
                                        Err(e) => {
                                            tracing::warn!(
                                                "Failed to create/open output file: {e}"
                                            );
                                            sender_c.input(
                                                ExecutionDialogInput::FailedToGenerateEvidence(e),
                                            );
                                            return;
                                        }
                                        Ok(evp) => {
                                            // Append new TC
                                            if let Err(e) = add_evidence(evp, evidence.clone()) {
                                                sender_c.input(
                                                    ExecutionDialogInput::FailedToGenerateEvidence(
                                                        e,
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to check if output file exists: {e}");
                                }
                            }
                        }
                        sender_c.input(ExecutionDialogInput::Close);
                    },
                );
            }
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: relm4::ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            ExecutionDialogCommandOutput::Complete(evidence) => {
                tracing::info!("Execution complete.");
                sender.input(ExecutionDialogInput::SaveEvidence(evidence));
            }

            ExecutionDialogCommandOutput::InvalidDataSpreadsheet(reason) => {
                tracing::warn!("Data spreadsheet invalid: {reason:?}");
                let dialog = create_message_dialog(
                    lang::lookup("flow-execution-failed"),
                    lang::lookup_with_args(
                        "flow-execution-failed-data",
                        lang_args!("reason", reason.to_string()),
                    ),
                );
                dialog.set_transient_for(Some(root));
                dialog.add_response("ok", &lang::lookup("ok"));
                dialog.set_default_response(Some("ok"));
                let sender_c = sender.clone();
                dialog.connect_response(None, move |dlg, response| match response {
                    _ => {
                        sender_c.input(ExecutionDialogInput::Close);
                        dlg.close();
                    }
                });
                dialog.set_visible(true);
            }

            ExecutionDialogCommandOutput::Failed(step, reason, evidence) => {
                tracing::warn!("Execution failed. Evidence: {evidence:?}");
                let dialog = create_message_dialog(
                    lang::lookup("flow-execution-failed"),
                    lang::lookup_with_args(
                        "flow-execution-failed-message",
                        lang_args!("step", step, "reason", reason.to_string()),
                    ),
                );
                dialog.set_transient_for(Some(root));
                if !evidence.is_empty() {
                    dialog
                        .add_response("save", &lang::lookup("flow-execution-save-evidence-anyway"));
                }
                dialog.add_response("copy", &lang::lookup("copy-ok"));
                dialog.add_response("ok", &lang::lookup("ok"));
                dialog.set_default_response(Some("ok"));
                let sender_c = sender.clone();
                dialog.connect_response(None, move |dlg, response| match response {
                    "copy" => {
                        if let Some(display) = gtk::gdk::Display::default() {
                            display.clipboard().set_text(reason.to_string().as_str());
                        } else {
                            tracing::warn!(
                                "No display is present, so no clipboard could be accessed!"
                            );
                        }
                        sender_c.input(ExecutionDialogInput::Close);
                        dlg.close();
                    }
                    "save" => {
                        sender_c.input(ExecutionDialogInput::SaveEvidence(evidence.clone()));
                    }
                    _ => {
                        sender_c.input(ExecutionDialogInput::Close);
                        dlg.close();
                    }
                });
                dialog.set_visible(true);
            }
        }
    }
}
