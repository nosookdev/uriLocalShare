use anyhow::anyhow;
use libp2p::{
    gossipsub, mdns, noise, request_response, swarm::NetworkBehaviour, swarm::SwarmEvent, tcp, yamux,
    StreamProtocol, futures::StreamExt,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use crate::types::{AppState, OutboundPolicy};
use tokio::sync::mpsc;


#[derive(libp2p::swarm::NetworkBehaviour)]
#[behaviour(out_event = "OmniBehaviourEvent")]
struct OmniBehaviour {
    gossipsub: gossipsub::Behaviour,
    request_response: request_response::json::Behaviour<OmniRequest, OmniResponse>,
}

#[derive(Debug)]
enum OmniBehaviourEvent {
    Gossipsub(gossipsub::Event),
    RequestResponse(request_response::Event<OmniRequest, OmniResponse>),
}

impl From<gossipsub::Event> for OmniBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        OmniBehaviourEvent::Gossipsub(event)
    }
}

impl From<request_response::Event<OmniRequest, OmniResponse>> for OmniBehaviourEvent {
    fn from(event: request_response::Event<OmniRequest, OmniResponse>) -> Self {
        OmniBehaviourEvent::RequestResponse(event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OmniRequest {
    ListShares,
    RequestDownload { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OmniResponse {
    ShareList(Vec<String>),
    DownloadApprovalRequired,
    DownloadApproved { url: String },
    Denied,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct P2PMessage {
    pub sender: String,
    pub content: String,
    pub timestamp: u64,
}

pub enum P2PCommand {
    SendMessage(String),
    RequestShares(libp2p::PeerId),
}

pub struct P2PManager {
    cmd_tx: mpsc::Sender<P2PCommand>,
}

impl P2PManager {
    pub fn new(app_handle: AppHandle) -> Result<Self, Box<dyn Error>> {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<P2PCommand>(32);

        let mut swarm = libp2p::SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| {
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(10))
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .build()
                    .map_err(|e| anyhow!(e))?;

                let gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                ).map_err(|e| anyhow!(e))?;
                
                let request_response = request_response::json::Behaviour::new(
                    [(
                        StreamProtocol::new("/omnishare/1.0.0"),
                        request_response::ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                );

                Ok(OmniBehaviour { gossipsub, request_response })
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        let topic = gossipsub::IdentTopic::new("omni-share-chat");
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;


        let app_handle_clone = app_handle.clone();

        tauri::async_runtime::spawn(async move {
            let _ = swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap());
            loop {
                tokio::select! {
                    event = swarm.select_next_some() => match event {
                        SwarmEvent::Behaviour(OmniBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source: peer_id,
                            message_id: _id,
                            message,
                        })) => {
                            let content = String::from_utf8_lossy(&message.data).to_string();
                            let msg = P2PMessage {
                                sender: peer_id.to_string(),
                                content,
                                timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
                            };
                            let _ = app_handle.emit("p2p-message", msg);
                        },
                        SwarmEvent::Behaviour(OmniBehaviourEvent::RequestResponse(request_response::Event::Message {
                            peer: peer_id,
                            message: request_response::Message::Request {
                                request,
                                channel,
                                ..
                            },
                            ..
                        })) => {
                            let state = app_handle.state::<AppState>();
                            match request {
                                OmniRequest::ListShares => {
                                    let folders = state.shared_folders.lock().unwrap();
                                    let visible_names: Vec<String> = folders.iter()
                                        .filter(|f| f.policy != OutboundPolicy::Private)
                                        .map(|f| f.name.clone())
                                        .collect();
                                    let _ = swarm.behaviour_mut().request_response.send_response(channel, OmniResponse::ShareList(visible_names));
                                },
                                OmniRequest::RequestDownload { path } => {
                                    // 다운로드 요청 시 UI 승인 유도 (이벤트 발송)
                                    let _ = app_handle.emit("download-requested", (peer_id.to_string(), path.clone()));
                                    // 일단 대기 응답
                                    let _ = swarm.behaviour_mut().request_response.send_response(channel, OmniResponse::DownloadApprovalRequired);
                                }
                            }
                        },
                        SwarmEvent::Behaviour(OmniBehaviourEvent::RequestResponse(request_response::Event::Message {
                            peer: peer_id,
                            message: request_response::Message::Response {
                                response,
                                ..
                            },
                            ..
                        })) => {
                            // 응답 수신 시 프런트엔드에 전달
                            let _ = app_handle.emit("remote-response-received", (peer_id.to_string(), response));
                        },
                        _ => {}
                    },
                    Some(cmd) = cmd_rx.recv() => {
                        match cmd {
                            P2PCommand::SendMessage(content) => {
                                let msg = P2PMessage {
                                    sender: "Me".to_string(),
                                    content: content.clone(),
                                    timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
                                };
                                let json = serde_json::to_vec(&msg).unwrap();
                                let topic = gossipsub::IdentTopic::new("omni-share-chat");
                                if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic, json) {
                                    println!("Publish error: {e:?}");
                                }
                            },
                            P2PCommand::RequestShares(peer_id) => {
                                swarm.behaviour_mut().request_response.send_request(&peer_id, OmniRequest::ListShares);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self { cmd_tx })
    }

    pub async fn send_message(&self, content: String) -> Result<(), Box<dyn Error>> {
        self.cmd_tx.send(P2PCommand::SendMessage(content)).await?;
        Ok(())
    }

    pub async fn request_remote_shares(&self, peer_id_str: String) -> Result<(), Box<dyn Error>> {
        let peer_id = peer_id_str.parse::<libp2p::PeerId>()?;
        self.cmd_tx.send(P2PCommand::RequestShares(peer_id)).await?;
        Ok(())
    }
}
