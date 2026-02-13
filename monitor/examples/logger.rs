pub fn main() {
    let system_info = monitor::system_info();
    println!("System Information:");
    println!("Kernel Version: {}", system_info.kernel_version);
    println!("CPU Cores: {}", system_info.cores_count);
    println!("Total RAM: {:.2} GB", system_info.ram_gb);
    println!("Total Disk Space: {:.2} GB", system_info.disk_gb);

    loop {
        let state = monitor::dump();
        println!("RAM Usage: {:.2}%", state.ram_usage);
        println!("CPU Usage: {:.2}%", state.cpu_usage);
        for disk in state.disk_usage {
            println!("Disk: {}", disk.name);
            println!("  Total: {:.2} GB", disk.total_gb);
            println!("  Used: {:.2} GB", disk.used_gb);
            println!("  Free: {:.2} GB", disk.free_gb);
            if let Some(health) = disk.health {
                println!("  Health: {}", health);
            }
        }
        println!("---");

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
