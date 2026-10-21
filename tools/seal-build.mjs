// Keep game-derived AOT bytes out of public Actions artifacts. Only the local
// recipient private key can decrypt a build; it is never committed or uploaded.
import fs from 'node:fs';
import crypto from 'node:crypto';
import {pipeline} from 'node:stream/promises';
import assert from 'node:assert/strict';
const [mode,input,keyPath,output]=process.argv.slice(2);
assert(input && keyPath && output,'seal|open input key output');
assert(!fs.existsSync(output),'refuse to overwrite an existing artifact');
const digest=async path=>{const h=crypto.createHash('sha256');for await(const b of fs.createReadStream(path))h.update(b);return h.digest('hex');};
if(mode==='seal') {
 const key=crypto.randomBytes(32),iv=crypto.randomBytes(12);
 const cipher=crypto.createCipheriv('aes-256-gcm',key,iv);
 await pipeline(fs.createReadStream(input),cipher,fs.createWriteStream(output,{flags:'wx'}));
 const manifest={schema:1,cipher:'AES-256-GCM/RSA-OAEP-SHA256',iv:iv.toString('base64'),key:crypto.publicEncrypt({key:fs.readFileSync(keyPath),oaepHash:'sha256'},key).toString('base64'),tag:cipher.getAuthTag().toString('base64'),sha256:await digest(input),bytes:fs.statSync(input).size};
 fs.writeFileSync(output+'.json',JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
} else if(mode==='open') {
 const m=JSON.parse(fs.readFileSync(input+'.json'));assert.equal(m.schema,1);assert.equal(m.cipher,'AES-256-GCM/RSA-OAEP-SHA256');
 const key=crypto.privateDecrypt({key:fs.readFileSync(keyPath),oaepHash:'sha256'},Buffer.from(m.key,'base64'));
 const cipher=crypto.createDecipheriv('aes-256-gcm',key,Buffer.from(m.iv,'base64'));cipher.setAuthTag(Buffer.from(m.tag,'base64'));
 try {await pipeline(fs.createReadStream(input),cipher,fs.createWriteStream(output,{flags:'wx'}));assert.equal(fs.statSync(output).size,m.bytes);assert.equal(await digest(output),m.sha256);}
 catch(e){fs.rmSync(output,{force:true});throw e;}
 console.log(JSON.stringify({verified:true,sha256:m.sha256,bytes:m.bytes}));
} else {throw Error('unknown mode');}
