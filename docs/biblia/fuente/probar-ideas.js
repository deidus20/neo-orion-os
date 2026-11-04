const B=__dirname, R=require('path').resolve(__dirname,'../../..');   // carpeta de la biblia y raíz del repo
const {chromium}=require('playwright');
(async()=>{const b=await chromium.launch();const p=await b.newPage({viewport:{width:1440,height:900},deviceScaleFactor:1});
const errores=[];p.on('pageerror',e=>errores.push(e.message));p.on('console',m=>{if(m.type()==='error')errores.push(m.text())});
await p.route(/three(\.min)?\.js/,r=>r.fulfill({path:R+'/hud/lib/three.min.js'}));
await p.route(/qrcode(\.min)?\.js/,r=>r.fulfill({path:R+'/hud/lib/qrcode.min.js'}));
await p.addInitScript(()=>{try{localStorage.setItem('orion.liviano','0')}catch(e){}});
await p.goto('file://'+R+'/hud/index.html');await p.waitForTimeout(3500);
const casos=[['llave','cambia mi frase'],['drv','revisa los drivers de mi gráfica'],['elec','qué cable necesito para 4500 W a 40 metros en 220'],['resp','respalda ahora'],['remo','quiero entrar desde el teléfono'],['cnc','abre la fresadora cnc'],
 ['cams','muéstrame las cámaras'],['casa','apaga la luz del taller'],['dict','dicta un informe para procesos industriales'],['upd','hay actualizaciones'],['trad','modo traductor con inglés']];
for(const [k,q] of casos){
  await p.evaluate(()=>{document.querySelectorAll('#panels .panel').forEach(x=>x.remove());for(const k in OPEN)delete OPEN[k]});
  await p.evaluate(q=>handle(q),q);await p.waitForTimeout(1600);
  if(k==='remo'){await p.click('[data-m="casa"]');await p.waitForTimeout(500);}
  if(k==='trad'){await p.fill('#tr-in','hola, ¿cómo estás?');await p.press('#tr-in','Enter');await p.waitForTimeout(500);}
  const hay=await p.evaluate(k=>!!OPEN[k]||k==='casa',k);
  const dicho=await p.evaluate(()=>document.querySelector('#heard')?.innerText||'');
  console.log(k, hay?'panel':'SIN PANEL', '|', dicho.slice(0,110).replace(/\n/g,' '));
  if(['drv','elec','cnc','trad','remo','dict'].includes(k)) await p.screenshot({path:`${B}/img2/hud-${k}.jpg`,quality:80,type:'jpeg'});
}
console.log('errores:',errores.length? errores.slice(0,5):'ninguno');await b.close();})();
