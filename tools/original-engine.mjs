// Reads already-published, pinned reference bytes for CI; never uploads assets.
import fs from 'node:fs';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
const revision='7030977aee7f5e5d87a206931d123d9901f793ca';
const base=`https://raw.githubusercontent.com/yosnuffyyy/gta5loader/${revision}/`;
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const pin='e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e';
async function get(path){for(let attempt=0;attempt<3;attempt++){try{const r=await fetch(base+path,{signal:AbortSignal.timeout(60000)});if(!r.ok)throw Error(`HTTP ${r.status}: ${path}`);return Buffer.from(await r.arrayBuffer());}catch(e){if(attempt===2)throw e;await new Promise(r=>setTimeout(r,1000*(attempt+1)));}}}
const output=process.argv[2];assert(output,'output path required');
if(process.argv[3]){const bytes=fs.readFileSync(process.argv[3]);assert.equal(sha(bytes),pin);fs.writeFileSync(output,bytes);}
else {
 const manifest=JSON.parse(await get('offline-manifest.json'));
 const entry=manifest.files.find(v=>v.path==='/b/8b0b5899ed/game.wasm');assert.equal(entry.sha256,pin);
 const parts=[];for(let i=0;i<entry.setupChunks.length;i++){
  const path=entry.setupChunks[i];assert(/^\/gta5loader\/engine32\/game\.wasm\.\d{3}\.[a-f0-9]{12}\.bin$/.test(path));
  const b=await get(path.slice('/gta5loader/'.length));assert.equal(sha(b),entry.chunks[i]);parts.push(b);
 }
 const b=Buffer.concat(parts);assert.equal(b.length,entry.size);assert.equal(sha(b),pin);fs.writeFileSync(output,b);
}
console.log(JSON.stringify({sha256:pin,bytes:fs.statSync(output).size,sourceRevision:revision,redistributed:false}));
