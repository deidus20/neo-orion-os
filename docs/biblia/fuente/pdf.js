const B=__dirname, R=require('path').resolve(__dirname,'../../..');   // carpeta de la biblia y raíz del repo
const {chromium}=require('playwright');
(async()=>{const b=await chromium.launch();const p=await b.newPage();
await p.goto('file://'+B+'/biblia.html',{waitUntil:'networkidle'});
await p.evaluate(()=>document.fonts.ready);await p.waitForTimeout(800);
await p.pdf({path:B+'/../La_biblia_de_Neo_Orion.pdf',width:'297mm',height:'210mm',printBackground:true,preferCSSPageSize:true});
await b.close();})();
