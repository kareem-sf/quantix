#[cfg(all(windows, feature = "runtime-fixture"))]
pub mod connections;
#[cfg(all(windows, not(feature = "runtime-fixture")))]
#[allow(dead_code)]
pub(crate) mod connections;
pub mod contract;
#[cfg(all(windows, feature = "runtime-fixture"))]
pub mod vault;
#[cfg(all(windows, not(feature = "runtime-fixture")))]
#[allow(dead_code)]
pub(crate) mod vault;
#[cfg(windows)]
pub mod windows_dpapi;
