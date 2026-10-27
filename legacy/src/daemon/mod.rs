pub mod core;
pub mod traits;

#[cfg(target_os = "linux")]
pub mod linux;

pub mod fallback; // Always available for fallback

pub use traits::DaemonError;
pub use traits::DaemonManager;

#[cfg(target_os = "linux")]
pub use linux::LinuxDaemonManager as PlatformDaemonManager;

#[cfg(not(target_os = "linux"))]
pub use fallback::FallbackDaemonManager as PlatformDaemonManager;
pub mod client;
