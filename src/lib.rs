use std::{process, fmt::Display};

use nalgebra::Vector3;

pub mod common;

pub mod server;
pub mod client;

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
