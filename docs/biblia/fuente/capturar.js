// Capturas para la biblia: HUD, instalador y láminas de diseño, con las fuentes reales.
const B=__dirname, R=require('path').resolve(__dirname,'../../..');   // carpeta de la biblia y raíz del repo
const {chromium}=require('playwright');
const OUT=B+'/img/', HUD='file://'+R+'/hud/index.html', INS='file://'+R+'/hud/instalar.html';
const FUENTES=B+'/fuentes.css';
// SOLO=hud-22-puente,ins-07-modulos node capturar.js → recaptura solo esas
const SOLO=(process.env.SOLO||'').split(',').filter(Boolean), quiero=n=>!SOLO.length||SOLO.includes(n);
(async()=>{
 const b=await chromium.launch(); const errs=[];
 const nueva=async(vp={width:1440,height:900},dsf=1.5)=>{const p=await b.newPage({viewport:vp,deviceScaleFactor:dsf});await p.addInitScript(()=>{try{localStorage.setItem('orion.liviano','0')}catch(e){}});await p.route('**/three.min.js',r=>r.fulfill({path:R+'/hud/lib/three.min.js',contentType:'application/javascript'}));await p.route('**/qrcode.min.js',r=>r.fulfill({path:R+'/hud/lib/qrcode.min.js',contentType:'application/javascript'}));p.on('pageerror',e=>errs.push(e.message));return p;};
 const shot=async(p,n)=>{await p.screenshot({path:OUT+n+'.jpg',type:'jpeg',quality:86});console.log('ok',n);};
 const fuentes=async p=>{await p.addStyleTag({path:FUENTES});await p.evaluate(()=>document.fonts.ready);await p.waitForTimeout(200);};
 // ---------- HUD
 const hud=async(nombre,cmds,espera=1600,antes)=>{if(!quiero(nombre))return;const p=await nueva();await p.goto(HUD);await fuentes(p);
   if(antes) await p.evaluate(antes);
   await p.evaluate(()=>{const b=document.getElementById('boot'); if(b) b.style.display='none';});
   for(const c of cmds){await p.evaluate(c=>handle(c),c);await p.waitForTimeout(espera);}
   await p.evaluate(()=>window.scrollTo(0,0));await p.waitForTimeout(300);await shot(p,nombre);await p.close();};
 if(quiero('hud-01-arranque')){const p=await nueva();await p.goto(HUD);await fuentes(p);await p.waitForTimeout(1400);await shot(p,'hud-01-arranque');await p.close();}
 await hud('hud-02-reposo',[],400,()=>{try{localStorage.clear();localStorage.setItem('orion.liviano','0')}catch(e){}});
 await hud('hud-03-chat',['hola','escanea el hardware'],1500);
 await hud('hud-04-hardware',['escanea el hardware']);
 await hud('hud-05-robot',['muéstrame el robot','mueve el hombro a 45 grados']);
 await hud('hud-06-onda',['muéstrame el robot','onda en el codo amplitud 25 frecuencia 0.5'],2500);
 await hud('hud-07-cad',['muéstrame el soporte del servo'],4000);
 await hud('hud-08-lidar',['muéstrame el lidar del robot'],2500);
 await hud('hud-09-radar',['radar del entorno'],2000);
 await hud('hud-10-lentes',['vista de los lentes'],2000);
 await hud('hud-11-gantt',['muéstrame la carta gantt de la oficina']);
 await hud('hud-12-proyecto',['muéstrame el proyecto del brazo']);
 await hud('hud-13-alternativas',['dame alternativas del robot']);
 await hud('hud-14-placa',['diseña una placa de control de bajo costo']);
 await hud('hud-15-nodos',['muestra los nodos']);
 await hud('hud-16-memoria',['abre la memoria']);
 await hud('hud-17-calibrar',['quiero calibrar con una foto']);
 await hud('hud-18-telefono',['vincular teléfono'],2500);
 await hud('hud-19-juegos',['quiero jugar en la nube']);
 await hud('hud-20-navegador',['abre chrome']);
 await hud('hud-21-piloto',['abre chrome','maneja tú'],2600);
 await hud('hud-22-puente',['usa claude para crear el informe del robot con lo que hemos hablado'],5000);
 await hud('hud-23-color',['muéstrame el lidar del robot','cambia el color a #b14bff'],1800);
 await hud('hud-24-patata',['modo patata','muéstrame el soporte del servo'],1800);
 // investigación por fases
 for(const [n,t] of [['hud-25-investigacion-1',2300],['hud-26-investigacion-2',6000],['hud-27-investigacion-3',12000]]){ if(!quiero(n)) continue;
   const p=await nueva();await p.goto(HUD);await fuentes(p);await p.evaluate(()=>{document.getElementById('boot').style.display='none';handle('investiga lidar para robots')});
   await p.waitForTimeout(t); if(n.endsWith('3')){await p.evaluate(()=>RS.cards[3]&&RS.cards[3].click());await p.waitForTimeout(800);} await shot(p,n);await p.close();}
 // alerta del guardián
 if(quiero('hud-28-alerta')){const p=await nueva();await p.goto(HUD);await fuentes(p);await p.evaluate(()=>{document.getElementById('boot').style.display='none';handle('muéstrame el robot')});await p.waitForTimeout(1200);
  await p.evaluate(()=>handle('detente'));await p.waitForTimeout(900);await shot(p,'hud-28-alerta');await p.close();}
 // ---------- Instalador
 const ins=async(nombre,fn,espera=1500,vp)=>{if(!quiero(nombre))return;const p=await nueva(vp);await p.goto(INS);await fuentes(p);await p.evaluate(()=>{try{localStorage.clear();localStorage.setItem('orion.liviano','0')}catch(e){}});await p.reload();await fuentes(p);await p.waitForTimeout(1500);
   if(fn){await p.mouse.click(300,300);await p.waitForTimeout(1400);await p.evaluate(fn);await p.waitForTimeout(espera);} await shot(p,nombre);await p.close();};
 await ins('ins-00-intro',null);
 const paso=(i,extra)=>`(()=>{C.equipo=PERFILES[4];C.disco='nvme0n1';C.confirmado=true;C.usuario='Ana';${extra||''};P=${i};$('#main').innerHTML='';PASOS[${i}].r($('#main'));nav();animarEntrada();})()`;
 const nombres=['idioma','equipo','disco','ia','voz','identidad','modulos','cuenta'];
 for(let i=0;i<8;i++) await ins(`ins-0${i+1}-${nombres[i]}`,paso(i),1700);
 await ins('ins-04b-ia-sinfiltro',paso(3)+";setTimeout(()=>{document.querySelector('#fil b[data-f=\"SIN FILTRO\"]').click();document.querySelector('#lista .op .inf').click()},700)",1700);
 await ins('ins-04c-ia-apps',paso(3)+";setTimeout(()=>{document.querySelector('#fil b[data-f=\"APPS\"]').click();document.querySelector('#lista .op .inf').click()},700)",1700);
 await ins('ins-04d-ia-patata',paso(3,'C.equipo=PERFILES[1];C.ia=[]'),1700);
 await ins('ins-03b-disco-confirmar',paso(2,'C.confirmado=false')+";setTimeout(()=>MX.alerta('BORRADO AUTORIZADO',3000),900)",1500);
 await ins('ins-09-instalando',paso(8),5000);
 await ins('ins-10-completo',paso(8),14000);
 // móvil
 await ins('ins-movil-ia',paso(3),1700,{width:390,height:844});
 if(quiero('hud-movil-investigacion')){const p=await nueva({width:390,height:844},2);await p.goto(HUD);await fuentes(p);await p.evaluate(()=>{document.getElementById('boot').style.display='none';handle('investiga lidar para robots')});await p.waitForTimeout(11000);await shot(p,'hud-movil-investigacion');await p.close();}
 console.log('errores',errs.slice(0,5));await b.close();})();
