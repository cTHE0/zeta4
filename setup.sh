#!/bin/bash
# Zeta Network — Installation d'un relais
# Usage: curl -sL https://raw.githubusercontent.com/cTHE0/zeta4/main/setup.sh | bash
set -e

echo "⚡ Installation du relais Zeta Network..."

# Dépendances
apt update -qq
apt install -y -qq build-essential pkg-config libssl-dev git curl nginx openssl

# Rust
if ! command -v cargo &>/dev/null; then
    echo "📦 Installation de Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Code source
if [ -d /root/zeta4 ]; then
    cd /root/zeta4 && git pull
else
    git clone https://github.com/cTHE0/zeta4.git /root/zeta4
fi

# Compilation
echo "🔨 Compilation du nœud..."
cd /root/zeta4/zetanetwork-node
source "$HOME/.cargo/env" 2>/dev/null || true
cargo build --release

# Certificat TLS auto-signé
if [ ! -f /etc/ssl/zeta.crt ]; then
    echo "🔐 Génération du certificat TLS..."
    openssl req -x509 -nodes -days 3650 -newkey rsa:2048 \
        -keyout /etc/ssl/zeta.key -out /etc/ssl/zeta.crt \
        -subj "/CN=zetarelay" 2>/dev/null
fi

# Nginx — HTTPS + proxy WebSocket
cat > /etc/nginx/sites-available/zetanetwork << 'NGINX'
server {
    listen 443 ssl;
    server_name _;
    ssl_certificate /etc/ssl/zeta.crt;
    ssl_certificate_key /etc/ssl/zeta.key;

    location /ws {
        proxy_pass http://127.0.0.1:9091;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 86400;
        proxy_send_timeout 86400;
    }

    location / {
        default_type text/html;
        return 200 '<!DOCTYPE html><html><body style="font-family:system-ui;text-align:center;padding:4rem;background:#0a0a0a;color:#ddd"><h1 style="color:#00d4ff">✅ Relais Zeta Network</h1><p>Certificat accepté. Retournez sur <a href="https://www.zetanetwork.org" style="color:#00d4ff">zetanetwork.org</a></p></body></html>';
    }
}
NGINX

rm -f /etc/nginx/sites-enabled/default
ln -sf /etc/nginx/sites-available/zetanetwork /etc/nginx/sites-enabled/
nginx -t && systemctl restart nginx

# Firewall
ufw allow 443/tcp 2>/dev/null || true
ufw allow 9090/tcp 2>/dev/null || true
iptables -I INPUT -p tcp --dport 443 -j ACCEPT 2>/dev/null || true
iptables -I INPUT -p tcp --dport 9090 -j ACCEPT 2>/dev/null || true

# Service systemd
cat > /etc/systemd/system/zetanode.service << 'SVC'
[Unit]
Description=Zeta Network Relay
After=network.target

[Service]
Type=simple
ExecStart=/root/zeta4/zetanetwork-node/target/release/zetanetwork-node
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
SVC

systemctl daemon-reload
systemctl enable zetanode
systemctl restart zetanode

MY_IP=$(curl -s ifconfig.me)
echo ""
echo "══════════════════════════════════════"
echo "  ✅ Relais Zeta Network actif !"
echo "  IP:   $MY_IP"
echo "  WSS:  wss://$MY_IP/ws"
echo "  P2P:  $MY_IP:9090"
echo "══════════════════════════════════════"
echo "  Ajoutez '$MY_IP' dans RELAYS de index.html"
echo ""
