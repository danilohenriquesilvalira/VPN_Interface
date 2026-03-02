use std::process::Command;
use std::sync::Mutex;
use std::path::PathBuf;
use tauri::State;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// Flag Windows para ocultar janela CMD
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

struct VpnState {
    connected: Mutex<bool>,
    process_id: Mutex<Option<u32>>,
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
}

// Localizar vpncmd: primeiro dentro da app (bundled), depois no sistema
fn find_vpncmd() -> Option<PathBuf> {
    // 1. Bundled junto com a app (preferido)
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let bundled = exe_dir.join("vpncmd.exe");
    if bundled.exists() {
        return Some(bundled);
    }

    // 2. SoftEther Client instalado no sistema (fallback)
    let system_paths = [
        "C:\\Program Files\\SoftEther VPN Client\\vpncmd.exe",
        "C:\\Program Files (x86)\\SoftEther VPN Client\\vpncmd.exe",
        "vpncmd.exe",
    ];
    for path in &system_paths {
        if PathBuf::from(path).exists() {
            return Some(PathBuf::from(path));
        }
    }
    None
}

// Executar vpncmd sem janela visível
fn run_vpncmd(args: &[&str]) -> Result<String, String> {
    let vpncmd = find_vpncmd()
        .ok_or_else(|| "vpncmd.exe não encontrado. Instala o SoftEther VPN Client.".to_string())?;

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

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !stderr.is_empty() {
        Ok(format!("{}\n{}", stdout, stderr))
    } else {
        Ok(stdout)
    }
}

// Verificar IP da ligação VPN (adaptador virtual SoftEther)
#[cfg(windows)]
fn get_vpn_ip() -> Option<String> {
    let output = Command::new("ipconfig")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();

    // Procurar IP na gama da rede RLS
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
fn get_status(state: State<VpnState>) -> VpnStatus {
    #[cfg(windows)]
    {
        // Verificar estado real via vpncmd
        if let Ok(output) = run_vpncmd(&[
            "localhost", "/CLIENT", "/CMD", "AccountStatusGet", "RLS_Automacao"
        ]) {
            let connected = output.contains("Connected")
                || output.contains("Session Status|Connection Established");
            let local_ip = get_vpn_ip();
            *state.connected.lock().unwrap() = connected;
            return VpnStatus {
                connected,
                local_ip,
                message: if connected {
                    "Ligado à rede RLS Automação".to_string()
                } else {
                    "Desligado".to_string()
                },
            };
        }
    }

    let connected = *state.connected.lock().unwrap();
    VpnStatus {
        connected,
        local_ip: None,
        message: if connected { "Ligado".to_string() } else { "Desligado".to_string() },
    }
}

#[tauri::command]
fn connect(config: VpnConfig, state: State<VpnState>) -> Result<String, String> {
    // 1. Garantir que o serviço SoftEther Client está activo
    #[cfg(windows)]
    {
        let _ = Command::new("sc")
            .args(&["start", "SevpnClient"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    // 2. Criar conta (ignora erro se já existir)
    let _ = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountCreate", &config.account_name,
        &format!("/SERVER:{}:{}", config.host, config.port),
        &format!("/HUB:{}", config.hub),
        &format!("/USERNAME:{}", config.username),
        "/NICNAME:VPN",
    ]);

    // 3. Definir tipo de autenticação e password
    let _ = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountPasswordSet", &config.account_name,
        &format!("/PASSWORD:{}", config.password),
        "/TYPE:standard",
    ]);

    // 4. Ligar
    let result = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountConnect", &config.account_name,
    ])?;

    if result.contains("completed successfully") || result.contains("The command completed") {
        *state.connected.lock().unwrap() = true;
        Ok("Ligado com sucesso à rede RLS Automação!".to_string())
    } else if result.contains("already") || result.contains("já está") {
        *state.connected.lock().unwrap() = true;
        Ok("Já estava ligado à rede RLS Automação.".to_string())
    } else {
        Err(format!("Resposta do servidor: {}", result.trim()))
    }
}

#[tauri::command]
fn disconnect(config: VpnConfig, state: State<VpnState>) -> Result<String, String> {
    let result = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountDisconnect", &config.account_name,
    ])?;

    *state.connected.lock().unwrap() = false;

    if result.contains("completed successfully") || result.contains("The command completed") {
        Ok("Desligado com sucesso.".to_string())
    } else {
        Ok("Desligado.".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(VpnState {
            connected: Mutex::new(false),
            process_id: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_default_config,
            get_status,
            connect,
            disconnect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
