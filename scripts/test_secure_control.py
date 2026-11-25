#!/usr/bin/env python3
"""Inert-only integration: normal STOP and local supervisor revocation on either peer."""
import queue
import socket
import subprocess
import sys
import threading
import time
from test_lan_diagnostic import expect

def spawn(exe, mode, address):
    proc = subprocess.Popen(
        [exe, mode, address, "--test", "--desktop-session"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, text=True, bufsize=1,
    )
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

def run_case(exe, revoke):
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    address = f"127.0.0.1:{port}"
    peers = []
    deadline = time.monotonic() + 25
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
        # Wait until the inert backend actually holds a test key. Closing a
        # pipe earlier would not exercise the release path.
        expect(host, "TEST_INPUT_HELD", deadline)
        if revoke == "host":
            host[0].stdin.close()
        elif revoke == "controller":
            controller[0].stdin.close()
        expect(host, "INPUT_RELEASED", deadline)
        expect(controller, "INPUT_RELEASED", deadline)
        expect(host, "PASS HOST CONTROL:", deadline)
        result = expect(controller, "PASS CONTROLLER CONTROL:", deadline)
        if revoke == "normal" and int(result.split("decoded_frames=")[1].split(",")[0]) < 30:
            raise RuntimeError("normal control must decode at least 30 frames")
        for proc, _ in peers:
            if proc.wait(timeout=5) != 0:
                raise RuntimeError("control process failed")
        print(f"PASS: {revoke}; both release markers and inert backend release verified")
    finally:
        for proc, _ in peers:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
            if proc.stdin and not proc.stdin.closed:
                proc.stdin.close()

def main():
    for case in ("normal", "host", "controller"):
        run_case(sys.argv[1], case)

if __name__ == "__main__":
    main()
