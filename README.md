# VPN_Interface — Raspberry Pi como Gateway VPN

Raspberry Pi configurado como **router/gateway** para acesso remoto à rede local industrial (PLCs Siemens) via ZeroTier + SoftEther VPN.

## Arquitectura

```
[ PC Remoto - Espanha ]
        │
        │  ZeroTier (túnel encriptado peer-to-peer)
        │
        ▼
[ Raspberry Pi - Portugal ]
  ZeroTier IP : 10.201.114.222  (interface: zt3hho244n)
  Ethernet IP : 192.168.222.253 (interface: eth0)
  Internet    : 192.168.1.75    (interface: wlan0)
        │
        │  NAT/Bridge -> rede local
        │
        ▼
[ Rede Local 192.168.222.0/24 ]
  PLCs, Switches Siemens, etc.
```

---

## Modos de Acesso

### Layer 3 — Acesso IP (via ZeroTier)
Para acesso geral à rede local (ping, web, SCADA, etc.).

**No PC remoto (Windows):**
```
route add 192.168.222.0 mask 255.255.255.0 10.201.114.222
```

**No PC remoto (Linux/Mac):**
```
sudo ip route add 192.168.222.0/24 via 10.201.114.222
```

Remover rota (Windows):
```
route delete 192.168.222.0
```

---

### Layer 2 — Acesso Ethernet real (via SoftEther sobre ZeroTier)
Para ferramentas que exigem Layer 2 como **Siemens PRONETA** (DCP/LLDP).

**Software necessário:** SoftEther VPN Client (softether.org)

**Configuracao da ligacao:**

| Campo      | Valor            |
|------------|------------------|
| Host       | 10.201.114.222   |
| Porta      | 443              |
| Hub        | DEFAULT          |
| Utilizador | luiz             |
| Password   | luiz1234         |

Apos ligar, o PC fica com IP na rede 192.168.222.x como se estivesse fisicamente ligado ao switch local. O PRONETA consegue fazer discovery DCP/LLDP normalmente.

---

## Componentes Configurados

### 1. ZeroTier
- **Rede:** b9a18a606f6713e2
- **ID do Raspberry:** 23c2952f4f
- **IP atribuido:** 10.201.114.222/24
- Allow Ethernet Bridging: ativado no ZeroTier Central

### 2. IP Forwarding (Kernel)
Ficheiro: `/etc/sysctl.d/99-zerotier-router.conf`
```
net.ipv4.ip_forward=1
```

### 3. iptables (NAT)
Ficheiro: `/etc/iptables/rules.v4`
- FORWARD: zt3hho244n -> eth0 (e retorno)
- MASQUERADE em eth0 (NAT para dispositivos locais)

### 4. SoftEther VPN Server
- Diretorio: /home/rls/vpnserver
- Versao: 4.42 Build 9798
- Hub: DEFAULT
- Local Bridge: DEFAULT -> eth0 (Layer 2 real)
- Portas: 443, 992, 1194, 5555

---

## Scripts

| Script | Descricao |
|--------|-----------|
| `scripts/setup-zerotier-router.sh` | Instala e configura ZeroTier + NAT (Layer 3) |
| `scripts/setup-softether-bridge.sh` | Configura SoftEther bridge Layer 2 |

---

## Porque Layer 2 para PRONETA?

O Siemens PRONETA usa **DCP** (Discovery and Configuration Protocol) e **LLDP** protocolos Layer 2 baseados em MAC Address que nao atravessam routers. Por isso o Layer 3 (ZeroTier sozinho) nao e suficiente para o PRONETA. O SoftEther em modo bridge cria uma extensao Layer 2 real da rede local.

| Ferramenta       | Layer 3 (ZeroTier) | Layer 2 (SoftEther) |
|------------------|--------------------|---------------------|
| Ping / SSH / Web | OK                 | OK                  |
| SCADA / OPC-UA   | OK                 | OK                  |
| Siemens PRONETA  | NAO                | OK                  |
| DCP / LLDP       | NAO                | OK                  |

---

## Rota ZeroTier para clientes

No ZeroTier Central (my.zerotier.com), adicionar em Managed Routes:
- Destination: 192.168.222.0/24
- Via: 10.201.114.222

Isto distribui a rota automaticamente a todos os membros autorizados.
