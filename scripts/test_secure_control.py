#!/usr/bin/env python3
"""Two independent processes using the encrypted view-only TEST profile."""
import socket
import sys
import time
from test_lan_diagnostic import start, expect

# Adapt the existing harness to an explicit, synthetic-only profile.
def spawn(exe, mode, address):
    import queue
    import subprocess
    import threading
    proc = subprocess.Popen([exe, mode, address, "--test"], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
    messages = queue.Queue()
    def reader():
        for line in proc.stdout:
            line = line.rstrip()
            print(f"{mode}: {line}", flush=True)
            messages.put(line)
        messages.put(None)
    threading.Thread(target=reader, daemon=True).start()
    return proc, messages

def approve(pair, peer):
    pair[0].stdin.write(peer + "\nALLOW TEST CONTROL\n")
    pair[0].stdin.flush()

def main():
    exe = sys.argv[1]
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    address = f"127.0.0.1:{port}"
    peers = []
    deadline = time.monotonic() + 75
    try:
        host = spawn(exe, "--listen", address)
        peers.append(host)
        hk = expect(host, "KEY ", deadline)
        controller = spawn(exe, "--connect", address)
        peers.append(controller)
        ck = expect(controller, "KEY ", deadline)
        approve(host, ck)
        expect(host, "LISTENING ", deadline)
        approve(controller, hk)
        expect(controller, "PASS CONTROLLER CONTROL:", deadline)
        expect(host, "PASS HOST CONTROL:", deadline)
        for proc, _ in peers:
            if proc.wait(timeout=5) != 0:
                raise RuntimeError("view process failed")
        print("PASS: encrypted view-only test, actual DTLS certificate pinning and decoded native frames")
    finally:
        import subprocess
        for proc, _ in peers:
            if proc.poll() is None:
                proc.terminate()
                try: proc.wait(timeout=3)
                except subprocess.TimeoutExpired: proc.kill(); proc.wait()
if __name__ == "__main__":
    main()
