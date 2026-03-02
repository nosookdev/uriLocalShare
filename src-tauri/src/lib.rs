use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

mod types;
mod p2p;
pub use types::*;
use p2p::P2PManager;

#[tauri::command]
async fn request_remote_shares(
    peer_id: String,
    p2p_manager: tauri::State<'_, P2PManager>,
) -> Result<(), String> {
    p2p_manager.request_remote_shares(peer_id).await.map_err(|e| e.to_string())
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
async fn list_shares(server_ip: String) -> Result<Vec<String>, String> {
    println!("Connecting to SMB server: {}", server_ip);
    
    // TODO: smb-rs 0.11.1 버전의 복잡한 API 연동 (Connection::build 등)
    // 현재는 빌드를 위해 가상의 목록을 반환하거나 일시 우회합니다.
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
    println!("Updated policy for {}: {:?}", ip_address, policies.get(&ip_address));
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
        policy: OutboundPolicy::Visible, // 기본값: 목록만 허용
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
    // TODO: 친구 추가 및 인증 로직 구현
    println!("Adding friend: {} ({})", mac_address, ip_address);
    Ok(format!("Friend added: {}", mac_address))
}

#[tauri::command]
fn start_discovery(app: AppHandle) -> Result<(), String> {
    let mdns = ServiceDaemon::new().map_err(|e| e.to_string())?;
    let receiver = mdns.browse("_smb._tcp.local.").map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn(async move {
        while let Ok(event) = receiver.recv() {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    let service = DiscoveredService {
                        name: info.get_fullname().to_string(),
                        ip: info.get_addresses().iter().next().map(|a| a.to_string()).unwrap_or_default(),
                        port: info.get_port(),
                        service_type: "smb".into(),
                    };
                    let _ = app.emit("service-discovered", service);
                }
                _ => {}
            }
        }
    });

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            friend_policies: Mutex::new(HashMap::new()),
            shared_folders: Mutex::new(Vec::new()),
        })
        .setup(|app| {
            let p2p_manager = P2PManager::new(app.handle().clone()).map_err(|e| e.to_string())?;
            app.manage(p2p_manager);
            Ok(())
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
            start_discovery
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
