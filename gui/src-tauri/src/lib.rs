use std::sync::RwLock;

use clap::Parser;
use anyhow::Result;

use app_state::{AppState, PDGConfig};
use graph_building::make_dpdg;
use graph_interaction::{get_n_timeslots, get_partial_graph, toggle_module, set_new_head, reset_head};

mod argument_parsing;
mod errors;
mod graph_building;
mod app_state;
mod graph_interaction;
mod translation;

#[tauri::command]
fn get_initial_route() -> String {
    "/loading_screen".into()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<()> {
    let args = argument_parsing::Args::parse().validate()?;
    let mut state = AppState::new();
    state.pdg_config = Some(PDGConfig { criterion: args.slice_criterion,
        pdg_path: args.pdg_path.into(),
        vcd_path: args.vcd_path.into(),
        hgldd_path: args.hgldd_path.into(),
        top_module: args.top_module,
        extra_scopes: args.extra_scopes.unwrap_or(vec![]),
        max_timesteps: args.max_timesteps,
        data_only: args.data_only.unwrap_or(false),
        group_nodes: args.hier_grouping.unwrap_or(false),
        fir_repr: args.fir.unwrap_or(false)
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(RwLock::new(state))
        .invoke_handler(tauri::generate_handler![get_initial_route, make_dpdg, get_n_timeslots, get_partial_graph, toggle_module, set_new_head, reset_head])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
    Ok(())
}
