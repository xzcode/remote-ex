#!/usr/bin/env python3
"""Three processes: real loopback WSS rendezvous and two synthetic peers."""
import os
import json
import secrets
import socket
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
    relay_only="--relay-only" in sys.argv[3:]
    exchange_sender=next((arg.split("=",1)[1] for arg in sys.argv[3:] if arg.startswith("--file-exchange=")),None)
    if exchange_sender not in (None,"host","controller"):raise RuntimeError("Invalid file sender role")
    pause_test="--file-pause" in sys.argv[3:]
    if pause_test and not exchange_sender:raise RuntimeError("Pause test needs file transfer")
    audio_test="--audio-test" in sys.argv[3:]
    if audio_test and exchange_sender:raise RuntimeError("Choose one test profile")
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
        deadline=time.monotonic()+100
        try:
            if relay_only:
                # CI-only private interface, not an open or Internet relay.
                interfaces=json.loads(subprocess.check_output(["ip","-j","-4","addr","show","scope","global"],text=True))
                ips=[info["local"] for interface in interfaces for info in interface.get("addr_info",[]) if info.get("family")=="inet"]
                if not ips:raise RuntimeError("No non-loopback CI IPv4 interface")
                ip=ips[0]
                with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as sock:
                    sock.bind((ip,0));turn_port=sock.getsockname()[1]
                secret=secrets.token_hex(32)
                secret_path=os.path.join(directory,"turn-secret")
                config_path=os.path.join(directory,"turn.conf")
                for path,data in [(secret_path,secret+"\n"),(config_path,
                    f"listening-ip={ip}\nrelay-ip={ip}\nlistening-port={turn_port}\n"
                    f"realm=remote-ex-ci\nuse-auth-secret\nstatic-auth-secret={secret}\n"
                    "fingerprint\nno-cli\nno-tls\nno-dtls\nno-multicast-peers\n"
                    "no-tcp-relay\nmin-port=40000\nmax-port=40063\ntotal-quota=8\nuser-quota=2\n"
                    f"pidfile={directory}/turn.pid\nuserdb={directory}/turn.db\n"
                    f"allowed-peer-ip={ip}\ndenied-peer-ip=0.0.0.0-255.255.255.255\n")]:
                    fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
                    with os.fdopen(fd,"w") as stream:stream.write(data)
                # Do not print coturn config or its potentially sensitive logs.
                p=subprocess.Popen(["turnserver","-c",config_path],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                peers.append((p,None))
                time.sleep(1)
                if p.poll() is not None:raise RuntimeError("Local TURN test service failed to start")
                env.update(REMOTE_EX_STUN_URL=f"stun://{ip}:{turn_port}",REMOTE_EX_TURN_URL=f"turn://{ip}:{turn_port}",REMOTE_EX_TURN_SECRET_FILE=secret_path)
            server=start([server_exe],env)
            peers.append(server)
            addr=expect(server,"Remote EX loopback WSS development signaling: ",deadline)
            port=addr.rsplit(":",1)[1]
            url=f"wss://localhost:{port}/v1/ws"
            flags=["--ca",cert]+(["--relay-only"] if relay_only else [])
            source=os.path.join(directory,"source.bin");target=os.path.join(directory,"received.bin")
            payload=bytes(range(256))*257
            host_flags=list(flags);controller_flags=list(flags)
            if audio_test:
                host_flags.extend(["--audio-test","--desktop-session"]);controller_flags.extend(["--audio-test","--desktop-session"])
            if exchange_sender:
                with open(source,"wb") as stream:stream.write(payload)
                for role,args in (("host",host_flags),("controller",controller_flags)):
                    args.extend(["--desktop-session","--exchange","file",exchange_sender,source if role==exchange_sender else target])
            host=start([peer_exe,"--host",url,pin,*host_flags])
            peers.append(host)
            hk=expect(host,"KEY ",deadline)
            expect(host,"ONLINE ",deadline)
            controller=start([peer_exe,"--controller",url,pin,*controller_flags])
            peers.append(controller)
            ck=expect(controller,"KEY ",deadline)
            expect(controller,"ONLINE ",deadline)
            if exchange_sender:
                for role,pair,key in (("host",host,ck),("controller",controller,hk)):
                    consent="ALLOW SEND FILE" if role==exchange_sender else "ALLOW RECEIVE FILE"
                    pair[0].stdin.write(key+"\n"+consent+"\n");pair[0].stdin.flush()
            elif audio_test:
                for pair,key in ((host,ck),(controller,hk)):
                    pair[0].stdin.write(key+"\nALLOW TEST AUDIO\n");pair[0].stdin.flush()
            else:
                approve(host,ck)
                approve(controller,hk)
            if pause_test:
                pair=host if exchange_sender=="host" else controller
                pair[0].stdin.write("PAUSE\n");pair[0].stdin.flush()
                expect(pair,"TRANSFER_PAUSED",deadline)
                time.sleep(2.2)
                pair[0].stdin.write("RESUME\n");pair[0].stdin.flush()
                expect(pair,"TRANSFER_RESUMED",deadline)
            expect(host,"PASS ROUTED HOST:",deadline)
            expect(controller,"PASS ROUTED CONTROLLER:",deadline)
            for proc,_ in peers[-2:]:
                if proc.wait(timeout=5)!=0:raise RuntimeError("peer exited unsuccessfully")
            if audio_test:
                print("PASS: synthetic audio transport and verified sample count",flush=True)
                return
            if exchange_sender:
                with open(target,"rb") as stream:
                    if stream.read()!=payload:raise RuntimeError("Transferred payload mismatch")
                print("PASS: authenticated file exchange "+exchange_sender+"; bytes verified",flush=True)
                return
            print("PASS: "+("forced TURN relay; " if relay_only else "direct ICE; ")+"WSS device-ID routing, encrypted signed SDP, DTLS/SCTP, decoded video and inert input release",flush=True)
        finally:
            for proc,_ in reversed(peers):
                if proc.poll() is None:
                    proc.terminate()
                    try:proc.wait(timeout=3)
                    except subprocess.TimeoutExpired:proc.kill();proc.wait()
                if proc.stdin and not proc.stdin.closed:proc.stdin.close()
if __name__=="__main__":
    main()
