use smartctl_rs::device::get_device_info;
use sysinfo::{Disks, System};

pub struct SystemState {
    pub ram_usage: f64,
    pub cpu_usage: f64,

    pub disk_usage: Vec<DiskInfo>,
}

pub struct SystemInfo {
    pub kernel_version: String,
    pub cores_count: usize,

    /// Total RAM in GB
    pub ram_gb: f64,
    /// Total disk space in GB
    pub disk_gb: f64,
}

pub struct DiskInfo {
    pub name: String,

    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,

    /// SMART life percentage, if available (for HDDs/SSDs that support SMART)
    pub health: Option<f64>,
}

pub fn dump() -> SystemState {
    let mut sys = System::new_all();
    sys.refresh_all();
    // Wait a bit and refresh again to get accurate CPU usage
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_all();

    let total_memory = sys.total_memory() as f64;
    let used_memory = sys.used_memory() as f64;
    let ram_usage = if total_memory > 0.0 {
        (used_memory / total_memory) * 100.0
    } else {
        0.0
    };

    let cpu_usage = sys.global_cpu_usage() as f64;

    let disks = Disks::new_with_refreshed_list();
    let disk_usage = disks
        .list()
        .iter()
        .map(|disk| {
            let total = disk.total_space() as f64 / 1_073_741_824.0;
            let available = disk.available_space() as f64 / 1_073_741_824.0;
            let used = total - available;

            let disk_name = disk.name().to_string_lossy();
            let health = get_smart_health(&disk_name);

            DiskInfo {
                name: disk_name.into_owned(),
                total_gb: total,
                used_gb: used,
                free_gb: available,
                health,
            }
        })
        .collect();

    SystemState {
        ram_usage,
        cpu_usage,
        disk_usage,
    }
}

pub fn system_info() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let ram_gb = sys.total_memory() as f64 / 1_073_741_824.0;

    let disks = Disks::new_with_refreshed_list();
    let disk_gb = disks.list().iter().map(|d| d.total_space()).sum::<u64>()
        as f64
        / 1_073_741_824.0;

    SystemInfo {
        kernel_version: System::kernel_version()
            .unwrap_or_else(|| "unknown".to_string()),
        cores_count: sys.cpus().len(),
        ram_gb,
        disk_gb,
    }
}

fn get_smart_health(disk_name: &str) -> Option<f64> {
    // heuristics to get parent device from partition name
    // e.g. sda1 -> /dev/sda
    // nvme0n1p1 -> /dev/nvme0n1

    let device_name = if disk_name.starts_with("nvme") {
        if let Some(pos) = disk_name.rfind('p') {
            if pos > 0 && disk_name[pos + 1..].chars().all(char::is_numeric) {
                // Check if it's p<digits> at the end, but be careful with nvme0n1
                // nvme0n1 also ends with numbers, but usually doesn't have 'p' as separator
                // unless it is a partition.
                // Safest bet: if it has 'p' followed by digits, strip it.
                // But nvme0n1 has no p (except maybe in the name, but usually it's nvmeXnY).
                let (base, _) = disk_name.split_at(pos);
                base.to_string()
            } else {
                disk_name.to_string()
            }
        } else {
            disk_name.to_string()
        }
    } else {
        // sda1 -> sda
        disk_name.trim_end_matches(char::is_numeric).to_string()
    };

    let path = format!("/dev/{}", device_name);

    match get_device_info(&path) {
        Ok(info) => {
            // Logic to calculate health
            if let Some(nvme_log) = info.nvme_smart_health_information_log
                && let Some(percentage_used) = nvme_log.get("percentage_used")
            {
                return Some((100.0 - *percentage_used as f64).max(0.0));
            }

            if let Some(attrs) = info.ata_smart_attributes {
                // Look for common life-left attributes
                for attr in attrs.table {
                    let name = attr.name.to_lowercase();
                    if name.contains("life_left")
                        || name.contains("lifetime_remain")
                    {
                        return Some(attr.value as f64);
                    }
                    // ID 233: Media_Wearout_Indicator
                    if attr.id == 233 {
                        return Some(attr.value as f64);
                    }
                }
            }

            // Fallback: Smart status
            if let Some(status) = info.smart_status {
                if status.passed {
                    Some(100.0)
                } else {
                    Some(0.0)
                }
            } else {
                None
            }
        }
        Err(_) => None,
    }
}
