const {chromium}=require('playwright');const fs=require('fs');
// secret-shaped test values are built at run time so no key-like literal lives in the repo
const SUB={'{{KEY1}}':'sk_test_'+'4eC39HqLyjWDarjtT1zdp7dc','{{KEY2}}':'sk_test_'+'9Zy8Xw7Vv6Uu5Tt4Ss3Rr2Qq','{{KEY3}}':'sk_test_'+'Qw3Er4Ty5Ui6Op7As8Df9Gh0J','{{JWT}}':['eyJhbGciOiJIUzI1NiJ9','eyJzdWIiOiJ0ZXN0LXVzZXIiLCJyb2xlIjoiYWRtaW4ifQ','c2lnbmF0dXJlLXNpZ25hdHVyZS1zaWduYXR1cmU'].join('.')};
let raw=fs.readFileSync('config.json','utf8');for(const [k,v] of Object.entries(SUB)) raw=raw.split(k).join(v);
const cfg=JSON.parse(raw);const which=process.argv.slice(2);
(async()=>{const br=await chromium.launch();
for(const [name,s] of Object.entries(cfg.sites)){ if(which.length&&!which.includes(name))continue;
 const ctx=await br.newContext({recordHar:{path:name+'.har',content:'embed'}});const pg=await ctx.newPage();const t=Date.now();
 try{await pg.goto(s.land,{timeout:30000,waitUntil:'domcontentloaded'})}catch(e){console.log(name,'landing failed',String(e).slice(0,80))}
 let ok=0;for(const [m,u,h,b] of s.calls){try{const r=await pg.evaluate(async([m,u,h,b])=>{try{const r=await fetch(u,{method:m,headers:Object.assign(b?{'Content-Type':'application/json'}:{},h),body:b?JSON.stringify(b):undefined,mode:'cors',credentials:'omit'});return r.status}catch(e){return -1}},[m,u,h,b]);if(r>0)ok++}catch(e){}}
 await ctx.close();console.log(`${name}: ${s.calls.length} calls driven, ${ok} answered, ${((Date.now()-t)/1000).toFixed(1)}s`)}
for(const [name,urls] of Object.entries(cfg.pages)){ if(which.length&&!which.includes(name))continue;
 const ctx=await br.newContext({recordHar:{path:name+'.har',content:'embed'}});const pg=await ctx.newPage();const t=Date.now();
 for(const u of urls){try{await pg.goto(u,{timeout:30000,waitUntil:'load'})}catch(e){console.log(name,u,'fail',String(e).slice(0,60))}}
 await ctx.close();console.log(`${name}: pages ${urls.length} ${((Date.now()-t)/1000).toFixed(1)}s`)}
await br.close()})();
