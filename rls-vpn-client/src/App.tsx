import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Shield, ShieldOff, Settings, Wifi, WifiOff,
  ChevronDown, ChevronUp, Loader, Network,
} from "lucide-react";
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
  connection_ready: boolean;
  vpn_state: string;
}

function App() {
  const [config, setConfig] = useState<VpnConfig | null>(null);
  const [status, setStatus] = useState<VpnStatus>({
    connected: false,
    message: "A iniciar...",
    softether_ready: false,
    connection_ready: false,
    vpn_state: "not_installed",
  });
  const [setupLoading, setSetupLoading] = useState(false);
  const [connectLoading, setConnectLoading] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [logMessages, setLogMessages] = useState<string[]>([]);
  const [editConfig, setEditConfig] = useState<VpnConfig | null>(null);

  const addLog = (msg: string) => {
    const time = new Date().toLocaleTimeString("pt-PT");
    setLogMessages(prev => [`[${time}] ${msg}`, ...prev].slice(0, 20));
  };

  const refreshStatus = async () => {
    try {
      const s = await invoke<VpnStatus>("get_status");
      setStatus(s);
    } catch (_) {}
  };

  useEffect(() => {
    invoke<VpnConfig>("get_default_config").then(cfg => {
      setConfig(cfg);
      setEditConfig(cfg);
    });

    const ensureReady = async () => {
      setInstalling(true);
      try {
        const msg = await invoke<string>("check_and_install_softether");
        addLog(msg);
        await refreshStatus();
      } catch (e: any) {
        addLog(`Aviso: ${e}`);
      } finally {
        setInstalling(false);
      }
    };
    ensureReady();
  }, []);

  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 4000);
    return () => clearInterval(interval);
  }, []);

  // Botão 1: Configurar placa de rede + conta VPN
  const handleSetup = async () => {
    if (!config) return;
    setSetupLoading(true);
    addLog("A criar adaptador de rede e conta VPN...");
    try {
      const msg = await invoke<string>("setup_connection", { config });
      addLog(msg);
      await refreshStatus();
    } catch (e: any) {
      addLog(`Erro ao configurar: ${e}`);
    } finally {
      setSetupLoading(false);
    }
  };

  // Botão 2: Ligar (AccountConnect)
  const handleConnect = async () => {
    if (!config) return;
    setConnectLoading(true);
    addLog("A ligar à rede RLS Automação...");
    try {
      const msg = await invoke<string>("connect", { config });
      addLog(msg);
      await refreshStatus();
    } catch (e: any) {
      addLog(`Erro ao ligar: ${e}`);
    } finally {
      setConnectLoading(false);
    }
  };

  // Botão 2: Desligar (AccountDisconnect)
  const handleDisconnect = async () => {
    if (!config) return;
    setConnectLoading(true);
    addLog("A desligar...");
    try {
      const msg = await invoke<string>("disconnect", { config });
      addLog(msg);
      await refreshStatus();
    } catch (e: any) {
      addLog(`Erro ao desligar: ${e}`);
    } finally {
      setConnectLoading(false);
    }
  };

  const saveSettings = () => {
    if (editConfig) {
      setConfig(editConfig);
      setShowSettings(false);
      addLog("Configurações guardadas.");
    }
  };

  const statusClass =
    status.vpn_state === "connected" ? "connected"
    : status.vpn_state === "connecting" ? "connecting"
    : status.vpn_state === "not_configured" ? "unconfigured"
    : "disconnected";

  const statusLabel =
    status.vpn_state === "connected" ? "LIGADO"
    : status.vpn_state === "connecting" ? "A LIGAR..."
    : status.vpn_state === "not_configured" ? "NÃO CONFIGURADO"
    : "DESLIGADO";

  const statusIconColor =
    status.vpn_state === "connected" ? "#00ff88"
    : status.vpn_state === "connecting" ? "#ffaa00"
    : status.vpn_state === "not_configured" ? "#8b949e"
    : "#ff4466";

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

      {/* STATUS CARD — estado real do SoftEther */}
      <div className={`status-card ${statusClass}`}>
        <div className="status-icon">
          {status.vpn_state === "connected"
            ? <Wifi size={48} color={statusIconColor} />
            : <WifiOff size={48} color={statusIconColor} />}
        </div>
        <div className="status-text">{statusLabel}</div>
        <div className="status-sub">{status.message}</div>
        {status.connected && (
          <div className="network-info">
            {status.local_ip
              ? <span className="badge">{status.local_ip}</span>
              : <span className="badge">A obter IP...</span>}
            <span className="badge">Layer 2</span>
          </div>
        )}
      </div>

      {/* ÁREA DE ACÇÃO — 2 botões independentes */}
      <div className="action-area">
        {installing ? (
          <div className="btn-row">
            <button className="btn-action btn-loading" disabled style={{ flex: 1 }}>
              <Loader size={16} className="spin" />
              A instalar componentes VPN...
            </button>
          </div>
        ) : !status.softether_ready ? (
          <div className="btn-row">
            <button className="btn-action btn-loading" disabled style={{ flex: 1 }}>
              SoftEther não disponível
            </button>
          </div>
        ) : (
          <div className="btn-row">
            {/* Botão 1: Configurar (sempre visível quando SoftEther pronto) */}
            {setupLoading ? (
              <button className="btn-action btn-loading" disabled>
                <Loader size={16} className="spin" />
                A configurar...
              </button>
            ) : (
              <button className="btn-action btn-setup" onClick={handleSetup} disabled={connectLoading}>
                <Network size={16} />
                Configurar
              </button>
            )}

            {/* Botão 2: Ligar / Desligar (visível quando configurado) */}
            {status.connection_ready && (
              connectLoading ? (
                <button className="btn-action btn-loading" disabled>
                  <Loader size={16} className="spin" />
                  {status.connected ? "A desligar..." : "A ligar..."}
                </button>
              ) : status.vpn_state === "connecting" ? (
                <button className="btn-action btn-loading" disabled>
                  <Loader size={16} className="spin" />
                  A ligar...
                </button>
              ) : status.connected ? (
                <button className="btn-action btn-disconnect" onClick={handleDisconnect} disabled={setupLoading}>
                  <ShieldOff size={16} />
                  Desligar
                </button>
              ) : (
                <button className="btn-action btn-connect" onClick={handleConnect} disabled={setupLoading}>
                  <Shield size={16} />
                  Ligar
                </button>
              )
            )}
          </div>
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
          <div className="info-row">
            <span className="info-label">Conta VPN</span>
            <span className="info-value">{config.account_name}</span>
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
            <input
              value={editConfig.host}
              onChange={e => setEditConfig({ ...editConfig, host: e.target.value })}
            />
          </div>
          <div className="field">
            <label>Porta</label>
            <input
              type="number"
              value={editConfig.port}
              onChange={e => setEditConfig({ ...editConfig, port: Number(e.target.value) })}
            />
          </div>
          <div className="field">
            <label>Hub</label>
            <input
              value={editConfig.hub}
              onChange={e => setEditConfig({ ...editConfig, hub: e.target.value })}
            />
          </div>
          <div className="field">
            <label>Utilizador</label>
            <input
              value={editConfig.username}
              onChange={e => setEditConfig({ ...editConfig, username: e.target.value })}
            />
          </div>
          <div className="field">
            <label>Password</label>
            <input
              type="password"
              value={editConfig.password}
              onChange={e => setEditConfig({ ...editConfig, password: e.target.value })}
            />
          </div>
          <button className="btn-save" onClick={saveSettings}>Guardar</button>
          <button
            className="btn-reset"
            onClick={async () => {
              if (!config) return;
              addLog("A fazer reset completo...");
              try {
                const msg = await invoke<string>("clean_reset", { config });
                addLog(msg);
                await refreshStatus();
              } catch (e: any) {
                addLog(`Erro: ${e}`);
              }
            }}
          >
            Limpar instalação (reset)
          </button>
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
