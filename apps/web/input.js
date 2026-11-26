// Pure bounded v1 protobuf encoding and physical keyboard/viewport mapping.
// Only used by explicitly authorized profile-2 controller. No OS input APIs.
import {concat} from "./crypto.js";
import {TouchGesture,modes} from "./touch.js";
const require=(v,m)=>{if(!v)throw Error(m);};
function uint(n,max){require(Number.isSafeInteger(n)&&n>=0&&n<=max,"Input value outside protocol bounds");return n;}
function varint(n){uint(n,Number.MAX_SAFE_INTEGER);const out=[];while(n>=128){out.push(n%128+128);n=Math.floor(n/128);}out.push(n);return Uint8Array.from(out);}
const field=(tag,n)=>concat(varint(tag*8),varint(n));
const bytes=(tag,b)=>concat(varint(tag*8+2),varint(b.length),b);
const zigzag=n=>n<0?-2*n-1:2*n;
export class InputEncoder{
    constructor(sid){require(sid instanceof Uint8Array&&sid.length===16&&sid.some(x=>x!==0),"Invalid input session");this.sid=sid.slice();this.sequence=0;}
    encode(kind,value={}){
        let tag,body;
        if(kind==="move"){tag=10;body=concat(field(1,uint(value.x,65535)),field(2,uint(value.y,65535)));}
        else if(kind==="button"){tag=11;require(typeof value.down==="boolean"&&uint(value.button,3)>0,"Unsupported pointer button");body=concat(field(1,value.button),field(2,Number(value.down)));}
        else if(kind==="key"){tag=12;require(typeof value.down==="boolean"&&uint(value.usage,231)>=4,"Invalid physical key");body=concat(field(1,value.usage),field(2,Number(value.down)));}
        else if(kind==="scroll"){tag=13;require(Number.isInteger(value.x)&&Number.isInteger(value.y)&&Math.abs(value.x)<=1200&&Math.abs(value.y)<=1200,"Scroll outside bounds");body=concat(field(1,zigzag(value.x)),field(2,zigzag(value.y)));}
        else if(kind==="release"){tag=14;body=new Uint8Array();}
        else throw Error("Unsupported input event");
        const sequence=uint(this.sequence+1,Number.MAX_SAFE_INTEGER);
        const out=concat(field(1,1),bytes(2,this.sid),field(3,sequence),bytes(tag,body));
        require(out.length<=1024,"Input packet too large");this.sequence=sequence;return out;
    }
}
const named={Enter:40,Escape:41,Backspace:42,Tab:43,Space:44,Minus:45,Equal:46,BracketLeft:47,BracketRight:48,Backslash:49,Semicolon:51,Quote:52,Backquote:53,Comma:54,Period:55,Slash:56,CapsLock:57,Insert:73,Home:74,PageUp:75,Delete:76,End:77,PageDown:78,ArrowRight:79,ArrowLeft:80,ArrowDown:81,ArrowUp:82,ControlLeft:224,ShiftLeft:225,AltLeft:226,MetaLeft:227,ControlRight:228,ShiftRight:229,AltRight:230,MetaRight:231};
export function physicalKey(code){
    if(typeof code!=="string")return null;
    if(/^Key[A-Z]$/.test(code))return code.charCodeAt(3)-65+4;
    if(/^Digit[1-9]$/.test(code))return Number(code[5])+29;
    if(code==="Digit0")return 39;
    if(/^F(?:[1-9]|1[0-2])$/.test(code))return Number(code.slice(1))+57;
    return Object.hasOwn(named,code)?named[code]:null;
}
export function pointer(rect,geometry,x,y,canvasWidth=960,canvasHeight=540){
    if(![rect.left,rect.top,rect.width,rect.height,x,y,canvasWidth,canvasHeight,geometry.x,geometry.y,geometry.width,geometry.height].every(Number.isFinite)||rect.width<=0||rect.height<=0||canvasWidth<=0||canvasHeight<=0||geometry.width<2||geometry.height<2)return null;
    const scale=Math.min(rect.width/canvasWidth,rect.height/canvasHeight);
    const px=(x-rect.left-(rect.width-canvasWidth*scale)/2)/scale-geometry.x;
    const py=(y-rect.top-(rect.height-canvasHeight*scale)/2)/scale-geometry.y;
    if(px<0||py<0||px>=geometry.width||py>=geometry.height)return null;
    return {x:Math.min(65535,Math.round(px/(geometry.width-1)*65535)),y:Math.min(65535,Math.round(py/(geometry.height-1)*65535))};
}
export function wheel(delta,mode){
    if(!Number.isFinite(delta)||![0,1,2].includes(mode))return 0;
    return Math.max(-1200,Math.min(1200,Math.round(delta*(mode===1?40:mode===2?120:1))));
}
// The host has a 300 events/s fail-closed budget; cap pointer movement well
// below it. Held state is released before disable/normal close; host cleanup
// independently releases on transport failure.
export class SurfaceInput{
    constructor({video,geometry,encoder,send,onFailure,onDisabled,touchMode="mouse"}){
        this.video=video;this.geometry=geometry;this.encoder=encoder;this.send=send;this.onFailure=onFailure;this.onDisabled=onDisabled;
        this.keys=new Set();this.buttons=new Set();this.active=false;this.lastMove=0;this.rateStart=0;this.rate=0;
        this.handlers=[];this.touchMode=touchMode;
        require(touchMode==="mouse"||modes.includes(touchMode),"Invalid touch mode");
        this.touch=touchMode==="mouse"?null:new TouchGesture(touchMode,(kind,value)=>this.emit(kind,value));
    }
    listen(target,name,fn,options){target.addEventListener(name,fn,options);this.handlers.push(()=>target.removeEventListener(name,fn,options));}
    emit(kind,value){
        const now=performance.now();if(now-this.rateStart>=1000){this.rateStart=now;this.rate=0;}
        if(kind!=="release"&&++this.rate>160){this.disable();return false;}
        try{this.send(this.encoder.encode(kind,value));return true;}
        catch{this.onFailure();return false;}
    }
    point(e){if(this.video.videoWidth!==960||this.video.videoHeight!==540)return null;return pointer(this.video.getBoundingClientRect(),this.geometry,e.clientX,e.clientY);}
    enabled(e){return this.active&&e.isTrusted&&document.activeElement===this.video&&!document.hidden;}
    enable(){
        if(this.active)return;
        this.active=true;this.rateStart=performance.now();this.rate=0;this.video.tabIndex=0;this.video.focus({preventScroll:true});
        if(document.activeElement!==this.video){this.disable();return;}
        const v=this.video,button=e=>({0:1,1:2,2:3})[e.button];
        if(this.touch){
            v.classList.add("touch-active");
            const safe=fn=>e=>{if(!this.active||!e.isTrusted||e.pointerType!=="touch")return;try{fn(e);}catch{this.disable();}};
            this.listen(v,"pointerdown",safe(e=>{
                if(!this.enabled(e)||this.touch.current){this.disable();return;}
                const p=this.point(e);if(!p)return;
                e.preventDefault();if(!this.touch.start(e.pointerId,p,e.clientX,e.clientY))this.disable();
                else v.setPointerCapture(e.pointerId);
            }),{passive:false});
            this.listen(v,"pointermove",safe(e=>{
                if(!this.enabled(e)||!this.touch.current)return;
                e.preventDefault();if(!this.touch.move(e.pointerId,this.point(e),e.clientX,e.clientY,performance.now()))this.disable();
            }),{passive:false});
            this.listen(v,"pointerup",safe(e=>{
                if(!this.touch.current)return;
                e.preventDefault();if(!this.touch.end(e.pointerId,this.point(e),e.clientX,e.clientY))this.disable();
            }),{passive:false});
            this.listen(v,"lostpointercapture",safe(e=>{if(this.touch.current?.id===e.pointerId)this.disable();}));
            this.listen(window,"pointerdown",safe(e=>{if(this.touch.current&&e.pointerId!==this.touch.current.id)this.disable();}),true);
        }
        this.listen(v,"pointermove",e=>{if(!this.enabled(e)||e.pointerType!=="mouse")return;const p=this.point(e);if(!p){if(this.buttons.size)this.disable();return;}const now=performance.now();if(now-this.lastMove>=25){this.lastMove=now;this.emit("move",p);}});
        this.listen(v,"pointerdown",e=>{if(!this.enabled(e)||e.pointerType!=="mouse")return;const p=this.point(e),b=button(e);if(!p||!b||this.buttons.has(b))return;e.preventDefault();if(this.emit("move",p)&&this.emit("button",{button:b,down:true}))this.buttons.add(b);});
        this.listen(window,"pointerup",e=>{if(!this.active||!e.isTrusted)return;const b=button(e);if(this.buttons.has(b)){e.preventDefault();this.emit("button",{button:b,down:false});this.buttons.delete(b);}});
        this.listen(v,"pointerleave",()=>{if(this.buttons.size)this.disable();});
        this.listen(v,"pointercancel",()=>this.disable());
        this.listen(v,"contextmenu",e=>{if(this.enabled(e))e.preventDefault();});
        this.listen(v,"keydown",e=>{if(!this.enabled(e)||e.isComposing||e.repeat)return;const usage=physicalKey(e.code);if(!usage||this.keys.has(usage))return;e.preventDefault();if(this.emit("key",{usage,down:true}))this.keys.add(usage);});
        this.listen(v,"keyup",e=>{if(!this.enabled(e))return;const usage=physicalKey(e.code);if(!this.keys.has(usage))return;e.preventDefault();this.emit("key",{usage,down:false});this.keys.delete(usage);});
        this.listen(v,"wheel",e=>{if(!this.enabled(e))return;const p=this.point(e);if(!p)return;e.preventDefault();const x=wheel(e.deltaX,e.deltaMode),y=wheel(e.deltaY,e.deltaMode);if((x||y)&&this.emit("move",p))this.emit("scroll",{x,y});},{passive:false});
        this.listen(v,"blur",()=>this.disable());this.listen(window,"blur",()=>this.disable());
    }
    disable(){
        if(!this.active)return;
        this.active=false;for(const remove of this.handlers.splice(0))remove();
        this.touch?.cancel();this.video.classList.remove("touch-active");this.emit("release");this.keys.clear();this.buttons.clear();this.video.removeAttribute("tabindex");this.onDisabled();
    }
    dispose(){this.active=false;for(const remove of this.handlers.splice(0))remove();this.touch?.cancel();this.video.classList.remove("touch-active");this.keys.clear();this.buttons.clear();this.video.removeAttribute("tabindex");}
}
