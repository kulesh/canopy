pub mod application;
pub mod domain;
pub mod error;
pub mod inference;
pub mod infrastructure;
pub mod tui;

pub use application::config::AppConfig;
pub use application::runner::run;
pub use error::{CanopyError, Result};
