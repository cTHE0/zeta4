#!/usr/bin/env python3
"""Deploy Zeta Network to VPS via SSH"""
import paramiko
import sys
import time

HOST = '65.75.201.11'
USER = 'root'
PASS = '&m&5wE57uvF6'

def ssh_exec(client, cmd, timeout=300):
    """Execute command and print output in real-time"""
    print(f"\n>>> {cmd}")
    stdin, stdout, stderr = client.exec_command(cmd, timeout=timeout)
    out = stdout.read().decode()
    err = stderr.read().decode()
    exit_code = stdout.channel.recv_exit_status()
    if out.strip():
        print(out.strip()[-2000:])  # last 2000 chars
    if err.strip():
        print(f"STDERR: {err.strip()[-1000:]}")
    print(f"[exit: {exit_code}]")
    return out, err, exit_code

def main():
    print(f"Connecting to {HOST}...")
    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    client.connect(HOST, username=USER, password=PASS, timeout=15)
    print("Connected!")

    # 1. Check system
    ssh_exec(client, "uname -a")
    ssh_exec(client, "free -h | head -3")
    ssh_exec(client, "df -h / | tail -1")

    # 2. Install dependencies
    ssh_exec(client, "apt update -qq && apt install -y -qq build-essential pkg-config libssl-dev git curl", timeout=120)

    # 3. Install Rust if not present
    _, _, rc = ssh_exec(client, "which cargo")
    if rc != 0:
        print("\nInstalling Rust...")
        ssh_exec(client, "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y", timeout=300)
        ssh_exec(client, "source $HOME/.cargo/env && rustc --version")

    # 4. Clone or update repo
    _, _, rc = ssh_exec(client, "test -d /root/zeta4 && echo exists")
    if rc == 0:
        ssh_exec(client, "cd /root/zeta4 && git pull")
    else:
        ssh_exec(client, "cd /root && git clone https://github.com/cTHE0/zeta4.git")

    # 5. Build the Rust node
    print("\nBuilding Rust node (this may take a few minutes)...")
    out, err, rc = ssh_exec(client, 
        "source $HOME/.cargo/env && cd /root/zeta4/zetanetwork-node && cargo build --release 2>&1 | tail -20",
        timeout=600)
    
    if rc != 0:
        print(f"\n!!! BUILD FAILED !!!\n{err[-2000:]}")
        # Get full error
        ssh_exec(client, "source $HOME/.cargo/env && cd /root/zeta4/zetanetwork-node && cargo build --release 2>&1 | tail -50", timeout=600)
        client.close()
        return

    # 6. Stop any existing instance
    ssh_exec(client, "pkill -f zetanetwork-node || true")
    time.sleep(1)

    # 7. Open firewall ports
    ssh_exec(client, "ufw allow 9090/tcp 2>/dev/null; ufw allow 9091/tcp 2>/dev/null; iptables -I INPUT -p tcp --dport 9090 -j ACCEPT 2>/dev/null; iptables -I INPUT -p tcp --dport 9091 -j ACCEPT 2>/dev/null; echo 'Ports opened'")

    # 8. Create systemd service
    service = """[Unit]
Description=Zeta Network Node
After=network.target

[Service]
Type=simple
ExecStart=/root/zeta4/zetanetwork-node/target/release/zetanetwork-node
Restart=always
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
"""
    ssh_exec(client, f"cat > /etc/systemd/system/zetanode.service << 'SERVICEEOF'\n{service}SERVICEEOF")
    ssh_exec(client, "systemctl daemon-reload && systemctl enable zetanode && systemctl restart zetanode")
    time.sleep(2)
    ssh_exec(client, "systemctl status zetanode --no-pager -l | head -20")
    ssh_exec(client, "journalctl -u zetanode --no-pager -n 15")

    # 9. Deploy web frontend (update index.html if Flask is running)
    ssh_exec(client, "cp /root/zeta4/zetanetwork-web/index.html /root/zeta4/zetanetwork-web/index.html.bak 2>/dev/null; echo 'Web files ready'")
    
    # Check if flask/web server is running  
    ssh_exec(client, "ps aux | grep -E 'flask|python.*app|nginx|apache' | grep -v grep || echo 'No web server detected'")

    print("\n=== DEPLOYMENT COMPLETE ===")
    print(f"Node: {HOST}:9090 (P2P) / {HOST}:9091 (WebSocket)")
    print("Web: zetanetwork.org")

    client.close()

if __name__ == '__main__':
    main()
