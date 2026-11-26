// Requires the real built/staged assets, tests only a fresh private copy.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {verify} from "./verify-mobile-stage.mjs";
const source=process.argv[2];assert(source,"Pass a staged asset directory");
const report=await verify(source);assert(report.valid&&!report.network_started);assert.equal(report.mode,"disabled");
const parent=await fs.mkdtemp(path.join(os.tmpdir(),"remote-ex-stage-test-"));
try{
    const target=path.join(parent,"copy");await fs.cp(source,target,{recursive:true});
    assert.deepEqual(await verify(target),report);
    const headers=path.join(target,"headers.json"),original=await fs.readFile(headers);
    const bad=JSON.parse(original);bad["Content-Security-Policy"]="default-src *";
    await fs.writeFile(headers,JSON.stringify(bad));await assert.rejects(verify(target),/headers/);
    await fs.writeFile(headers,original);
    await fs.writeFile(path.join(target,"unexpected.txt"),"unexpected");await assert.rejects(verify(target),/file list/);
    await fs.unlink(path.join(target,"unexpected.txt"));
    await fs.symlink("mobile.html",path.join(target,"extra-link"));await assert.rejects(verify(target),/Symlink/);
    await fs.unlink(path.join(target,"extra-link"));
    assert.deepEqual(await verify(target),report);
}finally{await fs.rm(parent,{recursive:true,force:true});}
console.log("PASS real staged asset tree, exact security policy, digest, extra-file and symlink rejection");
