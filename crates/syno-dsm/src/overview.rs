//! The dashboard picture of a NAS, built from three DSM answers.
//!
//! DSM's field names differ a little between versions and models, so every value is
//! read leniently: numbers may arrive as strings, and missing values become `None`
//! or zero instead of failing the whole overview.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Normal,
    Warning,
    Critical,
}

impl Health {
    fn from_status(status: Option<&str>) -> Self {
        match status.map(str::to_ascii_lowercase).as_deref() {
            None | Some("normal" | "initialized" | "") => Self::Normal,
            Some("crashed" | "broken" | "failing" | "failed" | "critical" | "damage") => {
                Self::Critical
            }
            Some(_) => Self::Warning,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DriveKind {
    Hdd,
    Ssd,
    Nvme,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub id: String,
    /// `1` for `/volume1`; the interface names it in the user's language.
    pub number: Option<u32>,
    /// RAID type as Synology names it, e.g. "SHR-1" or "RAID 5".
    pub raid: Option<String>,
    /// "Btrfs" or "ext4".
    pub file_system: Option<String>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub health: Health,
}

/// A wired network port (`SYNO.Core.Network.Ethernet`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPort {
    /// "eth0", "ovs_eth0" or a bond such as "bond0".
    pub name: String,
    pub connected: bool,
    /// Speed of the current link in Mbit/s.
    pub link_mbps: Option<u32>,
    /// Highest speed the port supports in Mbit/s, when DSM says.
    pub max_mbps: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    pub id: String,
    /// Slot name as DSM reports it, e.g. "Drive 1" or "M.2 Drive 1".
    pub slot: String,
    pub model: String,
    pub kind: DriveKind,
    pub size_bytes: u64,
    pub temperature_c: Option<f64>,
    pub health: Health,
    /// Whether the drive is on Synology's compatibility list; `None` when DSM does not say.
    pub listed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub model: String,
    pub dsm: String,
    pub uptime_seconds: u64,
    pub temperature_c: Option<f64>,
    pub cpu_percent: f64,
    /// e.g. "Intel Celeron J4125"; a Virtual DSM reports its host's processor.
    pub cpu_model: Option<String>,
    pub cpu_cores: Option<u32>,
    pub cpu_clock_mhz: Option<u32>,
    /// Microarchitecture, e.g. "Gemini Lake Refresh"; DSM does not report it.
    pub cpu_architecture: Option<String>,
    pub memory_percent: f64,
    pub memory_bytes: u64,
    pub network_rx_bytes_per_second: f64,
    pub network_tx_bytes_per_second: f64,
    pub network_ports: Vec<NetworkPort>,
    pub volumes: Vec<Volume>,
    pub drives: Vec<Drive>,
}

/// The first of `keys` that holds a number or a numeric string.
fn number(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| match &value[*key] {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    })
}

/// The first of `keys` that holds a non-empty string.
fn text<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        value[*key]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
    })
}

fn bytes(value: Option<f64>) -> u64 {
    value.map_or(0, |bytes| bytes.max(0.0) as u64)
}

/// `"1234:05:06"` (hours:minutes:seconds) or plain seconds.
fn uptime(value: &Value) -> u64 {
    if let Some(text) = value.as_str() {
        let parts: Vec<u64> = text
            .split(':')
            .filter_map(|part| part.trim().parse().ok())
            .collect();
        return match parts.as_slice() {
            [hours, minutes, seconds] => hours * 3600 + minutes * 60 + seconds,
            [seconds] => *seconds,
            _ => 0,
        };
    }
    value.as_u64().unwrap_or(0)
}

/// "INTEL", "Celeron", "J4125" → "Intel Celeron J4125". DSM writes some vendors in
/// capitals; a word already in the family is not repeated.
fn cpu_model(system: &Value) -> Option<String> {
    let vendor = text(system, &["cpu_vendor"]).map(|vendor| {
        if vendor.len() > 3 && vendor.chars().all(|c| c.is_ascii_uppercase()) {
            let mut word = vendor.to_ascii_lowercase();
            word[..1].make_ascii_uppercase();
            word
        } else {
            vendor.to_owned()
        }
    });
    let family = text(system, &["cpu_family"]);
    let series = text(system, &["cpu_series"]);
    let mut parts: Vec<String> = Vec::new();
    if let Some(vendor) = vendor {
        let repeated = family.is_some_and(|family| {
            family
                .to_ascii_lowercase()
                .starts_with(&vendor.to_ascii_lowercase())
        });
        if !repeated {
            parts.push(vendor);
        }
    }
    parts.extend(family.map(str::to_owned));
    parts.extend(series.map(str::to_owned));
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// "DSM 7.3.1-86003 Update 2" → "7.3.1-86003 Update 2".
fn dsm_version(firmware: &str) -> String {
    firmware.strip_prefix("DSM ").unwrap_or(firmware).to_owned()
}

/// Synology's internal RAID names, e.g. "shr_with_1_disk_protect" → "SHR-1".
fn raid_name(device_type: &str) -> String {
    let lower = device_type.to_ascii_lowercase();
    match lower.as_str() {
        "shr_without_disk_protect" => "SHR".into(),
        "shr_with_1_disk_protect" => "SHR-1".into(),
        "shr_with_2_disk_protect" => "SHR-2".into(),
        "basic" => "Basic".into(),
        "jbod" => "JBOD".into(),
        _ => match lower
            .strip_prefix("raid_")
            .or_else(|| lower.strip_prefix("raid"))
        {
            Some(level) if !level.is_empty() => format!("RAID {}", level.trim_start_matches('_')),
            _ => device_type.to_owned(),
        },
    }
}

/// `/volume1` or `volume_1` → 1.
fn volume_number(volume: &Value) -> Option<u32> {
    let path = text(volume, &["vol_path", "id"])?;
    let digits: String = path.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// "btrfs" → "Btrfs"; other names as DSM writes them ("ext4").
fn file_system_name(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "btrfs" => "Btrfs".into(),
        _ => name.to_owned(),
    }
}

/// Speeds arrive as numbers or strings in Mbit/s; DSM uses 0 or -1 for "unknown".
fn mbps(value: &Value, keys: &[&str]) -> Option<u32> {
    number(value, keys)
        .filter(|speed| *speed > 0.0)
        .map(|speed| speed as u32)
}

fn network_ports(ethernet: &Value) -> Vec<NetworkPort> {
    ethernet
        .as_array()
        .or_else(|| ethernet["ethernets"].as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|port| {
            let name = text(port, &["ifname", "id"])?.to_owned();
            Some(NetworkPort {
                connected: text(port, &["status"]).is_some_and(|status| status == "connected"),
                link_mbps: mbps(port, &["speed"]),
                max_mbps: mbps(port, &["max_supported_speed", "max_speed"]),
                name,
            })
        })
        .collect()
}

fn volumes(storage: &Value) -> Vec<Volume> {
    let pools = storage["storagePools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let raid_of = |volume: &Value| {
        text(volume, &["device_type", "raidType"])
            .or_else(|| {
                let pool = text(volume, &["pool_path"])?;
                pools
                    .iter()
                    .find(|candidate| text(candidate, &["id"]) == Some(pool))
                    .and_then(|pool| text(pool, &["device_type", "raidType"]))
            })
            .map(raid_name)
    };
    storage["volumes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|volume| Volume {
            id: text(volume, &["id", "vol_path"])
                .unwrap_or_default()
                .to_owned(),
            number: volume_number(volume),
            raid: raid_of(volume),
            total_bytes: bytes(number(&volume["size"], &["total"])),
            used_bytes: bytes(number(&volume["size"], &["used"])),
            file_system: text(volume, &["fs_type"]).map(file_system_name),
            health: Health::from_status(text(volume, &["status"])),
        })
        .collect()
}

fn drives(storage: &Value) -> Vec<Drive> {
    storage["disks"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|disk| {
            let id = text(disk, &["id"]).unwrap_or_default().to_owned();
            let disk_type = text(disk, &["diskType", "disk_type"]).unwrap_or_default();
            let kind = if disk_type.to_ascii_lowercase().contains("nvme") || id.starts_with("nvme")
            {
                DriveKind::Nvme
            } else if disk["isSsd"].as_bool() == Some(true) {
                DriveKind::Ssd
            } else {
                DriveKind::Hdd
            };
            let model = match (text(disk, &["vendor"]), text(disk, &["model"])) {
                (Some(vendor), Some(model)) if !model.starts_with(vendor) => {
                    format!("{vendor} {model}")
                }
                (_, Some(model)) => model.to_owned(),
                (Some(vendor), None) => vendor.to_owned(),
                (None, None) => String::new(),
            };
            let listed = text(disk, &["compatibility"]).map(|value| value == "support");
            Drive {
                slot: text(disk, &["longName", "name"]).unwrap_or(&id).to_owned(),
                model,
                kind,
                size_bytes: bytes(number(disk, &["size_total"])),
                temperature_c: number(disk, &["temp"]).filter(|temp| *temp > 0.0),
                health: Health::from_status(text(disk, &["overview_status", "status"])),
                listed,
                id,
            }
        })
        .collect()
}

impl Overview {
    pub(crate) fn from_api(
        system: &Value,
        utilization: &Value,
        storage: &Value,
        ethernet: &Value,
    ) -> Self {
        let cpu = &utilization["cpu"];
        let cpu_percent = ["user_load", "system_load", "other_load"]
            .iter()
            .filter_map(|key| number(cpu, &[key]))
            .sum::<f64>()
            .min(100.0);
        let memory = &utilization["memory"];
        let memory_bytes = number(system, &["ram_size"])
            .map(|megabytes| megabytes * 1024.0 * 1024.0)
            .or_else(|| number(memory, &["memory_size", "total_real"]).map(|kib| kib * 1024.0));
        let networks = utilization["network"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let total = networks
            .iter()
            .find(|network| network["device"].as_str() == Some("total"));
        let (rx, tx) = match total {
            Some(total) => (number(total, &["rx"]), number(total, &["tx"])),
            None => (
                Some(networks.iter().filter_map(|n| number(n, &["rx"])).sum()),
                Some(networks.iter().filter_map(|n| number(n, &["tx"])).sum()),
            ),
        };

        Self {
            model: text(system, &["model"]).unwrap_or_default().to_owned(),
            dsm: text(system, &["firmware_ver", "version_string"])
                .map(dsm_version)
                .unwrap_or_default(),
            uptime_seconds: uptime(&system["up_time"]),
            temperature_c: number(system, &["sys_temp", "temperature"]).filter(|temp| *temp > 0.0),
            cpu_percent,
            cpu_model: cpu_model(system),
            cpu_cores: number(system, &["cpu_cores"]).map(|cores| cores as u32),
            cpu_clock_mhz: number(system, &["cpu_clock_speed"]).map(|mhz| mhz as u32),
            cpu_architecture: text(system, &["cpu_series"])
                .and_then(crate::cpu::architecture)
                .map(str::to_owned),
            memory_percent: number(memory, &["real_usage"]).unwrap_or(0.0),
            memory_bytes: bytes(memory_bytes),
            network_rx_bytes_per_second: rx.unwrap_or(0.0),
            network_tx_bytes_per_second: tx.unwrap_or(0.0),
            network_ports: network_ports(ethernet),
            volumes: volumes(storage),
            drives: drives(storage),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Answers recorded from a Virtual DSM 7.4.1 that was not set up yet
    /// (serial number, UUIDs, time and CPU removed).
    #[test]
    fn reads_virtual_dsm_7_4_1() {
        let fixture = |name: &str| -> Value {
            let path = format!(
                "{}/tests/fixtures/dsm-7.4.1-virtualdsm/{name}",
                env!("CARGO_MANIFEST_DIR")
            );
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
        };
        let overview = Overview::from_api(
            &fixture("system-info.json"),
            &fixture("utilization.json"),
            &fixture("storage-load-info.json"),
            &Value::Null,
        );
        assert_eq!(overview.model, "VirtualDSM");
        assert_eq!(overview.dsm, "7.4.1-90080");
        assert_eq!(overview.uptime_seconds, 5 * 60 + 39);
        assert_eq!(overview.temperature_c, None);
        assert_eq!(overview.memory_bytes, 12 * 1024 * 1024 * 1024);
        assert_eq!(overview.memory_percent, 4.0);
        assert!(overview.drives.is_empty());

        let [volume] = overview.volumes.as_slice() else {
            panic!("one volume")
        };
        assert_eq!(volume.number, Some(1));
        assert_eq!(volume.raid.as_deref(), Some("Basic"));
        assert_eq!(volume.file_system.as_deref(), Some("Btrfs"));
        assert_eq!(volume.total_bytes, 412_310_568_960);
        assert_eq!(volume.used_bytes, 75_468_800);
        assert_eq!(volume.health, Health::Normal);
    }

    #[test]
    fn uptime_reads_hours_minutes_seconds() {
        assert_eq!(uptime(&json!("460:12:05")), 460 * 3600 + 12 * 60 + 5);
        assert_eq!(uptime(&json!(3600)), 3600);
        assert_eq!(uptime(&json!(null)), 0);
    }

    #[test]
    fn cpu_model_reads_vendor_family_and_series() {
        let intel =
            json!({ "cpu_vendor": "INTEL", "cpu_family": "Celeron", "cpu_series": "J4125" });
        assert_eq!(cpu_model(&intel).as_deref(), Some("Intel Celeron J4125"));
        let amd = json!({ "cpu_vendor": "AMD", "cpu_family": "Ryzen", "cpu_series": "R1600" });
        assert_eq!(cpu_model(&amd).as_deref(), Some("AMD Ryzen R1600"));
        let repeated = json!({ "cpu_vendor": "Realtek", "cpu_family": "Realtek RTD1296" });
        assert_eq!(cpu_model(&repeated).as_deref(), Some("Realtek RTD1296"));
        assert_eq!(cpu_model(&json!({})), None);
    }

    #[test]
    fn raid_names_follow_synology() {
        assert_eq!(raid_name("shr_with_1_disk_protect"), "SHR-1");
        assert_eq!(raid_name("raid_5"), "RAID 5");
        assert_eq!(raid_name("basic"), "Basic");
        assert_eq!(raid_name("something_new"), "something_new");
    }

    #[test]
    fn health_from_status() {
        assert_eq!(Health::from_status(Some("normal")), Health::Normal);
        assert_eq!(Health::from_status(Some("degrade")), Health::Warning);
        assert_eq!(Health::from_status(Some("crashed")), Health::Critical);
    }

    #[test]
    fn builds_overview_from_lenient_answers() {
        let system = json!({
            "model": "DS923+", "firmware_ver": "DSM 7.3.1-86003", "ram_size": 32768,
            "cpu_vendor": "AMD", "cpu_family": "Ryzen", "cpu_series": "R1600",
            "cpu_cores": "2", "cpu_clock_speed": 2600,
            "up_time": "10:00:00", "sys_temp": 41
        });
        let utilization = json!({
            "cpu": { "user_load": 7, "system_load": "3", "other_load": 2 },
            "memory": { "real_usage": 46 },
            "network": [ { "device": "eth0", "rx": 10, "tx": 1 }, { "device": "total", "rx": 3400000, "tx": 820000 } ]
        });
        let storage = json!({
            "storagePools": [ { "id": "reuse_1", "device_type": "shr_with_1_disk_protect" } ],
            "volumes": [ { "id": "volume_1", "vol_path": "/volume1", "pool_path": "reuse_1",
                           "status": "normal", "size": { "total": "21800000000000", "used": "13100000000000" } } ],
            "disks": [
                { "id": "sata1", "longName": "Drive 1", "vendor": "WDC", "model": "WD120EFBX",
                  "temp": 36, "size_total": "12000138625024", "status": "normal", "isSsd": false,
                  "diskType": "SATA", "compatibility": "not_support" },
                { "id": "nvme0n1", "name": "M.2 Drive 1", "model": "Samsung SSD 990 PRO 1TB",
                  "temp": 44, "size_total": 1_000_204_886_016_u64, "overview_status": "warning", "isSsd": true,
                  "diskType": "M.2 NVMe" }
            ]
        });
        let ethernet = json!([
            { "ifname": "eth0", "status": "connected", "speed": 2500, "max_supported_speed": "2500" },
            { "ifname": "eth1", "status": "disconnected", "speed": -1 }
        ]);
        let overview = Overview::from_api(&system, &utilization, &storage, &ethernet);
        assert_eq!(overview.model, "DS923+");
        assert_eq!(overview.dsm, "7.3.1-86003");
        assert_eq!(overview.uptime_seconds, 36_000);
        assert_eq!(overview.cpu_percent, 12.0);
        assert_eq!(overview.cpu_model.as_deref(), Some("AMD Ryzen R1600"));
        assert_eq!(overview.cpu_cores, Some(2));
        assert_eq!(overview.cpu_clock_mhz, Some(2600));
        assert_eq!(overview.cpu_architecture.as_deref(), Some("Zen"));
        assert_eq!(
            overview.network_ports,
            [
                NetworkPort {
                    name: "eth0".into(),
                    connected: true,
                    link_mbps: Some(2500),
                    max_mbps: Some(2500),
                },
                NetworkPort {
                    name: "eth1".into(),
                    connected: false,
                    link_mbps: None,
                    max_mbps: None,
                },
            ]
        );
        assert_eq!(overview.memory_bytes, 32 * 1024 * 1024 * 1024);
        assert_eq!(overview.network_rx_bytes_per_second, 3_400_000.0);

        let volume = &overview.volumes[0];
        assert_eq!(volume.number, Some(1));
        assert_eq!(volume.raid.as_deref(), Some("SHR-1"));
        assert_eq!(volume.used_bytes, 13_100_000_000_000);

        let [hdd, nvme] = overview.drives.as_slice() else {
            panic!("two drives")
        };
        assert_eq!(hdd.model, "WDC WD120EFBX");
        assert_eq!(hdd.kind, DriveKind::Hdd);
        assert_eq!(hdd.listed, Some(false));
        assert_eq!(nvme.kind, DriveKind::Nvme);
        assert_eq!(nvme.health, Health::Warning);
        assert_eq!(nvme.listed, None);
    }
}
