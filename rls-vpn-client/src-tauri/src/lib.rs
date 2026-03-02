use std::process::Command;
use std::sync::Mutex;
use std::path::PathBuf;
use tauri::{State, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

// Nome do adaptador virtual — aparece no Painel de Controlo de Rede
const NIC_NAME: &str = "RLS Automacao";

struct VpnState {
    connected: Mutex<bool>,
    softether_ready: Mutex<bool>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct VpnConfig {
    pub host: String,
    pub port: u16,
    pub hub: String,
    pub username: String,
    pub password: String,
    pub account_name: String,
}

impl Default for VpnConfig {
    fn default() -> Self {
        VpnConfig {
            host: "10.201.114.222".to_string(),
            port: 443,
            hub: "DEFAULT".to_string(),
            username: "luiz".to_string(),
            password: "luiz1234".to_string(),
            account_name: "RLS_Automacao".to_string(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct VpnStatus {
    pub connected: bool,
    pub local_ip: Option<String>,
    pub message: String,
    pub softether_ready: bool,
}

fn find_vpncmd() -> Option<PathBuf> {
    let paths = [
        "C:\\Program Files\\SoftEther VPN Client\\vpncmd.exe",
        "C:\\Program Files (x86)\\SoftEther VPN Client\\vpncmd.exe",
    ];
    for p in &paths {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    None
}

fn softether_installed() -> bool {
    find_vpncmd().is_some()
}

fn install_softether_silent(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let installer_path = app_handle
        .path()
        .resource_dir()
        .map_err(|e: tauri::Error| e.to_string())?
        .join("se_client.exe");

    if !installer_path.exists() {
        return Err("Installer SoftEther não encontrado nos resources da app.".to_string());
    }

    #[cfg(windows)]
    {
        let status = Command::new(&installer_path)
            .args(&["/SILENT", "/NORESTART"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("Erro ao iniciar installer: {}", e))?;

        if !status.success() {
            return Err("Instalação do SoftEther falhou.".to_string());
        }
    }

    std::thread::sleep(std::time::Duration::from_secs(3));

    #[cfg(windows)]
    {
        let _ = Command::new("sc")
            .args(&["start", "SevpnClient"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    if softether_installed() {
        Ok(())
    } else {
        Err("Instalação concluída mas vpncmd.exe não encontrado.".to_string())
    }
}

// Aguarda o serviço SevpnClient ficar pronto para receber comandos.
// Tenta NicList repetidamente até obter resposta (máx. max_secs segundos).
fn wait_for_service_ready(max_secs: u32) -> bool {
    for _ in 0..max_secs {
        if let Ok(out) = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicList"]) {
            if !out.is_empty() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    false
}

fn run_vpncmd(args: &[&str]) -> Result<String, String> {
    let vpncmd = find_vpncmd()
        .ok_or_else(|| "SoftEther VPN Client não instalado.".to_string())?;

    #[cfg(windows)]
    let output = Command::new(&vpncmd)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Erro ao executar vpncmd: {}", e))?;

    #[cfg(not(windows))]
    let output = Command::new(&vpncmd)
        .args(args)
        .output()
        .map_err(|e| format!("Erro ao executar vpncmd: {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// Remover adaptadores SoftEther antigos/órfãos (ex: "VPN", "VPN Client", "VPN RLS Automacao")
#[cfg(windows)]
fn cleanup_old_nics() {
    let old_names = ["VPN", "VPN Client", "VPN RLS Automacao"];
    for name in &old_names {
        let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicDelete", name]);
    }
}

// Renomear adaptador virtual para NIC_NAME via PowerShell
#[cfg(windows)]
fn rename_nic() {
    // Tenta renomear de qualquer nome que o SoftEther tenha criado
    let script = format!(
        "Get-NetAdapter | Where-Object {{ $_.InterfaceDescription -like '*SoftEther*' -or $_.Name -eq 'VPN' -or $_.Name -eq 'VPN Client' }} | Rename-NetAdapter -NewName '{}'",
        NIC_NAME
    );
    let _ = Command::new("powershell")
        .args(&["-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

// Obter IP do adaptador "VPN RLS Automacao" — dinâmico, qualquer rede
#[cfg(windows)]
fn get_vpn_ip() -> Option<String> {
    let script = format!(
        "Get-NetIPAddress -InterfaceAlias '{}' -AddressFamily IPv4 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty IPAddress",
        NIC_NAME
    );
    let output = Command::new("powershell")
        .args(&["-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if ip.is_empty() || ip.starts_with("169.254") {
        None
    } else {
        Some(ip)
    }
}

#[tauri::command]
fn get_default_config() -> VpnConfig {
    VpnConfig::default()
}

#[tauri::command]
fn check_and_install_softether(
    app_handle: tauri::AppHandle,
    state: State<VpnState>,
) -> Result<String, String> {
    if softether_installed() {
        *state.softether_ready.lock().unwrap() = true;
        return Ok("Componentes VPN já instalados.".to_string());
    }
    install_softether_silent(&app_handle)?;
    *state.softether_ready.lock().unwrap() = true;
    Ok("Componentes VPN instalados com sucesso!".to_string())
}

#[tauri::command]
fn get_status(state: State<VpnState>) -> VpnStatus {
    let se_ready = softether_installed();
    *state.softether_ready.lock().unwrap() = se_ready;

    if !se_ready {
        return VpnStatus {
            connected: false,
            local_ip: None,
            message: "A instalar componentes VPN...".to_string(),
            softether_ready: false,
        };
    }

    let output = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD", "AccountStatusGet", "RLS_Automacao",
    ]).unwrap_or_default();

    let connected = output.contains("Connection Established")
        || output.contains("Connected");

    #[cfg(windows)]
    let local_ip = if connected { get_vpn_ip() } else { None };
    #[cfg(not(windows))]
    let local_ip: Option<String> = None;

    *state.connected.lock().unwrap() = connected;

    VpnStatus {
        connected,
        local_ip,
        message: if connected {
            "Ligado à rede RLS Automação".to_string()
        } else {
            "Desligado".to_string()
        },
        softether_ready: true,
    }
}

#[tauri::command]
fn connect(config: VpnConfig, state: State<VpnState>) -> Result<String, String> {
    if !softether_installed() {
        return Err("Componentes VPN não instalados. Aguarda a instalação automática.".to_string());
    }

    // Garantir serviço activo e aguardar estar pronto (até 30 segundos)
    #[cfg(windows)]
    {
        let _ = Command::new("sc")
            .args(&["start", "SevpnClient"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    if !wait_for_service_ready(30) {
        return Err("SoftEther VPN Client não respondeu. Reinicia a aplicação e tenta novamente.".to_string());
    }

    // Limpar adaptadores antigos/órfãos do SoftEther
    #[cfg(windows)]
    cleanup_old_nics();

    // Criar adaptador virtual com nome RLS (ignora erro se já existe)
    let nic_result = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "NicCreate", NIC_NAME,
    ]);
    if let Err(ref e) = nic_result {
        // Não é fatal — pode já existir
        let _ = e;
    }

    // Activar adaptador explicitamente
    let _ = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "NicEnable", NIC_NAME,
    ]);

    // Criar conta apontada para o adaptador RLS
    let _ = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountCreate", &config.account_name,
        &format!("/SERVER:{}:{}", config.host, config.port),
        &format!("/HUB:{}", config.hub),
        &format!("/USERNAME:{}", config.username),
        &format!("/NICNAME:{}", NIC_NAME),
    ]);

    // Definir password
    let _ = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountPasswordSet", &config.account_name,
        &format!("/PASSWORD:{}", config.password),
        "/TYPE:standard",
    ]);

    // Ligar
    let result = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountConnect", &config.account_name,
    ])?;

    // Renomear adaptador após ligar (garante nome correcto no Painel de Rede)
    #[cfg(windows)]
    rename_nic();

    if result.contains("completed successfully") || result.contains("The command completed") {
        *state.connected.lock().unwrap() = true;
        Ok("Ligado com sucesso à rede RLS Automação!".to_string())
    } else if result.contains("already") {
        *state.connected.lock().unwrap() = true;
        Ok("Já está ligado à rede RLS Automação.".to_string())
    } else {
        Err(format!("Erro: {}", result.trim()))
    }
}

#[tauri::command]
fn disconnect(config: VpnConfig, state: State<VpnState>) -> Result<String, String> {
    let _ = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountDisconnect", &config.account_name,
    ]);
    *state.connected.lock().unwrap() = false;
    Ok("Desligado com sucesso.".to_string())
}

// Limpeza completa para testes — remove conta e adaptador
#[tauri::command]
fn clean_reset(config: VpnConfig) -> Result<String, String> {
    let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "AccountDisconnect", &config.account_name]);
    let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "AccountDelete", &config.account_name]);
    let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicDelete", NIC_NAME]);
    Ok("Reset completo. Adaptador e conta removidos.".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(VpnState {
            connected: Mutex::new(false),
            softether_ready: Mutex::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            get_default_config,
            get_status,
            connect,
            disconnect,
            check_and_install_softether,
            clean_reset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
