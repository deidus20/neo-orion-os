const {chromium}=require('playwright');
(async()=>{const b=await chromium.launch();const p=await b.newPage({viewport:{width:1024,height:768}});
await p.setContent(`<html><body style="margin:0;background:#0d0e10;width:1024px;height:768px;position:relative;overflow:hidden;
background-image:radial-gradient(#1c1e21 1px,transparent 1.3px);background-size:22px 22px">
<div id=l style="position:absolute;left:64px;top:430px"></div>
<div style="position:absolute;left:64px;top:612px;font:12px monospace;letter-spacing:3px;color:#8b8e94">ARRANQUE // ELIGE CÓMO ENCENDER</div>
<div style="position:absolute;right:30px;bottom:24px;font:11px monospace;color:#55585e">NEO ORIÓN OS 0.9 · POLARIS · 0x1A3F</div>
<div style="position:absolute;left:0;bottom:0;width:100%;height:6px;background:#ff2438"></div>
${[[16,16,'l','t'],[1008,16,'r','t'],[16,752,'l','b'],[1008,752,'r','b']].map(([x,y,h,v])=>`<div style="position:absolute;left:${x-(h=='r'?14:0)}px;top:${y-(v=='b'?14:0)}px;width:14px;height:14px;border-${h=='l'?'left':'right'}:2px solid #3a3d42;border-${v=='t'?'top':'bottom'}:2px solid #3a3d42"></div>`).join('')}
<script src="file:///home/claude/neo-orion-os/hud/marathon.js"></script></body></html>`);
await p.addScriptTag({path:'/home/claude/neo-orion-os/hud/marathon.js'});
await p.evaluate(()=>{const l=document.getElementById('l');const a=MX.pixel('NEO',{px:9,color:'#ffffff',anim:false}),c=MX.pixel('ORIÓN',{px:9,anim:false,color:'#ff2438'});c.style.display=a.style.display='block';a.style.marginBottom='12px';l.append(a,c)});
await p.waitForTimeout(300);await p.screenshot({path:'/home/claude/neo-orion-os/config/arranque/fondo.png'});await b.close();})();
