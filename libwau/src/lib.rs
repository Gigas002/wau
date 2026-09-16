pub mod catalogue;
pub mod config;
pub mod error;
pub mod fs;
pub mod http;
pub mod lockfile;
pub mod matchers;
pub mod model;
pub mod pkg_archives;
pub mod pkg_management;
pub mod progress;
pub mod results;
pub mod sources;
pub mod toc;

pub use error::Error;

pub type Result<T> = std::result::Result<T, Error>;
