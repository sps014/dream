use super::{Platform, capture, sdk};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::process::Command;

#[derive(Debug, Serialize)]
struct Device {
    platform: Platform,
    id: String,
    name: String,
    state: String,
}

#[derive(Serialize)]
struct Unavailable {
    platform: Platform,
    reason: String,
}

#[derive(Default, Serialize)]
struct Discovery {
    devices: Vec<Device>,
    unavailable: Vec<Unavailable>,
}

pub fn run(target: Option<&str>, json: bool) -> Result<()> {
    let platforms = match target {
        Some(target) => vec![Platform::parse(target)?],
        None => vec![Platform::Android, Platform::Ios],
    };
    let mut report = Discovery::default();
    for platform in &platforms {
        match discover(*platform) {
            Ok(mut devices) => report.devices.append(&mut devices),
            Err(error) => report.unavailable.push(Unavailable {
                platform: *platform,
                reason: format!("{error:#}"),
            }),
        }
    }
    report
        .devices
        .sort_by(|a, b| (a.platform.as_str(), &a.id).cmp(&(b.platform.as_str(), &b.id)));
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for device in &report.devices {
            println!(
                "{}  {}  {}  {}",
                device.platform.as_str(),
                device.id,
                device.state,
                device.name
            );
        }
        if report.devices.is_empty() {
            println!("No mobile devices found.");
        }
        for unavailable in &report.unavailable {
            eprintln!("{}: {}", unavailable.platform.as_str(), unavailable.reason);
        }
    }
    if report.unavailable.len() == platforms.len() {
        bail!("device discovery unavailable; run dreamer toolchain doctor --target android or ios");
    }
    Ok(())
}

fn discover(platform: Platform) -> Result<Vec<Device>> {
    match platform {
        Platform::Android => {
            let adb = sdk::adb(&sdk::android_root()?)?;
            android_devices(&capture(Command::new(adb).args(["devices", "-l"]))?)
        }
        Platform::Ios => {
            sdk::ios_sdk("iphonesimulator")?;
            ios_devices(&capture(Command::new("xcrun").args([
                "simctl",
                "list",
                "devices",
                "available",
                "--json",
            ]))?)
        }
    }
}

fn android_devices(output: &str) -> Result<Vec<Device>> {
    let mut lines = output.lines();
    if !lines.any(|line| line.trim() == "List of devices attached") {
        bail!("adb returned an unrecognized device list");
    }
    let mut devices = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 2 {
            bail!("adb returned an incomplete device record");
        }
        let name = fields[2..]
            .iter()
            .find_map(|field| field.strip_prefix("model:"))
            .unwrap_or(fields[0])
            .replace('_', " ");
        devices.push(Device {
            platform: Platform::Android,
            id: fields[0].into(),
            name,
            state: if fields[1] == "no" && fields.get(2) == Some(&"permissions") {
                "no permissions"
            } else {
                fields[1]
            }
            .into(),
        });
    }
    Ok(devices)
}

#[derive(Deserialize)]
struct Simulators {
    devices: BTreeMap<String, Vec<Simulator>>,
}

#[derive(Deserialize)]
struct Simulator {
    udid: String,
    name: String,
    state: String,
    #[serde(rename = "isAvailable", default)]
    available: bool,
}

fn ios_devices(output: &str) -> Result<Vec<Device>> {
    let simulators: Simulators =
        serde_json::from_str(output).context("reading simctl device list")?;
    Ok(simulators
        .devices
        .into_iter()
        .filter(|(runtime, _)| runtime.starts_with("com.apple.CoreSimulator.SimRuntime.iOS-"))
        .flat_map(|(_, devices)| devices)
        .filter(|device| device.available)
        .map(|device| Device {
            platform: Platform::Ios,
            id: device.udid,
            name: device.name,
            state: device.state,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_preserves_serials_and_unusable_device_states() {
        let devices = android_devices(
            "* daemon started successfully *\nList of devices attached\n\
             emulator-5554 device product:sdk model:Pixel_8 transport_id:1\n\
             physical unauthorized usb:1\n\
             disconnected offline\n\
             locked no permissions (user in plugdev group)\n",
        )
        .unwrap();
        assert_eq!(devices.len(), 4);
        assert_eq!(devices[0].id, "emulator-5554");
        assert_eq!(devices[0].name, "Pixel 8");
        assert_eq!(devices[1].state, "unauthorized");
        assert_eq!(devices[2].state, "offline");
        assert_eq!(devices[3].state, "no permissions");
    }

    #[test]
    fn android_empty_list_is_valid_but_malformed_output_is_not() {
        assert!(
            android_devices("List of devices attached\n\n")
                .unwrap()
                .is_empty()
        );
        assert!(android_devices("failed to start daemon").is_err());
        assert!(android_devices("List of devices attached\nbroken\n").is_err());
    }

    #[test]
    fn ios_only_lists_available_ios_simulators() {
        let devices = ios_devices(
            r#"{"devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-18-0": [
                {"udid":"b", "name":"iPhone", "state":"Booted", "isAvailable":true},
                {"udid":"a", "name":"iPad", "state":"Shutdown", "isAvailable":true},
                {"udid":"c", "name":"Missing runtime", "state":"Shutdown", "isAvailable":false}
            ],
            "com.apple.CoreSimulator.SimRuntime.watchOS-11-0": [
                {"udid":"d", "name":"Watch", "state":"Booted", "isAvailable":true}
            ]
        }}"#,
        )
        .unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "b");
        assert_eq!(devices[1].state, "Shutdown");
    }

    #[test]
    fn malformed_simulator_output_is_an_error() {
        assert!(ios_devices("{}").is_err());
        assert!(ios_devices("not json").is_err());
    }
}
