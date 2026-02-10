#!/usr/bin/env python3
"""Rebuild and restart the Zeta Network node on VPS"""
import paramiko
import time

HOST = '65.75.201.11'
USER = 'root'
PASS = '&m&5wE57uvF6'

def ssh_exec(client, cmd, timeout=600):
    print(f"\n>>> {cmd}")
    stdin, stdout, stderr = client.exec_command(cmd, timeout=timeout)
    out = stdout.read().decode()
    err = stderr.read().decode()
    exit_code = stdout.channel.recv_exit_status()
    if out.strip():
        print(out.strip()[-3000:])
    if err.strip():
        print(f"STDERR: {err.strip()[-2000:]}")
    print(f"[exit: {exit_code}]")
    return out, err, exit_code

client = paramiko.SSHClient()
client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
client.connect(HOST, username=USER, password=PASS, timeout=15)
print("Connected!")

# Check Rust version
ssh_exec(client, "rustc --version && cargo --version")

# Pull latest
ssh_exec(client, "cd /root/zeta4 && git pull")

# Build
print("\nBuilding (may take a few minutes)...")
out, err, rc = ssh_exec(client, 
    "cd /root/zeta4/zetanetwork-node && cargo build --release 2>&1",
    timeout=600)

if 'error' in out.lower() or rc != 0:
    print("\n!!! BUILD FAILED - showing last 80 lines !!!")
    ssh_exec(client, "cd /root/zeta4/zetanetwork-node && cargo build --release 2>&1 | tail -80", timeout=600)
else:
    print("\n✅ BUILD SUCCESS!")
    # Restart service
    ssh_exec(client, "systemctl restart zetanode")
    time.sleep(3)
    ssh_exec(client, "systemctl status zetanode --no-pager -l | head -15")
    ssh_exec(client, "journalctl -u zetanode --no-pager -n 20")
    
    # Test WebSocket port
    ssh_exec(client, "ss -tlnp | grep -E '9090|9091'")

client.close()
