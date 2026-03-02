use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FriendPolicy {
    AutoAccept,
    Manual,
    Block,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutboundPolicy {
    Private,   // 전면 차단
    Visible,   // 목록만 허용 (다운로드 시 승인 필요)
    Shared,    // 완전 공유 (자동 수락 친구용)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SharedFolder {
    pub path: String,
    pub name: String,
    pub policy: OutboundPolicy,
}

pub struct AppState {
    pub friend_policies: Mutex<HashMap<String, FriendPolicy>>,
    pub shared_folders: Mutex<Vec<SharedFolder>>,
}
