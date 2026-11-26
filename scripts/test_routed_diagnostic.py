#!/usr/bin/env python3
"""Three processes: real loopback WSS rendezvous and two synthetic peers."""
import os
import json
import hashlib
import struct
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
    manage_directory_test="--manage-directory-test" in sys.argv[3:]
    manage_test="--manage-test" in sys.argv[3:] or manage_directory_test
    batch_test="--catalog-batch-test" in sys.argv[3:]
    catalog_test="--catalog-test" in sys.argv[3:] or batch_test
    if catalog_test and exchange_sender:raise RuntimeError("Choose a separate catalog test")
    directory_test="--directory-exchange" in sys.argv[3:]
    resume_test="--file-resume" in sys.argv[3:]
    checkpoint_test="--file-checkpoint" in sys.argv[3:]
    if checkpoint_test and (not exchange_sender or resume_test or directory_test or pause_test):raise RuntimeError("Choose a separate interrupted-file test")
    if resume_test and not exchange_sender:raise RuntimeError("Resume test requires file mode")
    if directory_test and (not exchange_sender or resume_test or pause_test):raise RuntimeError("Choose a separate directory transfer test")
    clipboard_test="--clipboard-sync-test" in sys.argv[3:]
    if clipboard_test and exchange_sender:raise RuntimeError("Choose one test profile")
    extended_test="--extended-test" in sys.argv[3:]
    armed_test="--armed-test" in sys.argv[3:]
    armed_cancel="--armed-cancel" in sys.argv[3:]
    armed_wrong="--armed-wrong-key" in sys.argv[3:]
    armed_test=armed_test or armed_cancel or armed_wrong
    x11_test="--x11-view-test" in sys.argv[3:]
    power_test="--power-test" in sys.argv[3:]
    power_wrong="--power-wrong-token" in sys.argv[3:]
    power_test=power_test or power_wrong
    av_test="--av-test" in sys.argv[3:]
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
        deadline=time.monotonic()+(160 if extended_test or armed_test or x11_test else 100)
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
            payload=bytes(range(256))*(32768 if checkpoint_test else 257)
            host_flags=list(flags);controller_flags=list(flags)
            if "--shared-controller" in sys.argv[3:]:controller_flags.append("--shared-controller")
            if manage_test:
                os.mkdir(source)
                with open(os.path.join(source,"a.txt"),"wb") as stream:stream.write(b"alpha")
                with open(os.path.join(source,"b.txt"),"wb") as stream:stream.write(b"beta")
                if manage_directory_test:
                    old_recovery="Remote-EX-Recovery-"+"a"*32
                    os.makedirs(os.path.join(source,old_recovery,"0-saved","sub"))
                    with open(os.path.join(source,old_recovery,"0-saved","sub","keep.txt"),"wb") as stream:stream.write(b"saved")
                    os.makedirs(os.path.join(source,"folder","sub"))
                    with open(os.path.join(source,"folder","sub","keep.txt"),"wb") as stream:stream.write(b"folder")
                    os.makedirs(os.path.join(source,"remove","sub"))
                    with open(os.path.join(source,"remove","sub","keep.txt"),"wb") as stream:stream.write(b"remove")
                host_flags.extend(["--desktop-session","--manage",source]);controller_flags.extend(["--desktop-session","--manage"])
            if catalog_test:
                os.mkdir(source);os.mkdir(os.path.join(source,"nested"))
                with open(os.path.join(source,"nested","sample.bin"),"wb") as stream:stream.write(payload)
                if batch_test:
                    with open(os.path.join(source,"nested","skip.bin"),"wb") as stream:stream.write(b"excluded")
                    with open(os.path.join(source,"z.txt"),"wb") as stream:stream.write(b"second file")
                flag="--catalog-batch" if batch_test else "--catalog"
                host_flags.extend(["--desktop-session",flag,source]);controller_flags.extend(["--desktop-session",flag,target])
            if clipboard_test:
                host_flags.extend(["--clipboard-sync-test","--desktop-session"]);controller_flags.extend(["--clipboard-sync-test","--desktop-session"])
            if power_test:
                host_flags.extend(["--power-test","--desktop-session"]);controller_flags.extend(["--power-test","restart","--desktop-session"])
            if armed_test:
                host_flags.extend(["--armed-test","--desktop-session"])
            if x11_test:
                probe=subprocess.check_output([peer_exe,"--probe-x11"],text=True).strip().split()
                if probe[:3]!=["PASS","X11","CAPTURE"]:raise RuntimeError("Local X11 capture probe failed")
                host_flags.extend(["--view",probe[3],"--desktop-session"]);controller_flags.extend(["--view","--desktop-session"])
            if extended_test:
                host_flags.extend(["--extended-test","--desktop-session"]);controller_flags.extend(["--extended-test","--desktop-session"])
            if av_test:
                host_flags.extend(["--av-test","--desktop-session"]);controller_flags.extend(["--av-test","--desktop-session"])
            if audio_test:
                host_flags.extend(["--audio-test","--desktop-session"]);controller_flags.extend(["--audio-test","--desktop-session"])
            if exchange_sender:
                if directory_test:
                    os.mkdir(source);os.mkdir(os.path.join(source,"sub"));os.mkdir(os.path.join(source,"empty"))
                    with open(os.path.join(source,"sub","payload.bin"),"wb") as stream:stream.write(payload)
                else:
                    with open(source,"wb") as stream:stream.write(payload)
                for role,args in (("host",host_flags),("controller",controller_flags)):
                    args.extend(["--desktop-session","--exchange","directory" if directory_test else "file",exchange_sender,source if role==exchange_sender else target])
            partial=os.path.join(directory,"resume.rexpart")
            if checkpoint_test:
                (controller_flags if exchange_sender=="host" else host_flags).extend(["--save-checkpoint",partial])
            if resume_test:
                # The receiver validates existence before printing its public key.
                with open(partial,"xb"):pass
                (controller_flags if exchange_sender=="host" else host_flags).extend(["--resume-from",partial])
            host=start([peer_exe,"--host",url,pin,*host_flags])
            peers.append(host)
            hk=expect(host,"KEY ",deadline)
            expect(host,"ONLINE ",deadline)
            controller=start([peer_exe,"--controller",url,pin,*controller_flags])
            peers.append(controller)
            ck=expect(controller,"KEY ",deadline)
            expect(controller,"ONLINE ",deadline)
            if resume_test:
                prefix=payload[:12345]
                sender_key,receiver_key=(hk,ck) if exchange_sender=="host" else (ck,hk)
                header={"scope":{"sender":list(bytes.fromhex(sender_key)),"receiver":list(bytes.fromhex(receiver_key))},"offer":{"kind":"file","name":"source.bin","size":len(payload),"sha256":list(hashlib.sha256(payload).digest())},"offset":len(prefix),"prefix_hash":list(hashlib.sha256(prefix).digest())}
                encoded=json.dumps(header,separators=(",",":")).encode("utf-8")
                with open(partial,"wb") as stream:stream.write(b"REXPART1"+struct.pack(">I",len(encoded))+encoded+prefix)
            if exchange_sender:
                for role,pair,key in (("host",host,ck),("controller",controller,hk)):
                    consent=("ALLOW SEND " if role==exchange_sender else "ALLOW RECEIVE ")+("DIRECTORY" if directory_test else "FILE")
                    pair[0].stdin.write(key+"\n"+consent+"\n");pair[0].stdin.flush()
            elif power_test:
                for pair,key,consent in ((host,ck,"ALLOW REVIEW ONE POWER REQUEST"),(controller,hk,"ALLOW PROPOSE ONE POWER REQUEST")):
                    pair[0].stdin.write(key+"\n"+consent+"\n");pair[0].stdin.flush()
                token,action=expect(host,"POWER_REVIEW ",deadline).split(" ")
                if action!="restart" or len(token)!=64:raise RuntimeError("Wrong power review")
                if power_wrong:token=("0" if token[0]!="0" else "1")+token[1:]
                host[0].stdin.write("POWER_APPROVE "+token+"\n");host[0].stdin.flush()
                if power_wrong:
                    expect(host,"FAIL: Power approval does not match exact review",deadline)
                    if host[0].wait(timeout=8)==0:raise RuntimeError("Wrong review accepted")
                    print("PASS: wrong power review token rejected; no OS action",flush=True);return
                if expect(host,"POWER_TEST_AUTHORIZED ",deadline)!=token+" restart":raise RuntimeError("Wrong inert power receipt")
                if expect(controller,"POWER_ACCEPTED ",deadline)!="restart":raise RuntimeError("Wrong controller power receipt")
            elif manage_test:
                for pair,key,consent in ((host,ck,"ALLOW REVIEW FILE CHANGES"),(controller,hk,"ALLOW PROPOSE FILE CHANGES")):
                    pair[0].stdin.write(key+"\n"+consent+"\n");pair[0].stdin.flush()
                expect(controller,"CATALOG_READY ",deadline)
                plan={"changes":[{"op":"create_folder","parent":None,"name":"new-dir"},{"op":"create_file","parent":None,"name":"empty.txt"},{"op":"rename","index":0,"name":"renamed.txt"},{"op":"recycle","index":1}]}
                if manage_directory_test:
                    # Catalog traversal is lexical; derive indexes from its
                    # emitted records rather than relying on platform ordering.
                    # Use known fixture paths in sorted DFS (ASCII names).
                    paths=[]
                    def walk(p,parts=()):
                        for name in sorted(os.listdir(p)):
                            path=os.path.join(p,name); rel=parts+(name,)
                            paths.append(rel)
                            if os.path.isdir(path):walk(path,rel)
                    walk(source)
                    plan={"changes":[{"op":"rename","index":paths.index(("folder",)),"name":"renamed-folder"},{"op":"recycle","index":paths.index(("remove",))},{"op":"restore","index":paths.index((old_recovery,"0-saved")),"parent":None,"name":"restored"}]}
                controller[0].stdin.write("PLAN "+json.dumps(plan,separators=(",",":"))+"\n");controller[0].stdin.flush()
                review=expect(host,"MANAGE_REVIEW ",deadline)
                token,review_json=review.split(" ",1)
                reviewed=json.loads(review_json)
                if len(token)!=64 or len(reviewed["items"])!=len(plan["changes"]):raise RuntimeError("Incorrect review")
                expected_names=sorted(["a.txt","b.txt",old_recovery,"folder","remove"]) if manage_directory_test else ["a.txt","b.txt"]
                if sorted(os.listdir(source))!=expected_names:raise RuntimeError("Directory changed before host approval")
                host[0].stdin.write("APPROVE "+token+"\n");host[0].stdin.flush()
                if expect(controller,"MANAGE_APPLIED ",deadline)!=str(len(plan["changes"])):raise RuntimeError("Missing complete change receipt")
            elif catalog_test:
                for pair,key,consent in ((host,ck,"ALLOW LIST AND DOWNLOAD FOUR FILES" if batch_test else "ALLOW LIST AND DOWNLOAD SELECTED ROOT"),(controller,hk,"ALLOW BROWSE AND RECEIVE FILE BATCH" if batch_test else "ALLOW BROWSE AND RECEIVE ONE FILE")):
                    pair[0].stdin.write(key+"\n"+consent+"\n");pair[0].stdin.flush()
                expect(controller,"CATALOG_READY ",deadline)
                controller[0].stdin.write("BATCH 1,3\n" if batch_test else "SELECT 1\n");controller[0].stdin.flush()
            elif clipboard_test:
                for pair,key in ((host,ck),(controller,hk)):
                    pair[0].stdin.write(key+"\nALLOW TEST CLIPBOARD SYNC\n");pair[0].stdin.flush()
            elif armed_test:
                expected=secrets.token_hex(32) if armed_wrong else ck
                host[0].stdin.write(expected+"\nALLOW ROUTED TEST ARMED FOR UP TO 300 SECONDS\n");host[0].stdin.flush()
                expect(host,"LISTENING ARMED ",deadline)
                if armed_cancel:
                    host[0].stdin.close()
                    if host[0].wait(timeout=8)==0:raise RuntimeError("Canceled armed wait unexpectedly succeeded")
                    print("PASS: armed host revoked before incoming session",flush=True);return
                if armed_wrong:
                    approve(controller,hk)
                    expect(host,"FAIL: Unknown peer or unexpected test grant",deadline)
                    if host[0].wait(timeout=8)==0:raise RuntimeError("Wrong peer accepted")
                    print("PASS: armed host refused unapproved peer",flush=True);return
                time.sleep(24)
                approve(controller,hk)
            elif x11_test:
                for pair,key,consent in ((host,ck,"ALLOW ROUTED SCREEN"),(controller,hk,"ALLOW ROUTED VIEW")):
                    pair[0].stdin.write(key+"\n"+consent+"\n");pair[0].stdin.flush()
            elif extended_test:
                for pair,key in ((host,ck),(controller,hk)):
                    pair[0].stdin.write(key+"\nALLOW EXTENDED SYNTHETIC TEST\n");pair[0].stdin.flush()
            elif av_test:
                for pair,key in ((host,ck),(controller,hk)):
                    pair[0].stdin.write(key+"\nALLOW TEST AUDIO VIDEO\n");pair[0].stdin.flush()
            elif audio_test:
                for pair,key in ((host,ck),(controller,hk)):
                    pair[0].stdin.write(key+"\nALLOW TEST AUDIO\n");pair[0].stdin.flush()
            else:
                approve(host,ck)
                approve(controller,hk)
            if checkpoint_test:
                recipient=controller if exchange_sender=="host" else host
                expect(recipient,"TRANSFER_PROGRESS ",deadline)
                recipient[0].stdin.close() # explicit local revocation after actual bytes arrived
                saved=int(expect(recipient,"CHECKPOINT_SAVED ",deadline))
                if not 0<saved<len(payload):raise RuntimeError("No incomplete prefix saved")
                if recipient[0].wait(timeout=5)==0:raise RuntimeError("Interrupted transfer unexpectedly succeeded")
                if os.path.exists(target):raise RuntimeError("Incomplete content was published")
                with open(partial,"rb") as stream:
                    if stream.read(8)!=b"REXPART1":raise RuntimeError("Invalid checkpoint")
                    n=struct.unpack(">I",stream.read(4))[0]
                    if not 0<n<=4096:raise RuntimeError("Invalid checkpoint header size")
                    h=json.loads(stream.read(n));prefix=stream.read()
                sender_key,receiver_key=(hk,ck) if exchange_sender=="host" else (ck,hk)
                if h["scope"]!={"sender":list(bytes.fromhex(sender_key)),"receiver":list(bytes.fromhex(receiver_key))}:raise RuntimeError("Wrong checkpoint identity binding")
                if h["offer"]["sha256"]!=list(hashlib.sha256(payload).digest()) or h["offset"]!=saved or h["prefix_hash"]!=list(hashlib.sha256(prefix).digest()) or prefix!=payload[:saved] or len(prefix)!=saved:raise RuntimeError("Checkpoint bytes or digest mismatch")
                print(f"PASS: interrupted authorized transfer retained {saved} verified prefix bytes; destination absent",flush=True)
                return
            if resume_test:
                for pair in (host,controller):
                    marker=expect(pair,"TRANSFER_RESUME_OFFSET ",deadline)
                    if marker.split()[0]!="12345":raise RuntimeError("Wrong resume offset")
            if pause_test:
                pair=host if exchange_sender=="host" else controller
                pair[0].stdin.write("PAUSE\n");pair[0].stdin.flush()
                expect(pair,"TRANSFER_PAUSED",deadline)
                time.sleep(2.2)
                pair[0].stdin.write("RESUME\n");pair[0].stdin.flush()
                expect(pair,"TRANSFER_RESUMED",deadline)
            if extended_test:
                expect(controller,"ADAPTIVE_LOW ",deadline)
                expect(controller,"ADAPTIVE_FULL ",deadline)
            expect(host,"PASS ROUTED HOST:",deadline)
            result=expect(controller,"PASS ROUTED CONTROLLER:",deadline)
            if extended_test and "extended_seconds=" not in result:raise RuntimeError("Extended duration missing")
            if x11_test:
                import re
                match=re.search(r"decoded_frames=(\d+)",result)
                if not match or int(match.group(1))<5:raise RuntimeError("No real X11 frames decoded")
            for proc,_ in peers[-2:]:
                if proc.wait(timeout=5)!=0:raise RuntimeError("peer exited unsuccessfully")
            if power_test:
                print("PASS: signed power request with exact local review; inert path only",flush=True);return
            if armed_test:
                print("PASS: armed host admitted only its preapproved peer after old 20-second wait",flush=True);return
            if x11_test:
                print("PASS: authenticated read-only X11 capture and native viewer on disposable Xvfb",flush=True);return
            if extended_test:
                print("PASS: actual VP8 low/full resolution switching and >60-second authenticated session",flush=True)
                return
            if manage_directory_test:
                for path,expected in ((os.path.join("renamed-folder","sub","keep.txt"),b"folder"),(os.path.join(reviewed["recovery"],"1-remove","sub","keep.txt"),b"remove"),(os.path.join("restored","sub","keep.txt"),b"saved")):
                    with open(os.path.join(source,path),"rb") as stream:
                        if stream.read()!=expected:raise RuntimeError("Directory operation content mismatch")
                for path in ("folder","remove",os.path.join(old_recovery,"0-saved")):
                    if os.path.exists(os.path.join(source,path)):raise RuntimeError("Original directory still present")
                print("PASS: reviewed directory rename, recovery and restore, nested content preserved",flush=True)
                return
            if manage_test:
                if not os.path.isdir(os.path.join(source,"new-dir")) or os.path.getsize(os.path.join(source,"empty.txt"))!=0:raise RuntimeError("New entries missing")
                with open(os.path.join(source,"renamed.txt"),"rb") as stream:
                    if stream.read()!=b"alpha":raise RuntimeError("Renamed content mismatch")
                with open(os.path.join(source,reviewed["recovery"],"3-b.txt"),"rb") as stream:
                    if stream.read()!=b"beta":raise RuntimeError("Recovery content mismatch")
                if os.path.exists(os.path.join(source,"a.txt")) or os.path.exists(os.path.join(source,"b.txt")):raise RuntimeError("Original name remained")
                print("PASS: host-reviewed four file changes, no mutation before approval, renamed and recovered bytes verified",flush=True)
                return
            if batch_test:
                with open(os.path.join(target,"nested","sample.bin"),"rb") as stream:
                    if stream.read()!=payload:raise RuntimeError("Batch first file mismatch")
                with open(os.path.join(target,"z.txt"),"rb") as stream:
                    if stream.read()!=b"second file":raise RuntimeError("Batch second file mismatch")
                if os.path.exists(os.path.join(target,"nested","skip.bin")):raise RuntimeError("Unselected file downloaded")
                print("PASS: two selected files, preserved relative paths, unselected file excluded",flush=True)
                return
            if catalog_test:
                with open(target,"rb") as stream:
                    if stream.read()!=payload:raise RuntimeError("Catalog selected file mismatch")
                print("PASS: scoped directory catalog and one selected authenticated file download",flush=True)
                return
            if clipboard_test:
                print("PASS: synthetic bidirectional text clipboard sync, baseline and echo suppression",flush=True)
                return
            if av_test:
                print("PASS: authenticated VP8 and Opus RTP with inert input and release",flush=True)
                return
            if audio_test:
                print("PASS: synthetic audio transport and verified sample count",flush=True)
                return
            if exchange_sender:
                if directory_test:
                    if not os.path.isdir(os.path.join(target,"empty")):raise RuntimeError("Missing empty directory")
                    target=os.path.join(target,"sub","payload.bin")
                with open(target,"rb") as stream:
                    if stream.read()!=payload:raise RuntimeError("Transferred payload mismatch")
                print("PASS: authenticated "+("directory" if directory_test else "file")+" exchange "+exchange_sender+("; checkpoint prefix reused" if resume_test else "")+"; bytes verified",flush=True)
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
