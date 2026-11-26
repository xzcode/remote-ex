// Only the disposable Xvfb / WebKit acceptance test imports this helper.
// Screenshots remain in memory. No physical device, desktop or user data.
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
const decode=String.raw`import io,json,sys
from PIL import Image
raw=sys.stdin.buffer.read(4*1024*1024+1)
assert 0<len(raw)<=4*1024*1024
with Image.open(io.BytesIO(raw)) as im:
    assert im.format=="PNG" and 40<=im.width<=4096 and 40<=im.height<=4096 and im.width*im.height<=4000000
    rgb=im.convert("RGB")
    points=[list(rgb.getpixel((int(im.width*x),int(im.height*y)))) for y in (.35,.5,.65) for x in (.4,.5,.6)]
    print(json.dumps(points,separators=(",",":")))`;
export function matches(samples,color){
    assert(["green","blue"].includes(color),"Unknown fixture color");
    assert(Array.isArray(samples)&&samples.length===9&&samples.every(p=>Array.isArray(p)&&p.length===3&&p.every(n=>Number.isInteger(n)&&n>=0&&n<=255)),"Invalid screenshot sample");
    const wanted=([r,g,b])=>color==="green"?g>120&&g>r+50&&g>b+25:b>140&&b>r+50&&b>g+40;
    return samples.filter(wanted).length>=8;
}
export function samplesFromPng(png){
    assert(Buffer.isBuffer(png)&&png.length>0&&png.length<=4*1024*1024,"Unexpected fixture screenshot size");
    return JSON.parse(execFileSync("/usr/bin/python3",["-c",decode],{input:png,encoding:"utf8",maxBuffer:4096,timeout:3000}));
}
async function sampleVisible(page,color){
    const video=page.locator("#view-video"),end=Date.now()+6500;
    let last=null;
    do{
        if(await page.locator("#view-status").getAttribute("data-state")!=="viewing")throw Error("Receiver stopped before visible pixel proof");
        const png=await video.screenshot({type:"png",animations:"disabled",timeout:2500,scale:"css"});
        last=samplesFromPng(png);
        if(matches(last,color)){
            console.log("WEBKIT_RENDERED_PIXELS "+JSON.stringify({color,match:true,samples:last}));
            return;
        }
        await page.waitForTimeout(200);
    }while(Date.now()<end);
    console.log("WEBKIT_RENDERED_PIXELS "+JSON.stringify({color,match:false,samples:last}));
    // Canvas readback is diagnostic only: screenshot evidence must pass.
    const canvas=await page.evaluate(()=>{
        const v=document.querySelector("#view-video"),c=document.createElement("canvas");
        if(!v.videoWidth||!v.videoHeight)return null;
        c.width=v.videoWidth;c.height=v.videoHeight;
        const ctx=c.getContext("2d");if(!ctx)return null;
        ctx.drawImage(v,0,0);
        return Array.from(ctx.getImageData(Math.floor(c.width/2),Math.floor(c.height/2),1,1).data);
    }).catch(()=>null);
    console.log("WEBKIT_CANVAS_DIAGNOSTIC "+JSON.stringify(canvas));
    throw Error("Actual visible WebKit desktop color absent: "+color);
}
export async function verifyWebKitPixels(page){
    assert.equal(process.env.RUNNER_ENVIRONMENT,"github-hosted");
    assert.match(process.env.DISPLAY??"",/^:\d+$/);
    await page.locator("#view-video").scrollIntoViewIfNeeded({timeout:3000});
    await sampleVisible(page,"green");
    const frames=Number(await page.locator("#view-frames").textContent());
    // Change the independently running sender's isolated desktop after
    // confirming its first visible color. Stale decoded/painted frames
    // cannot pass both phases; no DOM-generated expected image is used.
    execFileSync("xsetroot",["-solid","#325bcf"],{stdio:"ignore",timeout:5000});
    await page.waitForFunction(previous=>Number(document.querySelector("#view-frames").textContent)>=previous+3,frames,{timeout:4000});
    await sampleVisible(page,"blue");
}
export function selfTest(){
    const nine=p=>Array.from({length:9},()=>p.slice());
    assert(matches(nine([38,183,115]),"green"));
    assert(matches(nine([50,91,207]),"blue"));
    assert(!matches(nine([0,0,0]),"green"));
    assert(!matches(nine([38,183,115]),"blue"));
    assert(!matches(nine([25,33,45]),"blue"));
    const partial=nine([50,91,207]);partial[0]=[0,0,0];assert(matches(partial,"blue"));partial[1]=[0,0,0];assert(!matches(partial,"blue"));
    // Independently exercise the real PNG decoder, if Pillow is installed.
    const png=execFileSync("/usr/bin/python3",["-c","from PIL import Image;import sys;Image.new('RGB',(120,80),(50,91,207)).save(sys.stdout.buffer,format='PNG')"],{timeout:3000,maxBuffer:65536});
    assert(matches(samplesFromPng(png),"blue"));
    console.log("PASS WebKit rendered pixel fixture: two strict color gates, black/stale rejection and real PNG sampling");
}
if(process.argv[2]==="--self-test")selfTest();
