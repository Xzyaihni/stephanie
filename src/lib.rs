#![allow(clippy::suspicious_else_formatting)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::new_without_default)]
#![allow(clippy::needless_update)]
// the fact that i can derive it is a coincidence
#![allow(clippy::derivable_impls)]
// this is so stupid
#![allow(clippy::len_without_is_empty)]
// collapsed ones r way less readable in most cases :/
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::single_match)]
#![allow(clippy::needless_lifetimes)]
// skill issue
#![allow(clippy::type_complexity)]
// ITS MORE DESCRIPTIVE OF WUT IT IS
#![allow(clippy::let_and_return)]
// consistency????????
#![allow(clippy::excessive_precision)]

use std::{process, fmt::Display};

use nalgebra::Vector3;

pub mod common;

pub mod server;
pub mod client;

pub mod main_menu;

pub mod app;

pub mod debug_config;


pub const LOG_PATH: &str = "log.txt";
pub const LONGEST_FRAME: f64 = 1.0 / 20.0;

pub const BACKGROUND_COLOR: Vector3<f32> = Vector3::new(0.831, 0.941, 0.988);

pub fn complain(message: impl Display) -> !
{
    eprintln!("{message}");

    process::exit(1)
}
