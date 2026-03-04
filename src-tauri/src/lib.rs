use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

mod types;
mod p2p;
mod server;
pub use types::*;
use p2p::P2PManager;
use std::sync::Arc;

#[tauri::command]
async fn request_remote_shares(
    peer_id: String,
    p2p_manager: tauri::State<'_, P2PManager>,
) -> Result<(), String> {
    p2p_manager.request_remote_shares(peer_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn request_download(
    peer_id: String,
    path: String,
    p2p_manager: tauri::State<'_, P2PManager>,
) -> Result<(), String> {
    p2p_manager.request_download(peer_id, path).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn approve_download(
    peer_id: String,
    path: String,
    p2p_manager: tauri::State<'_, P2PManager>,
) -> Result<(), String> {
    p2p_manager.approve_download(peer_id, path).await.map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
struct DiscoveredService {
    name: String,
    ip: String,
    port: u16,
    service_type: String,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn list_shares(_server_ip: String) -> Result<Vec<String>, String> {
    // TODO: 실제 SMB 연동 로직
    Ok(vec!["Public".into(), "Videos".into(), "Photos".into()])
}

#[tauri::command]
async fn send_message(
    _friend_id: String, 
    content: String, 
    p2p_manager: tauri::State<'_, P2PManager>
) -> Result<(), String> {
    p2p_manager.send_message(content).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_friend_policy(
    ip_address: String,
    policy: FriendPolicy,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut policies = state.friend_policies.lock().unwrap();
    policies.insert(ip_address.clone(), policy);
    Ok(())
}

// --- 내 공유 관리 커맨드 ---

#[tauri::command]
fn get_shared_folders(state: tauri::State<'_, AppState>) -> Vec<SharedFolder> {
    state.shared_folders.lock().unwrap().clone()
}

#[tauri::command]
fn add_shared_folder(path: String, name: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut folders = state.shared_folders.lock().unwrap();
    if folders.iter().any(|f| f.path == path) {
        return Err("이미 공유 중인 폴더입니다.".into());
    }
    folders.push(SharedFolder {
        path,
        name,
        policy: OutboundPolicy::Visible,
    });
    Ok(())
}

#[tauri::command]
fn remove_shared_folder(path: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut folders = state.shared_folders.lock().unwrap();
    folders.retain(|f| f.path != path);
    Ok(())
}

#[tauri::command]
fn update_shared_folder_policy(path: String, policy: OutboundPolicy, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut folders = state.shared_folders.lock().unwrap();
    if let Some(folder) = folders.iter_mut().find(|f| f.path == path) {
        folder.policy = policy;
        Ok(())
    } else {
        Err("폴더를 찾을 수 없습니다.".into())
    }
}

#[tauri::command]
async fn add_friend(mac_address: String, ip_address: String) -> Result<String, String> {
    println!("Adding friend: {} ({})", mac_address, ip_address);
    Ok(format!("Friend added: {}", mac_address))
}

#[tauri::command]
fn start_discovery(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut mdns_opt = state.mdns_daemon.lock().unwrap();
    
    let mdns = if let Some(ref mdns) = *mdns_opt {
        mdns.clone()
    } else {
        let new_mdns = ServiceDaemon::new().map_err(|e| e.to_string())?;
        *mdns_opt = Some(new_mdns.clone());
        new_mdns
    };

    // 브라우저 채널이 여러 번 열리지 않도록 주의 (필요시 관리 로직 추가)
    let receiver = mdns.browse("_omnishare._tcp.local.").map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn(async move {
        while let Ok(event) = receiver.recv() {
            if let ServiceEvent::ServiceResolved(info) = event {
                let service = DiscoveredService {
                    name: info.get_fullname().to_string(),
                    ip: info.get_addresses().iter().next().map(|a| a.to_string()).unwrap_or_default(),
                    port: info.get_port(),
                    service_type: "omnishare".into(),
                };
                let _ = app.emit("service-discovered", service);
            }
        }
    });

    Ok(())
}

fn register_self_service(state: &AppState) -> Result<(), String> {
    let mdns_opt = state.mdns_daemon.lock().unwrap();
    if let Some(ref mdns) = *mdns_opt {
        let port = *state.local_server_port.lock().unwrap();
        if port == 0 { return Ok(()); }

        let hostname = gethostname::gethostname().to_string_lossy().to_string();
        let service_name = format!("{}-{}", hostname, port);
        
        let service_info = mdns_sd::ServiceInfo::new(
            "_omnishare._tcp.local.",
            &service_name,
            &format!("{}.local.", service_name),
            "",
            port,
            None,
        ).map_err(|e| e.to_string())?;

        mdns.register(service_info).map_err(|e| e.to_string())?;
        println!("Registered mDNS service: {} on port {}", service_name, port);
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = Arc::new(AppState {
        friend_policies: Mutex::new(HashMap::new()),
        shared_folders: Mutex::new(Vec::new()),
        local_server_port: Mutex::new(0),
        mdns_daemon: Mutex::new(None),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state.clone())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            
            // 초기 mdns 데몬 생성
            {
                let mut mdns_opt = state.mdns_daemon.lock().unwrap();
                if let Ok(mdns) = ServiceDaemon::new() {
                    *mdns_opt = Some(mdns);
                }
            }

            // 파일 서버 실행 및 서비스 등록
            let server_state = state.clone();
            tauri::async_runtime::spawn(async move {
                server::start_file_server(server_state.clone()).await;
                let _ = register_self_service(&server_state);
            });

            let p2p_result = P2PManager::new(app_handle);
            match p2p_result {
                Ok(p2p_manager) => {
                    app.manage(p2p_manager);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("Setup failed: {}", e);
                    Err(e.into())
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet, 
            list_shares, 
            send_message, 
            add_friend,
            update_friend_policy,
            get_shared_folders,
            add_shared_folder,
            remove_shared_folder,
            update_shared_folder_policy,
            request_remote_shares,
            request_download,
            approve_download,
            start_discovery
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
