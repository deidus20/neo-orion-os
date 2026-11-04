const B=__dirname, R=require('path').resolve(__dirname,'../../..');   // carpeta de la biblia y raíz del repo
const {chromium}=require('playwright');const fs=require('fs');
(async()=>{const b=await chromium.launch();const errs=[];
const run=async(n,cmds,esp,acc)=>{const p=await b.newPage({viewport:{width:1440,height:900},deviceScaleFactor:1.5});await p.addInitScript(()=>{try{localStorage.setItem('orion.liviano','0')}catch(e){}});p.on('pageerror',e=>errs.push(e.message));
 await p.route('**/three.min.js',r=>r.fulfill({path:R+'/hud/lib/three.min.js',contentType:'application/javascript'}));
 await p.route('**/qrcode.min.js',r=>r.fulfill({path:R+'/hud/lib/qrcode.min.js',contentType:'application/javascript'}));
 await p.goto('file://'+R+'/hud/index.html');await p.addStyleTag({path:B+'/fuentes.css'});await p.evaluate(()=>document.fonts.ready);
 await p.evaluate(()=>document.getElementById('boot').style.display='none');
 for(const c of cmds){await p.evaluate(c=>handle(c),c);await p.waitForTimeout(esp);} if(acc) await p.evaluate(acc);
 await p.evaluate(()=>window.scrollTo(0,0));await p.waitForTimeout(500);await p.screenshot({path:B+'/img/'+n+'.jpg',type:'jpeg',quality:86});console.log('ok',n);await p.close();};
await run('hud-07-cad',['muéstrame el soporte del servo'],4500);
await run('hud-07b-cad-reducir',['muéstrame el soporte del servo','reduce el grosor a 3'],4000);
await run('hud-18-telefono',['vincular teléfono'],2500);
console.log(errs.slice(0,3));await b.close();})();
