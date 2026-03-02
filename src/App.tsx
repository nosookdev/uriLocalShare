import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

interface DiscoveredService {
  name: string;
  ip: string;
  port: number;
  service_type: string;
}

interface P2PMessage {
  sender: string;
  content: string;
  timestamp: number;
}

interface SharedFolder {
  path: string;
  name: string;
  policy: string;
}

function App() {
  const [activeTab, setActiveTab] = useState("files");
  const [shares, setShares] = useState<string[]>([]);
  const [discoveredDevices, setDiscoveredDevices] = useState<DiscoveredService[]>([]);
  const [selectedServer, setSelectedServer] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [p2pMessages, setP2pMessages] = useState<P2PMessage[]>([]);
  const [sharedFolders, setSharedFolders] = useState<SharedFolder[]>([]);
  const [remoteShares, setRemoteShares] = useState<Record<string, string[]>>({});
  const [activeDownloadRequest, setActiveDownloadRequest] = useState<any>(null);
  const [friends, setFriends] = useState<{ name: string, ip: string, status: string, policy: string }[]>([
    { name: "내 노트북 (Mac)", ip: "192.168.0.15", status: "Online", policy: "manual" },
    { name: "거실 PC (Win)", ip: "192.168.0.22", status: "Online", policy: "manual" },
    { name: "파일 서버 (Linux)", ip: "192.168.0.100", status: "Offline", policy: "autoaccept" },
  ]);
  const [message, setMessage] = useState("");

  useEffect(() => {
    // mDNS 탐색 시작
    invoke("start_discovery").catch(console.error);

    // 서비스 발견 이벤트 리스너
    const unlistenDiscoveryPromise = listen<DiscoveredService>("service-discovered", (event) => {
      setDiscoveredDevices(prev => {
        if (prev.find(d => d.ip === event.payload.ip)) return prev;
        return [...prev, event.payload];
      });
    });

    // P2P 메시지 수신 이벤트 리스너
    const unlistenMessagesPromise = listen<P2PMessage>("p2p-message", (event) => {
      setP2pMessages(prev => [...prev, event.payload]);
    });

    // 초기 공유 폴더 로드
    const loadSharedFolders = async () => {
      try {
        const folders = await invoke<SharedFolder[]>("get_shared_folders");
        setSharedFolders(folders);
      } catch (e) {
        console.error("Failed to load shared folders", e);
      }
    };
    loadSharedFolders();

    const unlistenShares = listen("remote-response-received", (event: any) => {
      const [peerId, response] = event.payload;
      if (response.ShareList) {
        setRemoteShares(prev => ({ ...prev, [peerId]: response.ShareList }));
      }
    });

    const unlistenDownload = listen("download-requested", (event: any) => {
      const [peerId, path] = event.payload;
      setActiveDownloadRequest({ peerId, path });
    });

    return () => {
      unlistenDiscoveryPromise.then(f => f());
      unlistenMessagesPromise.then(f => f());
      unlistenShares.then(u => u());
      unlistenDownload.then(u => u());
    };
  }, []);

  const handleAddFriend = async (name: string, ip: string) => {
    try {
      await invoke("add_friend", { macAddress: "unknown", ipAddress: ip });
      setFriends(prev => [...prev, { name, ip, status: "Online", policy: "manual" }]);
    } catch (e) {
      console.error("Failed to add friend", e);
    }
  };

  const handlePolicyChange = async (ip: string, newPolicy: string) => {
    try {
      await invoke("update_friend_policy", { ipAddress: ip, policy: newPolicy });
      setFriends(prev => prev.map(f => f.ip === ip ? { ...f, policy: newPolicy } : f));
    } catch (e) {
      console.error("Failed to update policy", e);
    }
  };

  const handleAddShare = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "공유할 폴더를 선택하세요"
      });

      if (selected && typeof selected === 'string') {
        const name = selected.split(/[\\/]/).pop() || "Shared Folder";
        await invoke("add_shared_folder", { path: selected, name });
        const folders = await invoke<SharedFolder[]>("get_shared_folders");
        setSharedFolders(folders);
      }
    } catch (e) {
      console.error("Failed to add share", e);
    }
  };

  const handleRemoveShare = async (path: string) => {
    try {
      await invoke("remove_shared_folder", { path });
      setSharedFolders(prev => prev.filter(f => f.path !== path));
    } catch (e) {
      console.error("Failed to remove share", e);
    }
  };

  const handleUpdateSharePolicy = async (path: string, policy: string) => {
    try {
      await invoke("update_shared_folder_policy", { path, policy });
      setSharedFolders(prev => prev.map(f => f.path === path ? { ...f, policy } : f));
    } catch (e) {
      console.error("Failed to update share policy", e);
    }
  };

  const handleSendMessage = async () => {
    if (!message.trim()) return;
    try {
      await invoke("send_message", { friendId: "all", content: message });
      setMessage("");
    } catch (e) {
      console.error("Failed to send message", e);
    }
  };

  const handleRequestRemoteShares = async (peerId: string) => {
    try {
      await invoke("request_remote_shares", { peerId });
    } catch (e) {
      console.error("Failed to request remote shares", e);
    }
  };

  const handleStartDiscovery = async () => {
    setIsLoading(true);
    try {
      await invoke("start_discovery");
    } catch (e) {
      console.error("Failed to start discovery", e);
    } finally {
      setIsLoading(false);
    }
  };

  const handleListShares = async (serverIp: string) => {
    setIsLoading(true);
    try {
      const result: string[] = await invoke("list_shares", { serverIp });
      setShares(result);
      alert(`Shares from ${serverIp}: ${result.join(', ')}`);
    } catch (e) {
      console.error(`Failed to list shares from ${serverIp}`, e);
      alert(`공유 목록을 가져오지 못했습니다: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="app-container">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="logo-area">
          <div className="logo-text">OmniShare</div>
        </div>

        <nav>
          <div className={`nav-item ${activeTab === 'home' ? 'active' : ''}`} onClick={() => setActiveTab('home')}>
            <span>🏠</span> 홈
          </div>
          <div className={`nav-item ${activeTab === 'files' ? 'active' : ''}`} onClick={() => setActiveTab('files')}>
            <span>🌐</span> 네트워크
          </div>
          <div className={`nav-item ${activeTab === 'myshares' ? 'active' : ''}`} onClick={() => setActiveTab('myshares')}>
            <span>📂</span> 내 공유
          </div>
          <div className={`nav-item ${activeTab === 'messages' ? 'active' : ''}`} onClick={() => setActiveTab('messages')}>
            <span>💬</span> 메시징
          </div>
          <div className={`nav-item ${activeTab === 'friends' ? 'active' : ''}`} onClick={() => setActiveTab('friends')}>
            <span>👥</span> 친구 관리
          </div>
          <div className={`nav-item ${activeTab === 'settings' ? 'active' : ''}`} onClick={() => setActiveTab('settings')}>
            <span>⚙️</span> 설정
          </div>
        </nav>
      </aside>

      {/* Main Content */}
      <main className="main-view">
        {activeTab === 'home' && (
          <div className="glass-card">
            <h2>반가워요! 👋</h2>
            <p>OmniShare로 안전하고 빠르게 파일을 공유해보세요.</p>
            <div style={{ marginTop: '24px' }}>
              <p style={{ fontSize: '14px', color: '#94a3b8', marginBottom: '12px' }}>네트워크에서 발견된 기기: {discoveredDevices.length}개</p>
              <div style={{ display: 'flex', gap: '12px' }}>
                <button className="btn-primary" onClick={() => setActiveTab('files')}>서버 검색하기</button>
                <button className="btn-primary" style={{ background: 'rgba(255,255,255,0.1)' }}>도움말</button>
              </div>
            </div>
          </div>
        )}

        {activeTab === 'files' && (
          <div className="section-content glass-card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
              <h2 style={{ fontSize: '24px', fontWeight: 'bold' }}>네트워크 공유 탐색</h2>
              <button
                className="btn btn-primary"
                onClick={handleStartDiscovery}
                disabled={isLoading}
              >
                {isLoading ? '탐색 중...' : '기기 탐색'}
              </button>
            </div>

            <div className="share-grid">
              {discoveredDevices.map((s, i) => (
                <div key={i} className="share-card">
                  <div style={{ fontSize: '32px', marginBottom: '12px' }}>🖥️</div>
                  <div style={{ fontWeight: 600, fontSize: '18px' }}>{s.name.split('.')[0]}</div>
                  <div style={{ color: '#94a3b8', fontSize: '14px', marginBottom: '16px' }}>{s.ip}</div>
                  <div style={{ display: 'flex', gap: '8px', justifyContent: 'center' }}>
                    <button className="btn btn-primary" onClick={() => handleRequestRemoteShares(s.name)}>
                      공유 조회
                    </button>
                    <button className="btn" style={{ background: 'rgba(255,255,255,0.05)', color: '#fff' }} onClick={() => setSelectedServer(s.ip)}>
                      SMB 연결
                    </button>
                  </div>
                  {remoteShares[s.name] && (
                    <div style={{ marginTop: '16px', textAlign: 'left', background: 'rgba(0,0,0,0.2)', padding: '10px', borderRadius: '12px' }}>
                      <div style={{ fontSize: '12px', color: '#6366f1', marginBottom: '8px', fontWeight: 700 }}>AVAILABLE SHARES:</div>
                      {remoteShares[s.name].map((share, idx) => (
                        <div key={idx} style={{ fontSize: '13px', padding: '4px 0', borderBottom: '1px solid rgba(255,255,255,0.05)', display: 'flex', justifyContent: 'space-between' }}>
                          <span>📁 {share}</span>
                          <span style={{ cursor: 'pointer', color: '#2d5bff' }}>⬇️</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              ))}
              {discoveredDevices.length === 0 && !isLoading && (
                <p style={{ gridColumn: '1 / -1', textAlign: 'center', color: '#64748b', padding: '40px' }}>
                  발견된 기기가 없습니다. '기기 탐색' 버튼을 눌러보세요.
                </p>
              )}
            </div>
          </div>
        )}

        {activeTab === 'myshares' && (
          <div className="section-content glass-card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
              <div>
                <h2 style={{ fontSize: '24px', fontWeight: 'bold' }}>내 공유 관리 (Zero-Trust)</h2>
                <p style={{ fontSize: '14px', color: '#94a3b8' }}>명시적으로 공유 지정한 폴더만 상대방에게 노출됩니다.</p>
              </div>
              <button className="btn btn-primary" onClick={handleAddShare}>
                + 공유 폴더 추가
              </button>
            </div>

            <div className="file-list">
              {sharedFolders.length === 0 ? (
                <div style={{ padding: '40px', textAlign: 'center', color: '#64748b', border: '1px dashed rgba(255,255,255,0.1)', borderRadius: '16px' }}>
                  아직 공유 중인 폴더가 없습니다.
                </div>
              ) : (
                sharedFolders.map(folder => (
                  <div key={folder.path} className="file-list-item" style={{ justifyContent: 'space-between' }}>
                    <div style={{ display: 'flex', alignItems: 'center' }}>
                      <div style={{ fontSize: '24px', marginRight: '16px' }}>📁</div>
                      <div>
                        <div style={{ fontWeight: 'bold' }}>{folder.name}</div>
                        <div style={{ fontSize: '11px', color: '#64748b' }}>{folder.path}</div>
                      </div>
                    </div>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                      <select
                        value={folder.policy}
                        onChange={(e) => handleUpdateSharePolicy(folder.path, e.target.value)}
                        style={{ background: 'rgba(255,255,255,0.05)', border: '1px solid rgba(255,255,255,0.1)', color: 'white', padding: '6px 12px', borderRadius: '8px', fontSize: '13px', cursor: 'pointer' }}
                      >
                        <option value="private" style={{ background: '#1e1b4b' }}>🔒 비공개</option>
                        <option value="visible" style={{ background: '#1e1b4b' }}>👁️ 목록만 허용 (승인 필요)</option>
                        <option value="shared" style={{ background: '#1e1b4b' }}>🔓 완전 공유 (신뢰용)</option>
                      </select>
                      <button
                        onClick={() => handleRemoveShare(folder.path)}
                        style={{ background: 'rgba(239, 68, 68, 0.1)', border: 'none', color: '#ef4444', padding: '6px 12px', borderRadius: '8px', cursor: 'pointer', fontSize: '13px' }}
                      >
                        삭제
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        {activeTab === 'messages' && (
          <div className="glass-card" style={{ height: '80%', display: 'flex', flexDirection: 'column' }}>
            <h2>보안 메시징 (P2P)</h2>
            <div style={{ flex: 1, border: '1px solid rgba(255,255,255,0.05)', borderRadius: '12px', margin: '16px 0', padding: '16px', backgroundColor: 'rgba(0,0,0,0.1)', overflowY: 'auto' }}>
              {p2pMessages.length === 0 ? (
                <p style={{ color: '#94a3b8', textAlign: 'center', marginTop: '40px' }}>종단간 암호화로 보호되는 대화방입니다.</p>
              ) : (
                p2pMessages.map((m, i) => (
                  <div key={i} style={{ marginBottom: '12px', padding: '10px', background: 'rgba(255,255,255,0.03)', borderRadius: '8px' }}>
                    <div style={{ fontSize: '11px', color: '#2d5bff', marginBottom: '4px' }}>{m.sender.slice(-8)}</div>
                    <div style={{ fontSize: '14px' }}>{m.content}</div>
                  </div>
                ))
              )}
            </div>
            <div style={{ display: 'flex', gap: '12px' }}>
              <input
                type="text"
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleSendMessage()}
                placeholder="메시지를 입력하세요..."
                style={{ flex: 1, background: 'rgba(255,255,255,0.05)', border: '1px solid rgba(255,255,255,0.1)', color: 'white', padding: '12px', borderRadius: '12px' }}
              />
              <button className="btn-primary" onClick={handleSendMessage}>전송</button>
            </div>
          </div>
        )}

        {activeTab === 'friends' && (
          <div className="glass-card">
            <h2>내 친구 및 기기</h2>
            <div className="friend-list" style={{ marginTop: '20px' }}>
              {friends.map(friend => (
                <div key={friend.ip} className="file-list-item" style={{ justifyContent: 'space-between' }}>
                  <div style={{ display: 'flex', alignItems: 'center' }}>
                    <div style={{ width: '10px', height: '10px', borderRadius: '50%', backgroundColor: friend.status === 'Online' ? '#10b981' : '#6b7280', marginRight: '12px' }}></div>
                    <div>
                      <div style={{ fontWeight: 'bold' }}>{friend.name}</div>
                      <div style={{ fontSize: '12px', color: '#94a3b8' }}>{friend.ip}</div>
                    </div>
                  </div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <span style={{ fontSize: '11px', color: '#64748b' }}>수락 정책:</span>
                    <select
                      value={friend.policy}
                      onChange={(e) => handlePolicyChange(friend.ip, e.target.value)}
                      style={{ background: 'rgba(255,255,255,0.05)', border: '1px solid rgba(255,255,255,0.1)', color: 'white', padding: '4px 8px', borderRadius: '8px', fontSize: '12px', cursor: 'pointer' }}
                    >
                      <option value="autoaccept" style={{ background: '#1e1b4b' }}>자동 수락</option>
                      <option value="manual" style={{ background: '#1e1b4b' }}>확인 후 수락</option>
                      <option value="block" style={{ background: '#1e1b4b' }}>차단</option>
                    </select>
                  </div>
                </div>
              ))}
            </div>

            <h3 style={{ marginTop: '32px', fontSize: '16px', color: '#94a3b8' }}>주변 검색된 기기</h3>
            <div className="discovered-list" style={{ marginTop: '12px' }}>
              {discoveredDevices.map(device => (
                <div key={device.ip} className="file-list-item" style={{ justifyContent: 'space-between', background: 'rgba(255,255,255,0.02)' }}>
                  <div style={{ display: 'flex', alignItems: 'center' }}>
                    <span>💻</span>
                    <div style={{ marginLeft: '12px' }}>
                      <div style={{ fontWeight: '500' }}>{device.name.split('.')[0]}</div>
                      <div style={{ fontSize: '11px', color: '#94a3b8' }}>{device.ip}</div>
                    </div>
                  </div>
                  <button
                    className="btn-primary"
                    style={{ padding: '4px 12px', fontSize: '12px' }}
                    onClick={() => handleAddFriend(device.name.split('.')[0], device.ip)}
                  >
                    추가
                  </button>
                </div>
              ))}
              {discoveredDevices.length === 0 && <p style={{ fontSize: '14px', color: '#64748b' }}>주변에 발견된 새로운 기기가 없습니다.</p>}
            </div>
          </div>
        )}
      </main>
      {/* 다운로드 승인 모달 */}
      {activeDownloadRequest && (
        <div style={{
          position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
          background: 'rgba(0,0,0,0.8)', backdropFilter: 'blur(10px)',
          display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000
        }}>
          <div className="glass-card" style={{ width: '400px', textAlign: 'center' }}>
            <h2 style={{ marginBottom: '16px' }}>🔑 다운로드 요청</h2>
            <p style={{ color: '#94a3b8', marginBottom: '24px' }}>
              친구가 <strong>{activeDownloadRequest.path}</strong> 폴더 입장을 요청했습니다. 허가하시겠습니까?
            </p>
            <div style={{ display: 'flex', gap: '12px' }}>
              <button className="btn btn-primary" style={{ flex: 1 }} onClick={() => setActiveDownloadRequest(null)}>
                승인
              </button>
              <button className="btn" style={{ flex: 1, background: '#ef4444' }} onClick={() => setActiveDownloadRequest(null)}>
                거절
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;

