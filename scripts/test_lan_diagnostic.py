#!/usr/bin/env python3
"""Run two independent synthetic-only diagnostic processes on loopback."""
import os
import queue
import socket
import subprocess
import sys
import threading
import time

def start(exe, mode, address):
    proc = subprocess.Popen([exe, mode, address], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, bufsize=1)
    messages = queue.Queue()
    def reader():
        for line in proc.stdout:
            line = line.rstrip()
            print(f"{mode}: {line}", flush=True)
            messages.put(line)
        messages.put(None)
    threading.Thread(target=reader, daemon=True).start()
    return proc, messages

def expect(pair, prefix, deadline):
    proc, messages = pair
    while time.monotonic() < deadline:
        try:
            line = messages.get(timeout=max(.01, deadline - time.monotonic()))
        except queue.Empty:
            break
        if line is None:
            raise RuntimeError(f"process ended before {prefix}: {proc.poll()}")
        if line.startswith(prefix):
            return line[len(prefix):]
    raise RuntimeError(f"timed out: {prefix}")

def approve(pair, peer):
    proc = pair[0]
    proc.stdin.write(peer + "\nALLOW DIAGNOSTIC\n")
    proc.stdin.flush()

def main():
    exe = sys.argv[1]
    if not os.path.isfile(exe):
        raise RuntimeError("build the diagnostic executable first")
    # Pick a loopback-only address; no public listener. The brief port
    # reservation race fails the test rather than broadening the bind.
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    address = f"127.0.0.1:{port}"
    peers = []
    deadline = time.monotonic() + 65
    try:
        host = start(exe, "--listen", address)
        peers.append(host)
        host_key = expect(host, "KEY ", deadline)
        controller = start(exe, "--connect", address)
        peers.append(controller)
        controller_key = expect(controller, "KEY ", deadline)
        approve(host, controller_key)
        expect(host, "LISTENING ", deadline)
        approve(controller, host_key)
        expect(controller, "PASS CONTROLLER:", deadline)
        expect(host, "PASS HOST:", deadline)
        for proc, _ in peers:
            if proc.wait(timeout=5) != 0:
                raise RuntimeError("peer exited unsuccessfully")
        print("PASS: two independent processes, signed session, video and reliable test data")
    finally:
        for proc, _ in peers:
            if proc.poll() is None:
                proc.terminate()
                try: proc.wait(timeout=3)
                except subprocess.TimeoutExpired: proc.kill(); proc.wait()
if __name__ == "__main__":
    main()
