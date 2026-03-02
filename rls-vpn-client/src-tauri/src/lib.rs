use std::process::Command;
use std::sync::Mutex;
use std::path::PathBuf;
use tauri::{State, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const NIC_NAME: &str = "RLS_Automacao";

struct VpnState {
    softether_ready: Mutex<bool>,
    connection_ready: Mutex<bool>,
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
    pub connection_ready: bool,
    /// "not_installed" | "not_configured" | "disconnected" | "connecting" | "connected"
    pub vpn_state: String,
    /// Output bruto do AccountStatusGet (para diagnóstico no log)
    pub raw_status: String,
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

/// Executa vpncmd e devolve stdout + stderr combinados para não perder erros.
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

    // Combinar stdout+stderr para não perder mensagens de erro
    let combined = format!("{}\n{}", stdout, stderr);
    Ok(combined.trim().to_string())
}

/// Espera até o serviço SevpnClient responder ao NicList com "The command completed".
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

#[cfg(windows)]
fn cleanup_old_nics() {
    let old_names = ["VPN", "VPN Client", "VPN RLS Automacao", "RLS Automacao"];
    for name in &old_names {
        let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicDelete", name]);
    }
}

#[cfg(windows)]
fn get_vpn_ip() -> Option<String> {
    // SoftEther cria o adaptador Windows como "VPN Client Adapter - {nome}".
    // Tentamos nomes específicos e depois fallback por descrição TAP/SoftEther.
    let script = format!(
        r#"
$names = @('{nic}', 'VPN Client Adapter - {nic}', 'VPN - {nic}')
foreach ($n in $names) {{
    $ip = Get-NetIPAddress -InterfaceAlias $n -AddressFamily IPv4 -ErrorAction SilentlyContinue |
          Where-Object {{ $_.IPAddress -notlike '169.254.*' }} |
          Select-Object -ExpandProperty IPAddress -First 1
    if ($ip) {{ Write-Output $ip; exit }}
}}
# Fallback: qualquer adaptador SoftEther ou TAP com IP válido
Get-NetAdapter -ErrorAction SilentlyContinue |
  Where-Object {{ $_.InterfaceDescription -like '*SoftEther*' -or
                  $_.InterfaceDescription -like '*tap0901*' -or
                  $_.InterfaceDescription -like '*TAP-Windows*' }} |
  ForEach-Object {{
    $ip = $_ | Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
               Where-Object {{ $_.IPAddress -notlike '169.254.*' }} |
               Select-Object -ExpandProperty IPAddress -First 1
    if ($ip) {{ Write-Output $ip; exit }}
  }}
"#,
        nic = NIC_NAME
    );
    let output = Command::new("powershell")
        .args(&["-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let ip = String::from_utf8_lossy(&output.stdout)
        .trim()
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
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

/// Cria a placa de rede virtual e a conta VPN no SoftEther (idempotente).
/// Aguarda o serviço após NicCreate porque o SevpnClient pode reiniciar
/// ao inicializar o novo adaptador.
#[tauri::command]
fn setup_connection(config: VpnConfig, state: State<VpnState>) -> Result<String, String> {
    if !softether_installed() {
        return Err("SoftEther não instalado. Aguarda a instalação automática.".to_string());
    }

    // Garantir serviço activo
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

    if !wait_for_service_ready(30) {
        return Err(
            "Serviço VPN não iniciou. Reinicia o Windows e tenta novamente.".to_string(),
        );
    }

    // Criar adaptador de rede se não existe
    let nic_exists = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicList"])
        .map(|o| o.contains(NIC_NAME))
        .unwrap_or(false);

    if !nic_exists {
        #[cfg(windows)]
        cleanup_old_nics();

        let nic_out = run_vpncmd(&[
            "localhost", "/CLIENT", "/CMD", "NicCreate", NIC_NAME,
        ])
        .unwrap_or_default();

        if nic_out.contains("Cannot connect") {
            return Err(format!(
                "Falha ao criar adaptador: {}",
                &nic_out[..nic_out.len().min(200)]
            ));
        }

        // O SevpnClient pode reiniciar após NicCreate para inicializar o adaptador.
        // Aguardar até o serviço estar novamente pronto antes de prosseguir.
        std::thread::sleep(std::time::Duration::from_secs(2));
        if !wait_for_service_ready(30) {
            return Err(
                "Serviço VPN não retomou após criar adaptador. Tenta novamente.".to_string(),
            );
        }

        let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicEnable", NIC_NAME]);
    }

    // Criar/actualizar conta VPN
    let acc_out = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountCreate", &config.account_name,
        &format!("/SERVER:{}:{}", config.host, config.port),
        &format!("/HUB:{}", config.hub),
        &format!("/USERNAME:{}", config.username),
        &format!("/NICNAME:{}", NIC_NAME),
    ])
    .unwrap_or_default();

    if acc_out.contains("Cannot connect to VPN Client") {
        return Err("Serviço VPN não responde ao criar conta. Tenta novamente.".to_string());
    }

    // Definir password (funciona mesmo se conta já existia)
    let pw_out = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountPasswordSet", &config.account_name,
        &format!("/PASSWORD:{}", config.password),
        "/TYPE:standard",
    ])
    .unwrap_or_default();

    if pw_out.contains("Cannot connect to VPN Client") {
        return Err("Serviço VPN não responde ao definir password. Tenta novamente.".to_string());
    }

    *state.connection_ready.lock().unwrap() = true;
    Ok("Placa de rede e conta VPN configuradas com sucesso.".to_string())
}

/// Liga a VPN (AccountConnect).
/// Aguarda o serviço estar pronto antes de tentar ligar.
#[tauri::command]
fn connect(config: VpnConfig) -> Result<String, String> {
    if !softether_installed() {
        return Err("SoftEther não instalado.".to_string());
    }

    // Garantir que o serviço está activo
    #[cfg(windows)]
    {
        let _ = Command::new("sc")
            .args(&["start", "SevpnClient"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    // Aguardar o serviço estar realmente pronto (até 15 segundos)
    // Sem isto o vpncmd imprime o banner em vez de executar o comando
    if !wait_for_service_ready(15) {
        return Err(
            "Serviço VPN não responde. Aguarda alguns segundos e tenta de novo.".to_string(),
        );
    }

    let result = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountConnect", &config.account_name,
    ])
    .unwrap_or_default();

    if result.contains("The command completed successfully") {
        Ok("Ligação iniciada com sucesso.".to_string())
    } else if result.contains("already") || result.contains("Already") {
        Ok("Já está ligado à rede RLS Automação.".to_string())
    } else if result.contains("Cannot connect to VPN Client") {
        Err("Serviço VPN não responde. Aguarda e tenta novamente.".to_string())
    } else {
        Err(format!("Falha ao ligar: {}", &result[..result.len().min(300)]))
    }
}

/// Desliga a VPN (AccountDisconnect).
#[tauri::command]
fn disconnect(config: VpnConfig) -> Result<String, String> {
    let result = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD",
        "AccountDisconnect", &config.account_name,
    ])
    .unwrap_or_default();

    if result.contains("The command completed successfully")
        || result.contains("not connected")
        || result.contains("Not Connected")
    {
        Ok("Desligado com sucesso.".to_string())
    } else {
        Ok(format!("Desligado. ({})", &result[..result.len().min(80)]))
    }
}

/// Lê o estado real do SoftEther via AccountStatusGet + IP da placa.
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
            connection_ready: false,
            vpn_state: "not_installed".to_string(),
            raw_status: String::new(),
        };
    }

    let output = run_vpncmd(&[
        "localhost", "/CLIENT", "/CMD", "AccountStatusGet", NIC_NAME,
    ])
    .unwrap_or_default();

    // Mostrar a PARTE ÚTIL do output (depois do banner) para diagnóstico.
    // O banner ocupa ~280 chars; o status real aparece depois.
    let raw_status = {
        // Tentar começar no "AccountStatusGet command" ou "Session Status"
        let start = output.find("AccountStatusGet command")
            .or_else(|| output.find("Session Status"))
            .or_else(|| output.find("Connected to VPN Client"))
            .unwrap_or_else(|| output.len().saturating_sub(500));
        output[start..].chars().take(500).collect::<String>()
            .replace('\n', " | ").replace('\r', "")
    };

    // Serviço não responde — mantemos o último estado conhecido de connection_ready
    if output.contains("Cannot connect to VPN Client") {
        return VpnStatus {
            connected: false,
            local_ip: None,
            message: "Serviço VPN não responde".to_string(),
            softether_ready: true,
            connection_ready: *state.connection_ready.lock().unwrap(),
            vpn_state: "disconnected".to_string(),
            raw_status,
        };
    }

    // Conta não existe → precisa de configuração
    let lower = output.to_lowercase();
    let connection_ready = !lower.contains("not found") && !output.trim().is_empty();
    *state.connection_ready.lock().unwrap() = connection_ready;

    if !connection_ready {
        return VpnStatus {
            connected: false,
            local_ip: None,
            message: "Conta VPN não configurada".to_string(),
            softether_ready: true,
            connection_ready: false,
            vpn_state: "not_configured".to_string(),
            raw_status,
        };
    }

    // Verificação 1: AccountStatusGet — vários padrões possíveis do SoftEther 4.42
    let session_up = lower.contains("connection established")
        || lower.contains("session status") && !lower.contains("not connected")
            && !lower.contains("not connect") && lower.contains("connect");

    // Verificação 2: IP da placa VPN no Windows
    // SoftEther cria o adaptador como "VPN Client Adapter - {NIC_NAME}"
    // get_vpn_ip() tenta vários formatos de nome
    #[cfg(windows)]
    let local_ip = get_vpn_ip();
    #[cfg(not(windows))]
    let local_ip: Option<String> = None;

    // Ligado se qualquer das verificações confirmar
    let connected = session_up || local_ip.is_some();

    let vpn_state = if connected {
        "connected"
    } else if lower.contains("connecting") {
        "connecting"
    } else {
        "disconnected"
    };

    let message = match vpn_state {
        "connected"  => "Ligado à rede RLS Automação".to_string(),
        "connecting" => "A estabelecer ligação...".to_string(),
        _            => "Pronto para ligar".to_string(),
    };

    VpnStatus {
        connected,
        local_ip,
        message,
        softether_ready: true,
        connection_ready: true,
        vpn_state: vpn_state.to_string(),
        raw_status,
    }
}

/// Reset completo — remove conta e adaptador de rede.
#[tauri::command]
fn clean_reset(config: VpnConfig, state: State<VpnState>) -> Result<String, String> {
    let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "AccountDisconnect", &config.account_name]);
    let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "AccountDelete", &config.account_name]);
    let _ = run_vpncmd(&["localhost", "/CLIENT", "/CMD", "NicDelete", NIC_NAME]);
    *state.connection_ready.lock().unwrap() = false;
    Ok("Reset completo. Adaptador e conta removidos.".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(VpnState {
            softether_ready: Mutex::new(false),
            connection_ready: Mutex::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            get_default_config,
            get_status,
            connect,
            disconnect,
            setup_connection,
            check_and_install_softether,
            clean_reset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
