const B=__dirname, R=require('path').resolve(__dirname,'../../..');   // carpeta de la biblia y raíz del repo
const {chromium}=require('playwright');
(async()=>{const b=await chromium.launch();const p=await b.newPage({viewport:{width:1400,height:900},deviceScaleFactor:1.5});const e=[];p.on('pageerror',x=>e.push(x.message));
await p.goto('file://'+B+'/laminas.html');await p.evaluate(()=>document.fonts.ready);await p.waitForTimeout(800);
for(const id of ['logo','paleta','tipo','escala','iconos','comp','hitos','mov']){await p.locator('#'+id).screenshot({path:B+'/img/lam-'+id+'.jpg',type:'jpeg',quality:88});console.log('ok',id);}
console.log(e);await b.close();})();
