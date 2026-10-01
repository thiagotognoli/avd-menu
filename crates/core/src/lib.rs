//! Núcleo do AVD Menu: faz o que o `sdkmanager`, o `avdmanager` e o Device
//! Manager do Android Studio fazem, sem Java.

#[macro_use]
pub mod error;
#[macro_use]
pub mod lang;
pub mod api;
pub mod avd;
pub mod cancel;
pub mod cli;
pub mod config;
pub mod devices;
pub mod emu;
pub mod platform;
pub mod sdk;
pub mod shortcut;
pub mod tasks;
#[cfg(test)]
pub(crate) mod testutil;

pub use cancel::Cancel;
pub use error::{Error, Result};
