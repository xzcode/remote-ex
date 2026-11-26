// One authorized touch pointer, with explicit gesture selection.
export const modes=["tap","drag","right","scroll"];
export class TouchGesture{
    constructor(mode,send){if(!modes.includes(mode))throw Error("Unsupported touch mode");this.mode=mode;this.send=send;this.current=null;this.held=false;}
    emit(kind,value){if(!this.send(kind,value))throw Error("Touch input unavailable");}
    start(id,p,x,y){
        if(this.current||!p)return false;
        this.current={id,x,y,lastX:x,lastY:y,moved:false,lastTime:0,remainderX:0,remainderY:0};
        if(this.mode==="drag"){this.emit("move",p);this.emit("button",{button:1,down:true});this.held=true;}
        return true;
    }
    move(id,p,x,y,now){
        const c=this.current;if(!c||c.id!==id||!p)return false;
        if(Math.hypot(x-c.x,y-c.y)>12)c.moved=true;
        if(this.mode==="drag"&&now-c.lastTime>=25){this.emit("move",p);c.lastTime=now;}
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
    end(id,p,x,y){
        const c=this.current;if(!c||c.id!==id)return false;
        this.current=null;
        if(this.mode==="drag"){
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
