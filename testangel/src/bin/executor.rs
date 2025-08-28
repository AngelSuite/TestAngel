#![warn(clippy::pedantic)]

use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use base64::{Engine, prelude::BASE64_STANDARD};
use clap::{Parser, arg};
use evp::{Author, EvidencePackage};
use testangel::{
    action_loader, data_spreadsheet::load_data_spreadsheet, ipc, types::AutomationFlow,
};
use testangel_ipc::prelude::*;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Parser)]

struct Cli {
    /// The data spreadsheet (CSV) to refer to for this flow
    #[arg(short, long)]
    data_spreadsheet: Option<PathBuf>,

    /// The output file for evidence. If this already exists then it will be appended.
    #[arg(short, long, default_value = "evidence.evp")]
    output: PathBuf,

    /// The flow file to execute.
    #[arg(index = 1)]
    flow: PathBuf,
}

fn main() {
    tracing_subscriber::FmtSubscriber::new().init();

    let cli = Cli::parse();

    let flow: AutomationFlow =
        ron::from_str(&fs::read_to_string(cli.flow).expect("Failed to read flow."))
            .expect("Failed to parse flow.");
    let engine_map = Arc::new(ipc::get_engines());
    let action_map = Arc::new(action_loader::get_actions(&engine_map));

    // Check flow for actions that aren't available.
    for action_config in &flow.actions {
        if action_map
            .get_action_by_id(&action_config.action_id)
            .is_none()
        {
            eprintln!(
                "This flow cannot be executed because an action isn't available or wasn't loaded. Maybe an engine is missing?"
            );
            std::process::exit(1);
        }
    }

    let spreadsheet_data = if let Some(path) = cli.data_spreadsheet {
        load_data_spreadsheet(path).expect("Failed to parse data spreadsheet")
    } else {
        vec![HashMap::new()]
    };

    match fs::exists(&cli.output) {
        Ok(exists) => {
            let evp = if exists {
                // Open
                EvidencePackage::open(cli.output)
            } else {
                // Create
                EvidencePackage::new(
                    cli.output,
                    "TestAngel Evidence".to_string(),
                    vec![Author::new("Anonymous Author")],
                )
            };

            if let Err(e) = &evp {
                eprintln!("Failed to create/open output file: {e}");
            }
            let mut evp = evp.unwrap();

            for spreadsheet_row in spreadsheet_data {
                // Append new TC
                let flow_ev = run_flow(flow.clone(), &engine_map, &action_map, &spreadsheet_row);
                if let Err(e) = add_evidence(&mut evp, flow_ev) {
                    eprintln!("Failed to write evidence: {e}");
                }
            }
        }
        Err(e) => eprintln!("Failed to check if output file exists: {e}"),
    }
}

fn run_flow(
    flow: AutomationFlow,
    engine_map: &Arc<ipc::EngineList>,
    action_map: &Arc<action_loader::ActionMap>,
    spreadsheet_row: &HashMap<String, ParameterValue>,
) -> Vec<Evidence> {
    let mut outputs: Vec<HashMap<usize, ParameterValue>> = Vec::new();
    let mut evidence = Vec::new();

    for engine in &***engine_map {
        if engine.reset_state().is_err() {
            evidence.push(Evidence {
                label: String::from("WARNING: State Warning"),
                content: EvidenceContent::Textual(String::from("For this test execution, the state couldn't be correctly reset. Some results may not be accurate."))
            });
        }
    }

    for action_config in flow.actions {
        match action_config.execute(action_map, engine_map, &outputs, spreadsheet_row) {
            Ok((output, ev)) => {
                outputs.push(output);
                evidence = [evidence, ev].concat();
            }
            Err((e, _ev)) => {
                panic!("Failed to execute: {e}");
            }
        }
    }

    evidence
}

fn add_evidence(evp: &mut EvidencePackage, evidence: Vec<Evidence>) -> evp::Result<()> {
    let tc = evp.create_test_case("TestAngel Test Case")?;
    let tc_evidence = tc.evidence_mut();
    for ev in evidence {
        let Evidence { label, content } = ev;
        let mut ea_ev = match content {
            EvidenceContent::Textual(text) => evp::Evidence::new(
                evp::EvidenceKind::Text,
                evp::EvidenceData::Text { content: text },
            ),
            EvidenceContent::ImageAsPngBase64(base64) => evp::Evidence::new(
                evp::EvidenceKind::Image,
                evp::EvidenceData::Base64 {
                    data: BASE64_STANDARD
                        .decode(base64)
                        .map_err(|e| evp::Error::OtherExportError(Box::new(e)))?,
                },
            ),
            EvidenceContent::HttpRequestResponse(req, res) => evp::Evidence::new(
                evp::EvidenceKind::Http,
                evp::EvidenceData::Base64 {
                    data: format!("{req}\x1e{res}").into_bytes(),
                },
            ),
        };
        if !label.is_empty() {
            ea_ev.set_caption(Some(label));
        }
        tc_evidence.push(ea_ev);
    }
    evp.save()?;
    Ok(())
}
