#![deny(clippy::unwrap_used)]

//! This module is intended to group everything that's "intrinsic" of ADB.
//!
//! Following the design philosophy of most of Rust `std`,
//! `*Command` are intended to be "thin wrappers" (low-overhead abstractions)
//! around `adb_client`,
//! which implies:
//! - no "magic"
//! - no custom commands
//! - no chaining ("piping") of existing commands
//!
//! This guarantees a 1-to-1 mapping between methods and cmds,
//! thereby reducing surprises such as:
//! - Non-atomic operations: consider what happens if a pack changes state
//!   in the middle of listing enabled and disabled packs!
//! - Non-standard semantics: what would happen if a new ADB version
//!   supports a feature we already defined,
//!   but has _slightly_ different behavior?
//!
//! Despite being "low-level", we can still "have cake and eat it too";
//! After all, what's the point of an abstraction if it doesn't come with goodies?:
//! We can reserve some artistic license, such as:
//! - pre-parsing or validanting output, to provide types with invariants
//! - strongly-typed rather than "stringly-typed" APIs
//! - nicer IDE support
//! - compile-time prevention of malformed cmds
//! - implicit enforcement of a narrow set of operations
//!
//! About that last point, if there's ever a need for an ADB feature
//! which these APIs don't expose,
//! please, **PLEASE** refrain from falling-back to any `Command`-like API.
//! Rather, please extend these APIs in a consistent way.
//!
//! Thank you! ❤️
//!
//! For comprehensive info about ADB,
//! [see this](https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/master/docs/)

use adb_client::{ADBDeviceExt, ADBServer};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::io::Cursor;

use crate::utils::is_all_w_c;
use log::{error, info};

/// Convert ADB output bytes to a trimmed UTF-8 string.
/// Uses lossy conversion to prevent panics on non-UTF8 output from certain OEMs.
#[must_use]
pub fn to_trimmed_utf8(v: &[u8]) -> String {
    String::from_utf8_lossy(v).trim_end().to_string()
}

/// Internal state for `ACommand` - tracks the device serial to use
#[derive(Debug)]
struct ACommandState {
    device_serial: Option<String>,
}

/// Builder object for an Android Debug Bridge command,
/// using the type-state and new-type patterns.
///
/// This is not intended to model the entire ADB API.
/// It only models the subset that concerns UADNG.
///
/// [More info here](https://developer.android.com/tools/adb)
#[derive(Debug)]
pub struct ACommand(ACommandState);

impl ACommand {
    /// `adb` command builder
    #[must_use]
    pub fn new() -> Self {
        Self(ACommandState {
            device_serial: None,
        })
    }

    /// `shell` sub-command builder.
    ///
    /// If `device_serial` is empty, it lets ADB choose the default device.
    #[must_use]
    pub fn shell<S: AsRef<str>>(mut self, device_serial: S) -> ShellCommand {
        let serial = device_serial.as_ref();
        if !serial.is_empty() {
            self.0.device_serial = Some(serial.to_string());
        }
        ShellCommand(self)
    }

    /// Header-less list of attached devices (as serials) and their statuses:
    /// - USB
    /// - TCP/IP: WIFI, Ethernet, etc...
    /// - Local emulators
    ///
    /// Status can be (but not limited to):
    /// - "unauthorized"
    /// - "device"
    pub fn devices(self) -> Result<Vec<(String, String)>, String> {
        let mut server = ADBServer::default();
        server
            .devices()
            .map(|device_list| {
                device_list
                    .into_iter()
                    .map(|dev| (dev.identifier, dev.state.to_string()))
                    .collect()
            })
            .map_err(|e| {
                error!("ADB: {}", e);
                format!("Cannot connect to ADB server: {}", e)
            })
    }

    /// Execute a shell command via `adb_client`
    fn run_shell_command(&self, shell_command: &str) -> Result<String, String> {
        let mut server = ADBServer::default();

        // List available devices first
        let device_list = server
            .devices()
            .map_err(|e| format!("Cannot get device list: {}", e))?;

        if device_list.is_empty() {
            return Err("No ADB devices found. Please connect a device and try again.".to_string());
        }

        // Select device by serial if provided, otherwise use the first available device
        let target_serial = if let Some(ref serial) = self.0.device_serial {
            // Verify the device exists
            if !device_list.iter().any(|d| d.identifier == *serial) {
                return Err(format!(
                    "Device '{}' not found. Available devices: {}",
                    serial,
                    device_list
                        .iter()
                        .map(|d| d.identifier.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            Some(serial.clone())
        } else {
            // Check if we have exactly one device, warn if multiple
            if device_list.len() > 1 {
                info!(
                    "Multiple devices found ({}), using first device: {}",
                    device_list.len(),
                    device_list[0].identifier
                );
            }
            None
        };

        // Connect to the device
        let mut device = server.get_device().map_err(|e| {
            format!(
                "Cannot connect to device{}: {}",
                if let Some(ref ser) = target_serial {
                    format!(" '{}'", ser)
                } else {
                    String::new()
                },
                e
            )
        })?;

        // Create a writer to capture output
        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);

        // Split command by space to handle commands with arguments
        // This is a simple approach - for complex commands, they should be passed properly by the caller
        let command_parts: Vec<&str> = shell_command
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .collect();

        if command_parts.is_empty() {
            return Err("Empty shell command".to_string());
        }

        // Execute the shell command
        info!("Ran command: adb shell {}", shell_command);

        device
            .shell_command(&command_parts, &mut cursor)
            .map_err(|e| {
                error!("ADB shell command failed: {}", e);
                format!("Shell command failed: {}", e)
            })?;

        // Extract output from buffer
        let output = String::from_utf8_lossy(&buffer);
        let trimmed = output.trim_end().to_string();

        Ok(trimmed)
    }
}

impl Default for ACommand {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder object for a command that runs on the device's default `sh` implementation.
/// Typically MKSH, but could be Ash.
///
/// [More info](https://chromium.googlesource.com/aosp/platform/system/core/+/refs/heads/upstream/shell_and_utilities).
#[derive(Debug)]
pub struct ShellCommand(ACommand);

impl ShellCommand {
    /// `pm` command builder
    #[must_use]
    pub fn pm(self) -> PmCommand {
        PmCommand(self)
    }

    /// Query a device property value, by its key.
    /// These can be of any type:
    /// - `boolean`
    /// - `int`
    /// - chars
    /// - etc...
    ///
    /// So to avoid lossy conversions, we return strs
    pub fn getprop(self, key: &str) -> Result<String, String> {
        let command = format!("getprop {}", key);
        self.0.run_shell_command(&command)
    }

    /// Reboots device
    pub fn reboot(self) -> Result<String, String> {
        self.0.run_shell_command("reboot")
    }

    /// Execute an arbitrary shell action string on the device's default shell.
    /// The action string is passed as a single argument to `adb shell` and
    /// interpreted by the remote shell (which splits on spaces).
    pub fn raw(self, action: &str) -> Result<String, String> {
        self.0.run_shell_command(action)
    }
}

#[must_use]
pub const fn is_pkg_component(s: &[u8]) -> bool {
    if s.is_empty() {
        return false;
    }
    s[0].is_ascii_alphabetic()
        && if s.len() > 1 {
            is_all_w_c(s.split_at(1).1)
        } else {
            true
        }
}

/// String with the invariant of being a valid package-name.
/// See [`PackageId::new`] for validation details.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
pub struct PackageId(Box<str>);
impl PackageId {
    /// Creates a package-ID if it's valid according to
    /// <https://developer.android.com/build/configure-app-module#set-application-id>
    #[must_use]
    pub fn new(p_id: Box<str>) -> Option<Self> {
        let mut components = p_id.split('.');
        for _ in 0..2 {
            if !components
                .next()
                .is_some_and(|comp| is_pkg_component(comp.as_bytes()))
            {
                return None;
            }
        }
        if components.all(|comp| is_pkg_component(comp.as_bytes())) {
            Some(Self(p_id))
        } else {
            None
        }
    }
}

/// `pm list packages` flag/state/type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmListPacksFlag {
    /// `-u`, not to be confused with `-a`
    IncludeUninstalled,
    /// `-e`
    OnlyEnabled,
    /// `-d`
    OnlyDisabled,
}
impl PmListPacksFlag {
    // is there a trait for this?
    fn to_str(self) -> &'static str {
        match self {
            Self::IncludeUninstalled => "-u",
            Self::OnlyEnabled => "-e",
            Self::OnlyDisabled => "-d",
        }
    }
}
#[expect(clippy::to_string_trait_impl, reason = "This is not user-facing")]
impl ToString for PmListPacksFlag {
    fn to_string(&self) -> String {
        self.to_str().to_string()
    }
}

const PACK_PREFIX: &str = "package:";

pub const PM_CLEAR_PACK: &str = "pm clear";

/// Builder object for an Android Package Manager command.
/// <https://developer.android.com/tools/adb#pm>
#[derive(Debug)]
pub struct PmCommand(ShellCommand);
impl PmCommand {
    /// `list packages -s` sub-command, [`PACK_PREFIX`] stripped.
    ///
    /// `Ok` variant:
    /// - isn't guaranteed to contain valid pack-IDs,
    ///   as "android" can be printed but it's invalid
    /// - isn't sorted
    /// - duplicates never _seem_ to happen, but don't assume uniqueness
    pub fn list_packages_sys(
        self,
        f: Option<PmListPacksFlag>,
        user_id: Option<u16>,
    ) -> Result<Vec<String>, String> {
        let mut command = "pm list packages -s".to_string();
        if let Some(flag) = f {
            command.push(' ');
            command.push_str(flag.to_str());
        }
        if let Some(uid) = user_id {
            let _ = write!(&mut command, " --user {}", uid);
        }

        self.0.raw(&command).map(|pack_ls| {
            pack_ls
                .lines()
                .filter_map(|p_ln| {
                    p_ln.strip_prefix(PACK_PREFIX).map(|p| {
                        debug_assert!(PackageId::new(p.into()).is_some() || p == "android");
                        String::from(p)
                    })
                })
                .collect()
        })
    }

    /// `list users` sub-command, deserialized/parsed.
    ///
    /// - <https://source.android.com/docs/devices/admin/multi-user-testing>
    /// - <https://stackoverflow.com/questions/37495126/android-get-list-of-users-and-profile-name>
    pub fn list_users(self) -> Result<Box<[UserInfo]>, String> {
        Ok(self
            .0
            .raw("pm list users")?
            .lines()
            .skip(1) // omit header
            .filter_map(|ln| {
                // this could be optimized by making more API-stability assumptions
                let ln = ln.trim_ascii_start();
                let ln = ln.strip_prefix("UserInfo").unwrap_or(ln).trim_ascii_start();
                let ln = ln.strip_prefix('{').unwrap_or(ln).trim_ascii();
                //let run;
                let ln = if let Some(l) = ln.strip_suffix("running") {
                    //run = true;
                    l.trim_ascii_end()
                } else {
                    //run = false;
                    ln
                };
                let ln = ln.strip_suffix('}').unwrap_or(ln).trim_ascii_end();
                // https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/content/pm/UserInfo.java
                // The format looks stable today, but google may change it in future Android versions
                // (and very old Androids might differ). Keep parsing defensive.
                // Expected shape: "UserInfo{<id>:<name>:<flags>}[ running]"

                let mut comps = ln.split(':');

                let id = comps.next().and_then(|s| s.parse().ok())?;

                Some(UserInfo {
                    id,
                    //name: name.into(),
                    //flags,
                    //running: run,
                })
            })
            .collect())
    }
}

/// Mirror of AOSP `UserInfo` Java Class, with an extra field
#[derive(Debug, Clone)]
pub struct UserInfo {
    id: u16,
    //name: Box<str>,
    //flags: u32,
    //running: bool,
}
impl UserInfo {
    #[must_use]
    pub const fn get_id(&self) -> u16 {
        self.id
    }
    /*
    /// Check if the user was logged-in at the time `pm list users` was invoked
    #[must_use]
    #[allow(dead_code, reason = "Currently unused by UI; kept for future features")]
    pub const fn was_running(&self) -> bool {
        self.running
    }
    */
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_pack_ids() {
        for p_id in [
            "",
            "   ",
            ".",
            "nodots",
            "com..example",
            "net.hello.",
            "org.0example",
            "org._foobar",
            "the.🎂.is.a.lie",
            "EXCLAMATION!!!!",
        ] {
            assert_eq!(PackageId::new(p_id.into()), None);
        }
    }

    #[test]
    fn valid_pack_ids() {
        for p_id in [
            "A.a",
            "x.X",
            "org.example",
            "net.hello",
            "uwu.owo",
            "Am0Gu5.Zuz",
            "net.net.net.net.net.net.net.net.net.net.net",
            "com.github.w1nst0n",
            "this_.String_.is_.not_.real_",
        ] {
            assert_ne!(PackageId::new(p_id.into()), None);
        }
    }
}
