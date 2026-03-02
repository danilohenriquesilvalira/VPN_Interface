use std::process::Command;
use std::sync::Mutex;
use tauri::State;

// Estado global da ligação
struct VpnState {
    connected: Mutex<bool>,
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
    pub message: String,
}

// Verificar se vpncmd está disponível
fn find_vpncmd() -> Option<String> {
    let candidates = vec![
        "vpncmd",
        "C:\\Program Files\\SoftEther VPN Client\\vpncmd.exe",
        "C:\\Program Files (x86)\\SoftEther VPN Client\\vpncmd.exe",
    ];
    for path in candidates {
        if Command::new(path).arg("/VERSION").output().is_ok() {
            return Some(path.to_string());
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
    let connected = *state.connected.lock().unwrap();

    // Verificar se o adaptador VPN está ativo (Windows)
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("ipconfig").output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            let active = text.contains("VPN") || text.contains("192.168.222");
            return VpnStatus {
                connected: active,
                message: if active {
                    "Ligado à rede RLS".to_string()
                } else {
                    "Desligado".to_string()
                },
            };
        }
    }

    VpnStatus {
        connected,
        message: if connected {
            "Ligado à rede RLS".to_string()
        } else {
            "Desligado".to_string()
        },
    }
}

#[tauri::command]
fn connect(config: VpnConfig, state: State<VpnState>) -> Result<String, String> {
    let vpncmd = find_vpncmd().ok_or(
        "SoftEther VPN Client não encontrado. Por favor instala o SoftEther VPN Client.".to_string()
    )?;

    // Criar conta VPN (ignora erro se já existir)
    let create_cmd = format!(
        "AccountCreate {} /SERVER:{}:{} /HUB:{} /USERNAME:{} /NICNAME:VPN",
        config.account_name, config.host, config.port, config.hub, config.username
    );
    let _ = Command::new(&vpncmd)
        .args(&["/CLIENT", "localhost", "/CMD", &create_cmd])
        .output();

    // Definir password
    let pass_cmd = format!(
        "AccountPasswordSet {} /PASSWORD:{} /TYPE:standard",
        config.account_name, config.password
    );
    let _ = Command::new(&vpncmd)
        .args(&["/CLIENT", "localhost", "/CMD", &pass_cmd])
        .output();

    // Ligar
    let connect_cmd = format!("AccountConnect {}", config.account_name);
    let output = Command::new(&vpncmd)
        .args(&["/CLIENT", "localhost", "/CMD", &connect_cmd])
        .output()
        .map_err(|e| e.to_string())?;

    let result = String::from_utf8_lossy(&output.stdout).to_string();

    if output.status.success() || result.contains("completed successfully") {
        *state.connected.lock().unwrap() = true;
        Ok("Ligado com sucesso à rede RLS Automação!".to_string())
    } else {
        Err(format!("Erro ao ligar: {}", result))
    }
}

#[tauri::command]
fn disconnect(config: VpnConfig, state: State<VpnState>) -> Result<String, String> {
    let vpncmd = find_vpncmd().ok_or("SoftEther VPN Client não encontrado.".to_string())?;

    let disconnect_cmd = format!("AccountDisconnect {}", config.account_name);
    let output = Command::new(&vpncmd)
        .args(&["/CLIENT", "localhost", "/CMD", &disconnect_cmd])
        .output()
        .map_err(|e| e.to_string())?;

    *state.connected.lock().unwrap() = false;

    if output.status.success() {
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
        })
        .invoke_handler(tauri::generate_handler![
            get_default_config,
            get_status,
            connect,
            disconnect
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
