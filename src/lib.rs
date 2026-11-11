pub mod book;
pub mod settings;

#[cfg(windows)]
pub mod native;

pub mod preferences;

pub mod hotkeys;

#[cfg(windows)]
pub mod compat;

#[cfg(windows)]
mod legacy_text;
