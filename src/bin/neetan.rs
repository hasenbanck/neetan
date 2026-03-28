#![forbid(unsafe_code)]

use common::{error, info, log::Level};
use neetan::{
    CARGO_PKG_VERSION, GAME_NAME,
    config::{Action, parse_args},
};

#[cfg(debug_assertions)]
const DEFAULT_LOG_LEVEL: Level = Level::Debug;

#[cfg(not(debug_assertions))]
const DEFAULT_LOG_LEVEL: Level = Level::Info;

fn main() {
    common::log::initialize_logger(DEFAULT_LOG_LEVEL, vec![]);

    let action = match parse_args() {
        Ok(action) => action,
        Err(error) => {
            error!("{error:#}");
            std::process::exit(1);
        }
    };

    match action {
        Action::Run(config) => {
            info!("{GAME_NAME}");
            info!("Build version: {CARGO_PKG_VERSION}");

            if let Err(error) = neetan::run(*config) {
                error!("Error while executing the emulator: {error:#}");
                std::process::exit(1);
            }
        }
        Action::CreateFdd { path, fdd_type } => {
            if let Err(error) = neetan::create::create_fdd_image(&path, fdd_type) {
                error!("{error:#}");
                std::process::exit(1);
            }
        }
        Action::CreateHdd { path, hdd_type } => {
            if let Err(error) = neetan::create::create_hdd_image(&path, hdd_type) {
                error!("{error:#}");
                std::process::exit(1);
            }
        }
        Action::ConvertHdd { input, output } => {
            if let Err(error) = neetan::convert::convert_hdd_image(&input, &output) {
                error!("{error:#}");
                std::process::exit(1);
            }
        }
    }
}
