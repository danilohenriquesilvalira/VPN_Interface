use std::process::Command;
use std::sync::Mutex;
use std::path::PathBuf;
use tauri::{State, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

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

// Localizar vpncmd.exe no sistema
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

// Verificar se SoftEther Client está instalado
fn softether_installed() -> bool {
    find_vpncmd().is_some()
}

// Instalar SoftEther VPN Client silenciosamente a partir do installer bundled
fn install_softether_silent(app_handle: &tauri::AppHandle) -> Result<(), String> {
    // Encontrar o installer bundled com a app
    let installer_path = app_handle
        .path()
        .resource_dir()
        .map_err(|e: tauri::Error| e.to_string())?
        .join("se_client.exe");

    if !installer_path.exists() {
        return Err("Installer SoftEther não encontrado nos resources da app.".to_string());
    }

    // Instalar silenciosamente (requer admin — o utilizador verá UAC uma vez)
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

    // Aguardar serviço ficar activo
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Iniciar serviço
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

// Executar vpncmd sem janelas
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

#[cfg(windows)]
fn get_vpn_ip() -> Option<String> {
    let output = Command::new("ipconfig")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    for line in text.lines() {
        if line.contains("192.168.222") && line.contains("IPv4") {
            if let Some(ip) = line.split(':').last() {
                return Some(ip.trim().to_string());
            }
        }
    }
    None
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
        return Ok("SoftEther VPN Client já instalado.".to_string());
    }

    install_softether_silent(&app_handle)?;
    *state.softether_ready.lock().unwrap() = true;
    Ok("SoftEther VPN Client instalado com sucesso!".to_string())
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

    // Garantir serviço activo
    #[cfg(windows)]
    {
        let _ = Command::new("sc")
            .args(&["start", "SevpnClient"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Criar conta (ignora erro se já existe)
    let _ = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountCreate", &config.account_name,
        &format!("/SERVER:{}:{}", config.host, config.port),
        &format!("/HUB:{}", config.hub),
        &format!("/USERNAME:{}", config.username),
        "/NICNAME:VPN",
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
