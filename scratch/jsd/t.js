const {JSDOM}=require('jsdom');const fs=require('fs');
const html=fs.readFileSync('../../static/index.html','utf8').replace(/<script src[^>]*><\/script>/g,'');
const dom=new JSDOM(html,{runScripts:'outside-only',url:'http://localhost:8081/'});
const w=dom.window;
w.localStorage.setItem('tce_token','x');w.localStorage.setItem('tce_user_id','1');w.localStorage.setItem('tce_username','navin');
w.fetch=()=>Promise.resolve({ok:true,status:200,json:()=>Promise.resolve({domains:[],styles:[]})});
for(const f of ['mock_api.js','app.js']){try{w.eval(fs.readFileSync('../../static/'+f,'utf8'))}catch(e){console.log('LOADERR',f,e.message)}}
w.document.dispatchEvent(new w.Event('DOMContentLoaded'));
try{w.switchTab('ask')}catch(e){console.log('EXC',e.message)}
const p=w.document.getElementById('tab-ask-content');
console.log('class=',p.className,'parent=',p.parentElement.id||p.parentElement.className,'children=',p.children.length);
let n=p,chain=[];while(n&&n.id!=='main-section'&&n.parentElement){n=n.parentElement;chain.push((n.id||n.className)+':'+n.style.display)}
console.log(chain.join(' > '));
console.log('main-section display=',w.document.getElementById('main-section').style.display);
console.log('nested ask in compress?',w.document.getElementById('tab-compress').contains(p));
