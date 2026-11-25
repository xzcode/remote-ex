#!/usr/bin/env python3
"""Synthetic-only acceptance in isolated Linux namespaces on a disposable CI VM.

Two independently NATed peers share NO direct path. Each gateway forwards
only WSS and TURN traffic to a lab-only service namespace. Nothing is attached
to the runner's real interfaces, routes or firewall. Both ICE All and Relay
must deliver the same decoded-video and inert input-release evidence.
"""
import hashlib
import os
from pathlib import Path
import secrets
import signal
import subprocess
import sys
import tempfile
import time

from test_routed_diagnostic import start, approve
from test_lan_diagnostic import expect

SERVICE_IP = "198.18.0.1"
WSS_PORT = 47822
TURN_PORT = 3478

def run(*args):
    subprocess.run(args, check=True)

def nsrun(ns, *args):
    run("ip", "netns", "exec", ns, *args)

def ip(ns, *args):
    run("ip", "-n", ns, *args)

def create_lab(names):
    lab, gw1, gw2, peer1, peer2 = names
    for ns in names:
        run("ip", "netns", "add", ns)
        ip(ns, "link", "set", "lo", "up")
    ip(lab, "link", "add", "labbr", "type", "bridge")
    ip(lab, "addr", "add", f"{SERVICE_IP}/24", "dev", "labbr")
    ip(lab, "link", "set", "labbr", "up")
    for n, (gw, peer) in enumerate(((gw1, peer1), (gw2, peer2)), 1):
        # Each link is created inside a dedicated namespace.
        ip(lab, "link", "add", f"edge{n}", "type", "veth", "peer", "name", "outer")
        ip(lab, "link", "set", "outer", "netns", gw)
        ip(lab, "link", "set", f"edge{n}", "master", "labbr")
        ip(lab, "link", "set", f"edge{n}", "up")
        ip(gw, "addr", "add", f"198.18.0.{10+n}/24", "dev", "outer")
        ip(gw, "link", "set", "outer", "up")
        ip(gw, "link", "add", "inner", "type", "veth", "peer", "name", "ethlab")
        ip(gw, "link", "set", "ethlab", "netns", peer)
        ip(gw, "addr", "add", f"10.201.{n}.1/24", "dev", "inner")
        ip(gw, "link", "set", "inner", "up")
        ip(peer, "addr", "add", f"10.201.{n}.2/24", "dev", "ethlab")
        ip(peer, "link", "set", "ethlab", "up")
        ip(peer, "route", "add", "default", "via", f"10.201.{n}.1")
        nsrun(gw, "sysctl", "-qw", "net.ipv4.ip_forward=1")
        # Namespace-scoped rules only. No direct peer-to-peer traffic and no
        # external Internet. TURN allocation traffic remains on labbr.
        nsrun(gw, "iptables", "-P", "FORWARD", "DROP")
        for proto, port in (("tcp", WSS_PORT), ("udp", TURN_PORT), ("tcp", TURN_PORT)):
            nsrun(gw, "iptables", "-A", "FORWARD", "-i", "inner", "-o", "outer",
                  "-s", f"10.201.{n}.0/24", "-d", SERVICE_IP, "-p", proto,
                  "--dport", str(port), "-j", "ACCEPT")
        nsrun(gw, "iptables", "-A", "FORWARD", "-i", "outer", "-o", "inner",
              "-m", "conntrack", "--ctstate", "ESTABLISHED,RELATED", "-j", "ACCEPT")
        nsrun(gw, "iptables", "-t", "nat", "-A", "POSTROUTING", "-o", "outer",
              "-s", f"10.201.{n}.0/24", "-j", "MASQUERADE")
        # The other external gateway and all direct media destinations are
        # unreachable through FORWARD; there are no shared client interfaces.
    print("LAB: two separate NAT gateways; only WSS and TURN listener reachable", flush=True)

def spawn(ns, argv, env=None):
    return start(["ip", "netns", "exec", ns, *argv], env)

def write_private(path, value):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as stream:
        stream.write(value)

def cleanup_processes(peers):
    for proc, _ in reversed(peers):
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=3)
        if proc.stdin and not proc.stdin.closed:
            proc.stdin.close()

def case(names, server_exe, peer_exe, relay_only):
    lab, _, _, host_ns, controller_ns = names
    peers = []
    deadline = time.monotonic() + 105
    with tempfile.TemporaryDirectory(prefix="remote-ex-nat-") as directory:
        path = Path(directory)
        cert, key = str(path / "cert.pem"), str(path / "key.pem")
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                        "-subj", f"/CN={SERVICE_IP}", "-addext", f"subjectAltName=IP:{SERVICE_IP}",
                        "-addext", "basicConstraints=critical,CA:FALSE",
                        "-addext", "extendedKeyUsage=serverAuth",
                        "-addext", "keyUsage=critical,digitalSignature,keyEncipherment",
                        "-keyout", key, "-out", cert], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        pin = secrets.token_hex(32)
        secret = secrets.token_hex(32)
        secret_path = str(path / "turn-secret")
        turn_config = str(path / "turn.conf")
        allow = str(path / "devices.txt")
        write_private(secret_path, secret + "\n")
        write_private(turn_config,
                      f"listening-ip={SERVICE_IP}\nrelay-ip={SERVICE_IP}\nlistening-port={TURN_PORT}\n"
                      f"realm=remote-ex-isolated-ci\nuse-auth-secret\nstatic-auth-secret={secret}\n"
                      "fingerprint\nno-cli\nno-tls\nno-dtls\nno-multicast-peers\n"
                      "no-tcp-relay\nmin-port=40000\nmax-port=40063\ntotal-quota=8\nuser-quota=2\n"
                      f"pidfile={directory}/turn.pid\nuserdb={directory}/turn.db\n"
                      f"allowed-peer-ip={SERVICE_IP}\ndenied-peer-ip=0.0.0.0-255.255.255.255\n")
        url = f"wss://{SERVICE_IP}:{WSS_PORT}/v1/ws"
        flags = ["--ca", cert, "--desktop-session", "--defer-test-registration"]
        if relay_only:
            flags.append("--relay-only")
        try:
            host = spawn(host_ns, [peer_exe, "--host", url, pin, *flags])
            peers.append(host)
            hk = expect(host, "KEY ", deadline)
            controller = spawn(controller_ns, [peer_exe, "--controller", url, pin, *flags])
            peers.append(controller)
            ck = expect(controller, "KEY ", deadline)
            # Exercise actual operator allowlist, not a bypass or test server.
            ids = [hashlib.sha256(bytes.fromhex(k)).hexdigest() for k in (hk, ck)]
            write_private(allow, "\n".join(ids) + "\n")
            turn = subprocess.Popen(["ip", "netns", "exec", lab, "turnserver", "-c", turn_config],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            peers.append((turn, None))
            time.sleep(1)
            if turn.poll() is not None:
                raise RuntimeError("isolated TURN failed to start")
            env = dict(os.environ, REMOTE_EX_SIGNAL_BIND=f"{SERVICE_IP}:{WSS_PORT}",
                       REMOTE_EX_PUBLIC_BIND="1", REMOTE_EX_TLS_CERT=cert,
                       REMOTE_EX_TLS_KEY=key, REMOTE_EX_SERVER_ID=pin,
                       REMOTE_EX_ALLOWED_DEVICES_FILE=allow,
                       REMOTE_EX_STUN_URL=f"stun://{SERVICE_IP}:{TURN_PORT}",
                       REMOTE_EX_TURN_URL=f"turn://{SERVICE_IP}:{TURN_PORT}",
                       REMOTE_EX_TURN_SECRET_FILE=secret_path)
            server = spawn(lab, [server_exe], env)
            peers.append(server)
            expect(server, "Remote EX enrolled WSS signaling: ", deadline)
            for pair in (host, controller):
                pair[0].stdin.write("REGISTER TEST\n")
                pair[0].stdin.flush()
            if expect(host, "ONLINE ", deadline) != ids[0] or expect(controller, "ONLINE ", deadline) != ids[1]:
                raise RuntimeError("enrolled device ID mismatch")
            approve(host, ck)
            approve(controller, hk)
            host_result = expect(host, "PASS ROUTED HOST:", deadline)
            controller_result = expect(controller, "PASS ROUTED CONTROLLER:", deadline)
            if "input_released=true" not in host_result or "release_ack=true" not in controller_result:
                raise RuntimeError("missing release evidence")
            for proc, _ in (host, controller):
                if proc.wait(timeout=5) != 0:
                    raise RuntimeError("routed peer failed")
            print("PASS isolated two-NAT " + ("forced TURN" if relay_only else "ICE All automatic TURN fallback") +
                  ": enrolled TLS server, no direct path, decoded video and inert release", flush=True)
        finally:
            cleanup_processes(peers)

def main():
    if sys.platform != "linux" or os.geteuid() != 0 or os.environ.get("REMOTE_EX_ISOLATED_CI") != "1":
        raise SystemExit("Disposable Linux CI only; requires explicit isolated test opt-in")
    server, peer = (str(Path(p).resolve(strict=True)) for p in sys.argv[1:3])
    prefix = "rxe-" + str(os.getpid()) + "-"
    names = [prefix+s for s in ("lab", "gw1", "gw2", "peer1", "peer2")]
    if any(Path("/run/netns", ns).exists() for ns in names):
        raise SystemExit("Namespace collision")
    try:
        create_lab(names)
        case(names, server, peer, False)
        case(names, server, peer, True)
    finally:
        for ns in reversed(names):
            if Path("/run/netns", ns).exists():
                # Only these uniquely created isolated test namespaces.
                subprocess.run(["ip", "netns", "del", ns], check=False)

def on_term(*_):
    raise SystemExit("Test interrupted")

if __name__ == "__main__":
    signal.signal(signal.SIGTERM, on_term)
    main()
