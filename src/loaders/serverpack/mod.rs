pub mod addons;
pub mod archive;
pub mod curseforge;
pub mod detector;
pub mod installer;

pub use addons::*;
#[allow(unused_imports)]
pub use archive::*;
pub use curseforge::*;
#[allow(unused_imports)]
pub use detector::*;
pub use installer::*;

#[cfg(test)]
mod tests;
