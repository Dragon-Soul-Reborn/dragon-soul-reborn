//! Shared logic for the Dragon Soul Reborn launcher and recipe-tool. No Tauri and no UI: everything here is
//! plain Rust that tests drive directly.
pub mod apply;
pub mod aup;
pub mod client_login;
pub mod display;
pub mod extract;
pub mod feed;
pub mod fetch;
pub mod gameconfig;
pub mod hash;
pub mod inspect;
pub mod launch;
pub mod model;
pub mod paths;
pub mod plan;
pub mod progress;
pub mod recipe_build;
pub mod routine;
pub mod signature;
pub mod state;
