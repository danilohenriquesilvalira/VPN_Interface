import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Shield, ShieldOff, Settings, Wifi, WifiOff, ChevronDown, ChevronUp, Loader } from "lucide-react";
import "./App.css";

interface VpnConfig {
  host: string;
  port: number;
  hub: string;
  username: string;
  password: string;
  account_name: string;
}

interface VpnStatus {
  connected: boolean;
  local_ip?: string;
  message: string;
  softether_ready: boolean;
}

function App() {
  const [config, setConfig] = useState<VpnConfig | null>(null);
  const [status, setStatus] = useState<VpnStatus>({ connected: false, message: "A iniciar...", softether_ready: false });
  const [loading, setLoading] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [logMessages, setLogMessages] = useState<string[]>([]);
  const [editConfig, setEditConfig] = useState<VpnConfig | null>(null);

  const addLog = (msg: string) => {
    const time = new Date().toLocaleTimeString("pt-PT");
    setLogMessages(prev => [`[${time}] ${msg}`, ...prev].slice(0, 20));
  };

  useEffect(() => {
    invoke<VpnConfig>("get_default_config").then(cfg => {
      setConfig(cfg);
      setEditConfig(cfg);
    });
    // Verificar e instalar SoftEther automaticamente na primeira execução
    const ensureReady = async () => {
      setInstalling(true);
      try {
        const msg = await invoke<string>("check_and_install_softether");
        addLog(msg);
      } catch (e: any) {
        addLog(`Aviso: ${e}`);
      } finally {
        setInstalling(false);
      }
    };
    ensureReady();
  }, []);

  useEffect(() => {
    const check = async () => {
      const s = await invoke<VpnStatus>("get_status");
      setStatus(s);
    };
    check();
    const interval = setInterval(check, 4000);
    return () => clearInterval(interval);
  }, []);

  const handleConnect = async () => {
    if (!config) return;
    setLoading(true);
    addLog("A ligar à rede RLS Automação...");
    try {
      const msg = await invoke<string>("connect", { config });
      addLog(msg);
      setStatus(prev => ({ ...prev, connected: true, message: "Ligado à rede RLS" }));
    } catch (e: any) {
      addLog(`Erro: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const handleDisconnect = async () => {
    if (!config) return;
    setLoading(true);
    addLog("A desligar...");
    try {
      const msg = await invoke<string>("disconnect", { config });
      addLog(msg);
      setStatus(prev => ({ ...prev, connected: false, message: "Desligado" }));
    } catch (e: any) {
      addLog(`Erro: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const saveSettings = () => {
    if (editConfig) {
      setConfig(editConfig);
      setShowSettings(false);
      addLog("Configurações guardadas.");
    }
  };

  return (
    <div className="app">
      <div className="header">
        <div className="logo-area">
          <div className="logo-icon">
            <Shield size={28} color="#00d4ff" />
          </div>
          <div>
            <div className="logo-title">RLS Automação</div>
            <div className="logo-sub">VPN Secure Connect</div>
          </div>
        </div>
        <div className="version">v1.0.0</div>
      </div>

      <div className={`status-card ${status.connected ? "connected" : "disconnected"}`}>
        <div className="status-icon">
          {status.connected
            ? <Wifi size={52} color="#00ff88" />
            : <WifiOff size={52} color="#ff4466" />}
        </div>
        <div className="status-text">
          {status.connected ? "LIGADO" : "DESLIGADO"}
        </div>
        <div className="status-sub">{status.message}</div>
        {status.connected && (
          <div className="network-info">
            <span className="badge">192.168.222.x</span>
            <span className="badge">Layer 2</span>
          </div>
        )}
      </div>

      <div className="action-area">
        {installing ? (
          <button className="btn-loading" disabled>
            <Loader size={18} className="spin" />
            A instalar componentes VPN...
          </button>
        ) : loading ? (
          <button className="btn-loading" disabled>
            <Loader size={18} className="spin" />
            {status.connected ? "A desligar..." : "A ligar..."}
          </button>
        ) : status.connected ? (
          <button className="btn-disconnect" onClick={handleDisconnect}>
            <ShieldOff size={18} />
            Desligar
          </button>
        ) : (
          <button className="btn-connect" onClick={handleConnect}>
            <Shield size={18} />
            Ligar à Rede RLS
          </button>
        )}
      </div>

      {config && (
        <>
          <div className="info-row">
            <span className="info-label">Servidor</span>
            <span className="info-value">{config.host}:{config.port}</span>
          </div>
          <div className="info-row">
            <span className="info-label">Utilizador</span>
            <span className="info-value">{config.username} @ {config.hub}</span>
          </div>
        </>
      )}

      <div className="settings-toggle" onClick={() => setShowSettings(!showSettings)}>
        <Settings size={14} />
        Configurações avançadas
        {showSettings ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
      </div>

      {showSettings && editConfig && (
        <div className="settings-panel">
          <div className="field">
            <label>Servidor</label>
            <input value={editConfig.host} onChange={e => setEditConfig({ ...editConfig, host: e.target.value })} />
          </div>
          <div className="field">
            <label>Porta</label>
            <input type="number" value={editConfig.port} onChange={e => setEditConfig({ ...editConfig, port: Number(e.target.value) })} />
          </div>
          <div className="field">
            <label>Hub</label>
            <input value={editConfig.hub} onChange={e => setEditConfig({ ...editConfig, hub: e.target.value })} />
          </div>
          <div className="field">
            <label>Utilizador</label>
            <input value={editConfig.username} onChange={e => setEditConfig({ ...editConfig, username: e.target.value })} />
          </div>
          <div className="field">
            <label>Password</label>
            <input type="password" value={editConfig.password} onChange={e => setEditConfig({ ...editConfig, password: e.target.value })} />
          </div>
          <button className="btn-save" onClick={saveSettings}>Guardar</button>
        </div>
      )}

      {logMessages.length > 0 && (
        <div className="log-panel">
          {logMessages.map((msg, i) => (
            <div key={i} className="log-line">{msg}</div>
          ))}
        </div>
      )}

      <div className="footer">RLS Automação © 2026 · Todos os direitos reservados</div>
    </div>
  );
}

export default App;
