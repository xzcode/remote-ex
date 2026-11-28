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
