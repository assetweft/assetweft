import * as THREE from 'three';
import {GLTFLoader} from 'three/addons/loaders/GLTFLoader.js';
import {invoke} from '@tauri-apps/api/core';
import {saveEvidence} from './evidence';
const status=document.querySelector<HTMLElement>('#status')!;
const renderer=new THREE.WebGLRenderer({antialias:true,preserveDrawingBuffer:true});
renderer.setPixelRatio(window.devicePixelRatio);renderer.setSize(800,600);renderer.setClearColor(0x101820);
renderer.outputColorSpace=THREE.SRGBColorSpace;renderer.toneMapping=THREE.NoToneMapping;
document.querySelector('#view')!.append(renderer.domElement);
const camera=new THREE.OrthographicCamera(-3,3,2.25,-2.25,0.1,100);camera.position.set(2,3.5,10);camera.lookAt(2,3.5,3);
let scene=new THREE.Scene();let generation=0;
function clear(){scene.traverse(o=>{if(o instanceof THREE.Mesh){o.geometry.dispose();for(const m of Array.isArray(o.material)?o.material:[o.material])m.dispose();}});scene=new THREE.Scene();renderer.render(scene,camera);}
async function load(caseName:string){const current=++generation;clear();status.textContent='Loading '+caseName+' — no PASS claimed';const start=performance.now();
 try {const bytes=await invoke<ArrayBuffer>('load_fixture',{case:caseName,generation:current});if(current!==generation)return;
 const gltf=await new GLTFLoader().parseAsync(bytes,'');if(current!==generation)return;
 scene.add(gltf.scene);scene.add(new THREE.AmbientLight(0xffffff,Math.PI));
 const timings:number[]=[];let previous=performance.now();
 for(let i=0;i<61;i++){await new Promise<void>(resolve=>requestAnimationFrame(()=>resolve()));if(current!==generation)return;renderer.render(scene,camera);const now=performance.now();if(i)timings.push(now-previous);previous=now;}
 const meshes:object[]=[];gltf.scene.updateMatrixWorld(true);gltf.scene.traverse(o=>{if(o instanceof THREE.Mesh){const m=o.material as THREE.MeshStandardMaterial;meshes.push({worldMatrix:o.matrixWorld.toArray(),color:m.color.toArray(),metalness:m.metalness,roughness:m.roughness});}});
 status.textContent='Rendered technical static Mesh · Core admitted fixed bytes\nTEST_FIXTURE / CC0 declaration unverified · quality/Skin/Morph/animation NOT_RUN';
 const observations={caseName,loadToFirstFrameMs:timings.length?previous-start-timings.reduce((a,b)=>a+b,0):null,frameIntervalsMs:timings,canvasRect:renderer.domElement.getBoundingClientRect().toJSON(),innerWidth,innerHeight,devicePixelRatio:devicePixelRatio,width:renderer.domElement.width,height:renderer.domElement.height,camera:{position:camera.position.toArray(),target:[2,3.5,3],left:-3,right:3,top:2.25,bottom:-2.25},meshes,userAgent:navigator.userAgent};
 await saveEvidence(current, observations, renderer.domElement, invoke, () => current === generation);
 }catch(error){if(current===generation){clear();status.textContent='ERROR: '+String(error);}}
}
for(const name of ['fixture','empty','malformed'])document.getElementById(name)!.onclick=()=>void load(name);
window.addEventListener('unload',()=>{clear();renderer.dispose();});void load('fixture');
