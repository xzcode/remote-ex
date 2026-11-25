#!/usr/bin/env python3
"""Three processes: real loopback WSS rendezvous and two synthetic peers."""
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
from test_lan_diagnostic import expect

def start(argv, env=None):
    p=subprocess.Popen(argv,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,
                       stderr=subprocess.STDOUT,text=True,bufsize=1)
    q=queue.Queue()
    def reader():
        for line in p.stdout:
            line=line.rstrip()
            print(f"{argv[0].split('/')[-1]}: {line}",flush=True)
            q.put(line)
        q.put(None)
    threading.Thread(target=reader,daemon=True).start()
    return p,q

def approve(pair,key):
    pair[0].stdin.write(key+"\nALLOW ROUTED TEST\n")
    pair[0].stdin.flush()

def main():
    server_exe, peer_exe=sys.argv[1:3]
    peers=[]
    with tempfile.TemporaryDirectory() as directory:
        cert=os.path.join(directory,"cert.pem")
        key=os.path.join(directory,"key.pem")
        subprocess.run(["openssl","req","-x509","-newkey","rsa:2048","-nodes","-days","1",
            "-subj","/CN=localhost","-addext","subjectAltName=DNS:localhost",
            "-addext","basicConstraints=critical,CA:FALSE","-addext","extendedKeyUsage=serverAuth",
            "-addext","keyUsage=critical,digitalSignature,keyEncipherment",
            "-keyout",key,"-out",cert],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        pin="07"*32
        env=dict(os.environ,REMOTE_EX_SIGNAL_BIND="127.0.0.1:0",
                 REMOTE_EX_TLS_CERT=cert,REMOTE_EX_TLS_KEY=key,REMOTE_EX_SERVER_ID=pin)
        deadline=time.monotonic()+75
        try:
            server=start([server_exe],env)
            peers.append(server)
            addr=expect(server,"Remote EX loopback WSS development signaling: ",deadline)
            port=addr.rsplit(":",1)[1]
            url=f"wss://localhost:{port}/v1/ws"
            host=start([peer_exe,"--host",url,pin,"--ca",cert])
            peers.append(host)
            hk=expect(host,"KEY ",deadline)
            expect(host,"ONLINE ",deadline)
            controller=start([peer_exe,"--controller",url,pin,"--ca",cert])
            peers.append(controller)
            ck=expect(controller,"KEY ",deadline)
            expect(controller,"ONLINE ",deadline)
            approve(host,ck)
            approve(controller,hk)
            expect(host,"PASS ROUTED HOST:",deadline)
            expect(controller,"PASS ROUTED CONTROLLER:",deadline)
            for proc,_ in peers[1:]:
                if proc.wait(timeout=5)!=0:raise RuntimeError("peer exited unsuccessfully")
            print("PASS: WSS device-ID routing, signed SDP, DTLS/SCTP, decoded video and inert input release",flush=True)
        finally:
            for proc,_ in reversed(peers):
                if proc.poll() is None:
                    proc.terminate()
                    try:proc.wait(timeout=3)
                    except subprocess.TimeoutExpired:proc.kill();proc.wait()
                if proc.stdin and not proc.stdin.closed:proc.stdin.close()
if __name__=="__main__":
    main()
