use actix_web::{web, App, HttpServer, Responder};
use serde::Serialize;
use std::sync::Arc;
use sysinfo::{Cpu, CpuRefreshKind, Disks, NetworkData, Networks, RefreshKind, System};
use tokio::sync::RwLock;

#[derive(Serialize, Clone)]
struct MemoryInfo {
    total: u64,
    used: u64,
    free: u64,
}

#[derive(Serialize, Clone)]
struct DiskInfo {
    name: String,
    total_space: u64,
    available_space: u64,
}

#[derive(Serialize, Clone)]
struct NetworkStats {
    name: String,
    received: u64,
    transmitted: u64,
}

#[derive(Serialize, Clone)]
struct CpuInfo {
    model: String,
    usage: f32,
}

#[derive(Serialize, Clone)]
struct SystemInfo {
    cpu_info: CpuInfo,
    num_cores: usize,
    uptime: String,
    hostname: String,
    memory: MemoryInfo,
    disks: Vec<DiskInfo>,
    network: Vec<NetworkStats>,
}

#[derive(Clone)]
struct AppState {
    cpu_info: Arc<RwLock<CpuInfo>>,
    system: Arc<RwLock<System>>,
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    format!("{:02}:{:02}:{:02}:{:02}", days, hours, minutes, seconds)
}

async fn get_system_info(data: web::Data<AppState>) -> impl Responder {
    let cpu_info = data.cpu_info.read().await;
    let system = data.system.read().await;

    let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());
    let uptime = format_uptime(System::uptime());
    let num_cores = system.cpus().len();

    let memory = MemoryInfo {
        total: system.total_memory(),
        used: system.total_memory() - system.available_memory(),
        free: system.available_memory(),
    };

    let disks = Disks::new_with_refreshed_list()
        .iter()
        .map(|d| DiskInfo {
            name: d.name().to_string_lossy().to_string(),
            total_space: d.total_space(),
            available_space: d.available_space(),
        })
        .collect();

    let network = Networks::new_with_refreshed_list()
        .iter()
        .map(|(name, data)| NetworkStats {
            name: name.clone(),
            received: data.received(),
            transmitted: data.transmitted(),
        })
        .collect();

    let info = SystemInfo {
        cpu_info: cpu_info.clone(),
        num_cores,
        uptime,
        hostname,
        memory,
        disks,
        network,
    };

    web::Json(info)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut sys = System::new_with_specifics(RefreshKind::everything());
    sys.refresh_all();

    let cpu_info = Arc::new(RwLock::new(CpuInfo {
        model: sys
            .cpus()
            .first()
            .map_or("Unknown".to_string(), |cpu| cpu.brand().to_string()),
        usage: 0.0,
    }));
    let system = Arc::new(RwLock::new(sys));

    let cpu_info_clone = cpu_info.clone();

    tokio::spawn(async move {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()));
        loop {
            // Wait a bit because CPU usage is based on time interval.
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            sys.refresh_cpu_all();
            let mut total_usage = 0.0;
            for cpu in sys.cpus() {
                let cpu_usage = cpu.cpu_usage();
                total_usage += cpu_usage;
                println!("{}%", cpu.cpu_usage());
            }

            println!("[DEBUG] Total CPU usage: {}", total_usage);

            let mut cpu_data = cpu_info_clone.write().await;
            cpu_data.usage = total_usage;
        }
    });

    let app_state = AppState { cpu_info, system };

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .route("/api/system-info", web::get().to(get_system_info))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
