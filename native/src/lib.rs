pub mod codex_profiles;
pub mod config;
pub mod db;
pub mod engines;
pub mod fs_ops;
pub mod git;
pub mod locale;
pub mod models;
pub mod native_app;
pub mod path_utils;
pub mod power;
pub mod process_utils;
pub mod runtime_env;
pub mod workspace_startup;

pub fn run() {
    native_app::run();
}
