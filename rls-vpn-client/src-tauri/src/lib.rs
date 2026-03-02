use std::process::Command;
use std::sync::Mutex;
use std::path::PathBuf;
use tauri::{State, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

// Nome sem espaços — evita qualquer problema de parsing no vpncmd
const NIC_NAME: &str = "RLS_Automacao";

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

// Executa vpncmd e devolve stdout+stderr combinados.
// stdin = null para evitar que vpncmd fique à espera de input interactivo.
fn run_vpncmd(args: &[&str]) -> Result<String, String> {
    let vpncmd = find_vpncmd()
        .ok_or_else(|| "SoftEther VPN Client não instalado.".to_string())?;

    #[cfg(windows)]
    let output = Command::new(&vpncmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Erro ao executar vpncmd: {}", e))?;

    #[cfg(not(windows))]
    let output = Command::new(&vpncmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("Erro ao executar vpncmd: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Devolve stderr se stdout vazio (vpncmd às vezes usa stderr)
    if stdout.trim().is_empty() && !stderr.trim().is_empty() {
        Ok(stderr)
    } else {
        Ok(stdout)
    }
}

// Aguarda o serviço SevpnClient estar REALMENTE pronto.
// "The command completed" no output de NicList confirma que o serviço responde.
// Output não-vazio NÃO chega — "Cannot connect to VPN Client Service" é não-vazio mas indica falha.
fn wait_for_service_ready(max_secs: u32) -> bool {
    for _ in 0..max_secs {
        if let Ok(out) = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicList"]) {
            if out.contains("The command completed") {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    false
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
            .stdin(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("Erro ao iniciar installer: {}", e))?;

        if !status.success() {
            return Err("Instalação do SoftEther falhou.".to_string());
        }
    }

    std::thread::sleep(std::time::Duration::from_secs(4));

    #[cfg(windows)]
    {
        // Configurar serviço para arranque automático e iniciar
        let _ = Command::new("sc")
            .args(&["config", "SevpnClient", "start=auto"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = Command::new("sc")
            .args(&["start", "SevpnClient"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        std::thread::sleep(std::time::Duration::from_secs(3));
    }

    if softether_installed() {
        Ok(())
    } else {
        Err("Instalação concluída mas vpncmd.exe não encontrado.".to_string())
    }
}

// Remover adaptadores SoftEther antigos/órfãos antes de criar o novo
#[cfg(windows)]
fn cleanup_old_nics() {
    let old_names = ["VPN", "VPN Client", "VPN RLS Automacao", "RLS Automacao"];
    for name in &old_names {
        let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicDelete", name]);
    }
}

// Obter IP do adaptador VPN pelo nome definido em NIC_NAME
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
        // Garantir serviço em auto-start e activo
        #[cfg(windows)]
        {
            let _ = Command::new("sc")
                .args(&["config", "SevpnClient", "start=auto"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            let _ = Command::new("sc")
                .args(&["start", "SevpnClient"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }
        *state.softether_ready.lock().unwrap() = true;
        return Ok("Componentes VPN prontos.".to_string());
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

    // Verificação 1: AccountStatusGet reporta sessão estabelecida
    let session_up = output.contains("Connection Established");

    // Verificação 2: IP válido na placa (não APIPA) — confirma VPN activa
    // mesmo que AccountStatusGet ainda não reflicta o estado
    #[cfg(windows)]
    let local_ip = get_vpn_ip();
    #[cfg(not(windows))]
    let local_ip: Option<String> = None;

    let connected = session_up || local_ip.is_some();

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

    // 1. Configurar serviço para auto-start e iniciar
    #[cfg(windows)]
    {
        let _ = Command::new("sc")
            .args(&["config", "SevpnClient", "start=auto"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = Command::new("sc")
            .args(&["start", "SevpnClient"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    // 2. Aguardar o serviço estar REALMENTE pronto (até 30 segundos)
    // Detecta quando NicList responde com "The command completed"
    if !wait_for_service_ready(30) {
        return Err(
            "Serviço VPN não iniciou. Instala o SoftEther, reinicia o Windows e tenta de novo.".to_string()
        );
    }

    // 3. Criar adaptador apenas se ainda não existe
    let nic_already = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicList"])
        .map(|o| o.contains(NIC_NAME))
        .unwrap_or(false);

    if nic_already {
        // Placa já existe — não tocar, ir directo para AccountCreate/Connect
    } else {
        // Limpar antigos e criar novo adaptador
        #[cfg(windows)]
        cleanup_old_nics();

        let nic_out = run_vpncmd(&[
            "localhost", "/CLIENT", "/CMD", "NicCreate", NIC_NAME,
        ]).unwrap_or_default();

        if nic_out.contains("Cannot connect") {
            return Err(format!(
                "Falha ao criar adaptador VPN: {}",
                &nic_out[..nic_out.len().min(120)]
            ));
        }

        let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicEnable", NIC_NAME]);
    }

    // 6. AccountCreate — criar conta VPN (ignora erro se já existe)
    let _ = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountCreate", &config.account_name,
        &format!("/SERVER:{}:{}", config.host, config.port),
        &format!("/HUB:{}", config.hub),
        &format!("/USERNAME:{}", config.username),
        &format!("/NICNAME:{}", NIC_NAME),
    ]);

    // 7. AccountPasswordSet — definir password
    let _ = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountPasswordSet", &config.account_name,
        &format!("/PASSWORD:{}", config.password),
        "/TYPE:standard",
    ]);

    // 8. AccountConnect — ligar
    let result = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountConnect", &config.account_name,
    ]).unwrap_or_default();

    if result.contains("The command completed successfully") {
        *state.connected.lock().unwrap() = true;
        Ok("Adaptador criado. Ligado com sucesso à rede RLS Automação!".to_string())
    } else if result.contains("already") {
        *state.connected.lock().unwrap() = true;
        Ok("Já está ligado à rede RLS Automação.".to_string())
    } else if result.contains("Cannot connect") {
        Err("Serviço VPN não responde. Reinicia o Windows e tenta novamente.".to_string())
    } else {
        Err(format!("Falha ao ligar: {}", &result[..result.len().min(200)]))
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

// Reset completo — remove conta e adaptador
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
