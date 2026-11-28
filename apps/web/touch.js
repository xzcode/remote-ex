// One authorized touch pointer, with explicit gesture selection.
export const modes=["smart","tap","drag","right","scroll"];
export const LONG_PRESS_MS=550;
export class TouchGesture{
    constructor(mode,send){if(!modes.includes(mode))throw Error("Unsupported touch mode");this.mode=mode;this.send=send;this.current=null;this.held=false;}
    emit(kind,value){if(!this.send(kind,value))throw Error("Touch input unavailable");}
    start(id,p,x,y,now=performance.now()){
        if(this.current||!p)return false;
        this.current={id,x,y,lastX:x,lastY:y,moved:false,lastTime:0,started:now,remainderX:0,remainderY:0};
        if(this.mode==="smart"){this.emit("move",p);this.current.lastTime=now;}
        if(this.mode==="drag"){this.emit("move",p);this.emit("button",{button:1,down:true});this.held=true;}
        return true;
    }
    move(id,p,x,y,now){
        const c=this.current;if(!c||c.id!==id||!p)return false;
        if(Math.hypot(x-c.x,y-c.y)>12)c.moved=true;
        if((this.mode==="drag"||this.mode==="smart")&&now-c.lastTime>=25){this.emit("move",p);c.lastTime=now;}
        if(this.mode==="scroll"){
            c.remainderX+=c.lastX-x;c.remainderY+=c.lastY-y;
            if(now-c.lastTime>=35){
                const step=v=>Math.max(-1200,Math.min(1200,Math.trunc(v/12)*120));
                const sx=step(c.remainderX),sy=step(c.remainderY);
                if(sx||sy){this.emit("move",p);this.emit("scroll",{x:sx,y:sy});c.remainderX-=sx/10;c.remainderY-=sy/10;c.lastTime=now;}
            }
        }
        c.lastX=x;c.lastY=y;return true;
    }
    end(id,p,x,y,now=performance.now()){
        const c=this.current;if(!c||c.id!==id)return false;
        this.current=null;
        if(this.mode==="smart"){
            if(!p)return false;
            this.emit("move",p);
            // Decide a long-press only on release. Movement never presses
            // a button, and cancellation cannot produce a delayed click.
            if(!c.moved&&Math.hypot(x-c.x,y-c.y)<=12){
                const button=now-c.started>=LONG_PRESS_MS?3:1;
                this.emit("button",{button,down:true});this.emit("button",{button,down:false});
            }
        }else if(this.mode==="drag"){
            if(!p)return false;
            this.emit("move",p);
            if(this.held){this.emit("button",{button:1,down:false});this.held=false;}
        }else if(p&&!c.moved&&Math.hypot(x-c.x,y-c.y)<=12&&(this.mode==="tap"||this.mode==="right")){
            const button=this.mode==="tap"?1:3;
            this.emit("move",p);this.emit("button",{button,down:true});this.emit("button",{button,down:false});
        }
        return true;
    }
    cancel(){this.current=null;this.held=false;}
}


// Two-finger scroll is a distinct input owner, never a second pointer in
// click/drag mode. A lone finger or a spread/pinch cannot send wheel events.
export class TwoFingerScroll{
    constructor(send){this.send=send;this.points=new Map();this.base=null;this.draining=false;this.lastTime=0;}
    start(id,p,x,y,now){
        if(this.draining||this.points.has(id)||this.points.size>=2||!p||![x,y,now].every(Number.isFinite))return false;
        this.points.set(id,{p,x,y});
        if(this.points.size===2){this.base=[...this.points.values()];this.lastTime=now;}
        return true;
    }
    move(id,p,x,y,now){
        if(!this.points.has(id)||!p||![x,y,now].every(Number.isFinite))return false;
        this.points.set(id,{p,x,y});
        if(this.draining||this.points.size!==2||!this.base||now-this.lastTime<35)return true;
        const [a,b]=this.points.values(),[a0,b0]=this.base;
        // Take only concordant motion on each axis. Sequential platform
        // pointermove events for one finger, or symmetric pinching, do not
        // create a wheel event until both fingers move in the same direction.
        const axis=(d1,d2)=>{
            if(!d1||!d2||Math.sign(d1)!==Math.sign(d2))return 0;
            return Math.sign(d1)*Math.min(1200,Math.floor(Math.min(Math.abs(d1),Math.abs(d2))/12)*120);
        };
        const dx=axis(a0.x-a.x,b0.x-b.x),dy=axis(a0.y-a.y,b0.y-b.y);
        if(!dx&&!dy)return true;
        const center={x:Math.round((a.p.x+b.p.x)/2),y:Math.round((a.p.y+b.p.y)/2)};
        if(!this.send("move",center)||!this.send("scroll",{x:dx,y:dy}))return false;
        this.base=this.base.map((old,i)=>({...old,x:old.x-dx/10,y:old.y-dy/10,p:i===0?a.p:b.p}));
        this.lastTime=now;return true;
    }
    end(id){
        if(!this.points.delete(id))return false;
        this.base=null;this.draining=this.points.size>0;return true;
    }
    cancel(){this.points.clear();this.base=null;this.draining=false;this.lastTime=0;}
}


// Local-only magnification. Geometry is calculated against the stable
// containing viewport, not the transformed video. No remote input sender.
export function fitInspection(box,media,scale=1,x=0,y=0){
    if(![box.width,box.height,media.width,media.height,scale,x,y].every(Number.isFinite)||
        Math.min(box.width,box.height,media.width,media.height)<=0)return {scale:1,x:0,y:0};
    scale=Math.max(1,Math.min(3,scale));
    const fit=Math.min(box.width/media.width,box.height/media.height);
    const maxX=Math.max(0,(media.width*fit*scale-box.width)/2),maxY=Math.max(0,(media.height*fit*scale-box.height)/2);
    return {scale,x:Math.max(-maxX,Math.min(maxX,x)),y:Math.max(-maxY,Math.min(maxY,y))};
}
export class LocalInspection{
    constructor({frame,video,onChange}){
        this.frame=frame;this.video=video;this.onChange=onChange;
        this.active=false;this.points=new Map();this.handlers=[];this.base=null;this.value={scale:1,x:0,y:0};
    }
    listen(target,name,fn,options){target.addEventListener(name,fn,options);this.handlers.push(()=>target.removeEventListener(name,fn,options));}
    set(scale,x,y){
        if(!this.active)return;
        this.value=fitInspection(this.frame.getBoundingClientRect(),{width:this.video.videoWidth,height:this.video.videoHeight},scale,x,y);
        const v=this.value;
        this.video.style.transform="translate("+v.x+"px,"+v.y+"px) scale("+v.scale+")";
        this.onChange(v.scale);
    }
    clearGesture(){
        const ids=[...this.points.keys()];this.points.clear();this.base=null;
        for(const id of ids)try{if(this.frame.hasPointerCapture(id))this.frame.releasePointerCapture(id);}catch{}
    }
    reset(){this.clearGesture();this.set(1,0,0);}
    zoom(delta){
        if(!this.active||!Number.isFinite(delta))return;
        this.clearGesture();const v=this.value,next=Math.max(1,Math.min(3,v.scale+delta));
        this.set(next,v.x*next/v.scale,v.y*next/v.scale);
    }
    sample(){
        const p=[...this.points.values()];
        if(!p.length)return null;
        if(p.length===1)return {x:p[0].x,y:p[0].y,d:0};
        return {x:(p[0].x+p[1].x)/2,y:(p[0].y+p[1].y)/2,d:Math.hypot(p[0].x-p[1].x,p[0].y-p[1].y)};
    }
    rebase(){
        const p=this.sample();this.base=p?{...p,value:{...this.value}}:null;
    }
    enable(){
        if(this.active)return true;
        if(document.hidden||this.video.hidden||!this.frame.contains(this.video)||!this.video.videoWidth||!this.video.videoHeight)return false;
        this.active=true;this.frame.classList.add("inspect-frame");this.video.classList.add("inspect-active");this.reset();
        this.listen(this.frame,"pointerdown",e=>{
            if(!this.active||!e.isTrusted||!["touch","mouse"].includes(e.pointerType)||e.button!==0)return;
            if(this.points.has(e.pointerId)||this.points.size>=2||
                this.points.size&&[...this.points.values()].some(p=>p.type!==e.pointerType||p.type==="mouse")){
                this.clearGesture();return;
            }
            if(!this.points.size&&!e.isPrimary)return;
            this.points.set(e.pointerId,{x:e.clientX,y:e.clientY,type:e.pointerType});
            try{this.frame.setPointerCapture(e.pointerId);}catch{this.clearGesture();return;}
            e.preventDefault();this.rebase();
        },{passive:false});
        this.listen(this.frame,"pointermove",e=>{
            if(!this.active||!e.isTrusted||!this.points.has(e.pointerId)||!this.base)return;
            const old=this.points.get(e.pointerId);
            this.points.set(e.pointerId,{...old,x:e.clientX,y:e.clientY});
            const now=this.sample(),b=this.base,r=this.frame.getBoundingClientRect(),c=b.value;
            if(!now)return;
            const scale=b.d>=8?Math.max(1,Math.min(3,c.scale*now.d/b.d)):c.scale;
            const ratio=scale/c.scale,cx=r.left+r.width/2,cy=r.top+r.height/2;
            // Anchor the displayed point beneath the starting midpoint.
            this.set(scale,now.x-cx-(b.x-cx-c.x)*ratio,now.y-cy-(b.y-cy-c.y)*ratio);
            e.preventDefault();
        },{passive:false});
        this.listen(this.frame,"pointerup",e=>{
            if(!e.isTrusted||!this.points.delete(e.pointerId))return;
            try{if(this.frame.hasPointerCapture(e.pointerId))this.frame.releasePointerCapture(e.pointerId);}catch{}
            this.rebase();
        });
        this.listen(this.frame,"lostpointercapture",e=>{if(this.points.has(e.pointerId))this.clearGesture();});
        this.listen(window,"pointercancel",()=>this.clearGesture(),true);
        this.listen(window,"blur",()=>this.clearGesture());
        this.listen(document,"visibilitychange",()=>{if(document.hidden)this.disable();});
        this.listen(this.frame,"contextmenu",e=>{if(e.isTrusted)e.preventDefault();});
        // Toolbar changes alter the stable viewport; discard the gesture and
        // clamp zoom to its new bounds without forwarding any input.
        if(typeof ResizeObserver==="function"){
            this.observer=new ResizeObserver(()=>{if(this.active){this.clearGesture();const v=this.value;this.set(v.scale,v.x,v.y);}});
            this.observer.observe(this.frame);
        }
        return true;
    }
    disable(){
        if(!this.active)return;
        this.active=false;this.observer?.disconnect();this.observer=null;
        for(const remove of this.handlers.splice(0))remove();
        this.clearGesture();
        this.video.style.removeProperty("transform");
        this.video.classList.remove("inspect-active");this.frame.classList.remove("inspect-frame");
        this.value={scale:1,x:0,y:0};this.onChange(1);
    }
}
