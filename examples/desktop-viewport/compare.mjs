// Responsibility: independent analytic projection/material image gate; dev-only Node/pngjs.
// Callers: author native evidence; invariant: expected geometry comes from original fixture bytes.
import fs from 'node:fs';import crypto from 'node:crypto';import path from 'node:path';
import {inflateSync} from 'node:zlib';import {execFileSync} from 'node:child_process';import {PNG} from 'pngjs';
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
try {
const [imagePath,observationsPath,...flags]=process.argv.slice(2);const native=flags.includes('--native');
const bytes=fs.readFileSync(new URL('../godot-static/triangle.glb',import.meta.url));
const json=JSON.parse(bytes.subarray(20,20+bytes.readUInt32LE(12)).toString());
const binStart=20+bytes.readUInt32LE(12)+8;const primitive=json.meshes[0].primitives[0];
const accessor=json.accessors[primitive.attributes.POSITION],view=json.bufferViews[accessor.bufferView];
const node=json.nodes.find(n=>n.mesh===0),tr=node.translation??[0,0,0],sc=node.scale??[1,1,1];
// Only this pinned fixture/fixed camera is supported; this is not a general GLB gate.
if(sha(bytes)!=='44e32648c9f09f1ee221b89b7da61dd29aecf4227cf6c704dd73ca3ab89c93c1')throw Error('Unsupported fixture');
const raw=fs.readFileSync(imagePath);let comparison=raw;
let colorManagement={rawSha256:sha(raw),conversion:'none',source:'untagged canvas sRGB'};
let icc=null,srgb=false;
for(let pos=8;pos<raw.length;){const n=raw.readUInt32BE(pos),type=raw.toString('ascii',pos+4,pos+8),data=raw.subarray(pos+8,pos+8+n);pos+=12+n;
 if(type==='iCCP'){if(icc)throw Error('Multiple ICC profiles');const end=data.indexOf(0);if(end<1||data[end+1]!==0)throw Error('Unsupported ICC encoding');icc=inflateSync(data.subarray(end+2),{maxOutputLength:1024*1024});}
 if(type==='sRGB')srgb=true;
}
if(icc){const profileSha256=sha(icc);colorManagement.source=profileSha256;
 const p3=['737bf33b8f467602ceeb963f457dd9368220862551b2a7f606517625a265fcd7','0ff6958f98684c61f6bbdce1368ddeaf3873baf84545baba482e920d92a914c0'];
 if(profileSha256!=='2b3aa1645779a9e634744faf9b01e9102b0c9b88fd6deced7934df86b949af7e'){
  if(!p3.includes(profileSha256)||process.platform!=='darwin')throw Error('Unsupported ICC profile/platform');
  const dir=flags[flags.indexOf('--canonical-dir')+1];if(!flags.includes('--canonical-dir')||!dir||dir.startsWith('--'))throw Error('P3 requires explicit --canonical-dir for disclosed derivative');
  const version=execFileSync('/usr/bin/sips',['--version'],{encoding:'utf8'}).trim();if(version!=='sips-316')throw Error('Unsupported sips version: '+version);
  const target='/System/Library/ColorSync/Profiles/sRGB Profile.icc',targetSha256=sha(fs.readFileSync(target));
  if(targetSha256!=='2b3aa1645779a9e634744faf9b01e9102b0c9b88fd6deced7934df86b949af7e')throw Error('Unsupported target profile');
  const derivative=path.resolve(dir,sha(raw)+'.srgb.png');fs.mkdirSync(path.dirname(derivative),{recursive:true});fs.writeFileSync(derivative,raw,{flag:'wx'});
  const argv=['--matchToWithIntent',target,'relative',derivative];
  const receipt=execFileSync('/usr/bin/sips',argv,{encoding:'utf8',timeout:10000});comparison=fs.readFileSync(derivative);
  colorManagement={...colorManagement,conversion:'ColorSync ICC relative to sRGB; comparison derivative, NOT raw capture',version,targetSha256,derivative,derivativeSha256:sha(comparison),argv,receipt};
 }
}else if(srgb){colorManagement.source='PNG sRGB chunk';}else if(native)throw Error('Native capture requires a supported embedded profile');
const o=JSON.parse(fs.readFileSync(observationsPath)),png=PNG.sync.read(comparison);
const scale=native?png.width/o.innerWidth:o.devicePixelRatio;
const rect=native?o.canvasRect:{left:0,top:0,width:800,height:600};
const chrome=native?png.height/scale-o.innerHeight:0;
const expected=[];
for(let i=0;i<accessor.count;i++){const xyz=[0,1,2].map(k=>bytes.readFloatLE(binStart+(view.byteOffset??0)+(accessor.byteOffset??0)+i*(view.byteStride??12)+k*4)*sc[k]+tr[k]);
 expected.push([(rect.left+(xyz[0]-2+3)/6*800)*scale,(chrome+rect.top+(2.25-(xyz[1]-3.5))/4.5*600)*scale]);}
let bounds=[Math.min(...expected.map(p=>p[0])),Math.min(...expected.map(p=>p[1])),Math.max(...expected.map(p=>p[0])),Math.max(...expected.map(p=>p[1]))];
if(flags.includes('--wrong-bounds'))bounds=bounds.map(v=>v+40*scale);
let actual=[Infinity,Infinity,-Infinity,-Infinity],count=0,badMaterial=0;const rows=new Map();
for(let y=Math.floor((chrome+rect.top)*scale);y<Math.min(png.height,(chrome+rect.top+600)*scale);y++)for(let x=Math.floor(rect.left*scale);x<Math.min(png.width,(rect.left+800)*scale);x++){const i=(y*png.width+x)*4;const [r,g,b]=png.data.subarray(i,i+3);if(g>100&&g>r*2&&g>b*2){actual=[Math.min(actual[0],x),Math.min(actual[1],y),Math.max(actual[2],x),Math.max(actual[3],y)];count++;const row=rows.get(y)??{min:x,max:x,count:0};row.max=x;row.count++;rows.set(y,row);if(r>10||b>10)badMaterial++;}}
const declared=json.materials[primitive.material].pbrMetallicRoughness;
const validMaterial=declared.baseColorFactor.join(',')==='0,1,0,1'&&declared.metallicFactor===0&&declared.roughnessFactor===1;
// Analytic scanline silhouette from the three projected vertices. Edge pixels
// retain the existing 3*scale raster tolerance; interior rows must be contiguous.
let badRows=0,checkedRows=0;const tolerance=3*scale;
const yMin=Math.min(...expected.map(p=>p[1])),yMax=Math.max(...expected.map(p=>p[1]));
for(let y=Math.ceil(yMin+tolerance);y<yMax-tolerance;y++){
 const xs=[];for(let j=0;j<3;j++){const a=expected[j],b=expected[(j+1)%3];if((a[1]<=y+0.5&&b[1]>y+0.5)||(b[1]<=y+0.5&&a[1]>y+0.5))xs.push(a[0]+(y+0.5-a[1])*(b[0]-a[0])/(b[1]-a[1]));}
 if(xs.length!==2)throw Error('Unsupported silhouette');const lo=Math.min(...xs),hi=Math.max(...xs),row=rows.get(y);checkedRows++;
 if(!row||Math.abs(row.min-lo)>tolerance||Math.abs(row.max-hi)>tolerance||row.max-row.min+1-row.count>2*tolerance)badRows++;
}
const pass=checkedRows>0&&badRows===0&&count>100&&bounds.every((v,i)=>Math.abs(v-actual[i])<=3*scale)&&validMaterial&&badMaterial/count<0.03;
console.log(JSON.stringify({status:pass?'PASS':'FAIL',image:imagePath,imageSha256:crypto.createHash('sha256').update(fs.readFileSync(imagePath)).digest('hex'),inputSha256:crypto.createHash('sha256').update(bytes).digest('hex'),colorManagement,checkedRows,badRows,expected:bounds,actual,count,badMaterial,scale,chrome,declared,tolerancePx:3*scale,native},null,2));process.exit(pass?0:2);

}catch(error){console.error(JSON.stringify({status:"FAIL",error:String(error)}));process.exit(2);}
