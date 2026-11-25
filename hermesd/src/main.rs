use anyhow::Result;
use clap::Parser;
use hermesd::{api, state::AppState, supervisor::ServiceManager, manifest::StackManifest};
use std::path::PathBuf;
use std::sync::Arc;
use std::fs;

#[derive(Parser, Debug)]
#[command(name = "hermesd")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 9876)]
    port: u16,
    #[arg(long, env = "HERMESD_TOKEN", default_value = "")]
    token: String,
    #[arg(long)]
    proot: Option<PathBuf>,
    #[arg(long)]
    rootfs: Option<PathBuf>,
    #[arg(long, env = "HERMESD_ROOTFS_URL", default_value = "")]
    rootfs_url: String,
    #[arg(long, env = "HERMESD_ROOTFS_SHA256", default_value = "")]
    rootfs_sha256: String,
    #[arg(long)]
    manifests: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let rootfs_path = args.rootfs.clone();

    // Bootstrap rootfs if missing
    if let Some(rootfs) = &rootfs_path {
        let needs = !rootfs.join("bin/sh").exists() && !rootfs.join("usr/bin/sh").exists();
        if needs && !args.rootfs_url.is_empty() {
            tracing::info!("Bootstrapping rootfs from {}", args.rootfs_url);
            std::fs::create_dir_all(rootfs).ok();
            let cache = rootfs.join(".rootfs.tar.gz");
            if let Err(e) = hermesd::rootfs::download(&args.rootfs_url, &cache) {
                tracing::error!("download failed: {}", e);
            } else {
                if !args.rootfs_sha256.is_empty() {
                    match hermesd::rootfs::sha256_file(&cache) {
                        Ok(h) if h == args.rootfs_sha256 => {}
                        Ok(h) => tracing::error!("sha256 mismatch: {} != {}", h, args.rootfs_sha256),
                        Err(e) => tracing::error!("sha256 error: {}", e),
                    }
                }
                let f = std::fs::File::open(&cache).unwrap();
                if let Err(e) = hermesd::rootfs::extract_auto(f, rootfs, &args.rootfs_url) {
                    tracing::error!("extract failed: {}", e);
                } else {
                    tracing::info!("Rootfs ready");
                }
                let _ = std::fs::remove_file(&cache);
            }
        }
    }

    let mut manager = ServiceManager::new(
        args.proot.map(|p| p.to_string_lossy().to_string()),
        rootfs_path.as_ref().map(|p| p.to_string_lossy().to_string()),
    );

    let mut manifests: Vec<StackManifest> = Vec::new();
    if let Some(dir) = &args.manifests {
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    let content = fs::read_to_string(&path)?;
                    if let Ok(m) = StackManifest::from_toml(&content) {
                        let stack = m.stack.name.clone();
                        manager.load_manifest(&stack, &m);
                        manifests.push(m);
                    }
                }
            }
        }
    }

    // autostart
    for m in &manifests {
        if m.stack.autostart {
            let stack = m.stack.name.clone();
            for svc in &m.service {
                if svc.enabled {
                    if let Some(h) = manager.get(&format!("{}/{}", stack, svc.name)) {
                        let _ = h.start();
                    }
                }
            }
        }
    }

    let manager = Arc::new(manager);
    // autostart
    // (manifests already loaded into manager; need re-read for autostart flags? We'll just start all services whose... skip; manager doesn't store manifests)
    // Start monitor
    let monitor_mgr = manager.clone();
    std::thread::spawn(move || loop {
        monitor_mgr.monitor_once();
        std::thread::sleep(std::time::Duration::from_secs(2));
    });

    let state = Arc::new(AppState::new(manager, args.token));
    let app = api::router(state.clone());

    let addr = format!("{}:{}", args.host, args.port);
    tracing::info!("hermesd listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
