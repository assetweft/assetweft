import {
  AmbientLight, Box3, DirectionalLight, LoadingManager, Material, Mesh, PerspectiveCamera,
  Scene, SkinnedMesh, Sphere, Texture, Vector3, WebGLRenderer,
} from 'three';
import type { Object3D } from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import type { Preview, VersionRow } from '../../types';
import { errorMessage } from '../../controller';
import { button, createStatusBadge, el, makeNotice } from '../primitives';

function materials(mesh: Mesh): Material[] {
  return Array.isArray(mesh.material) ? mesh.material : [mesh.material];
}

function disposeModel(root: Object3D) {
  const geometries = new Set<Mesh['geometry']>();
  const surfaceMaterials = new Set<Material>();
  const textures = new Set<Texture>();
  root.traverse((node) => {
    if (!(node instanceof Mesh)) return;
    geometries.add(node.geometry);
    for (const material of materials(node)) surfaceMaterials.add(material);
  });
  for (const material of surfaceMaterials) {
    for (const value of Object.values(material)) {
      if (value instanceof Texture) textures.add(value);
    }
    material.dispose();
  }
  for (const texture of textures) {
    if (typeof ImageBitmap !== 'undefined' && texture.image instanceof ImageBitmap) texture.image.close();
    texture.dispose();
  }
  for (const geometry of geometries) geometry.dispose();
}

function useStaticMeshes(root: Object3D) {
  const skinned: SkinnedMesh[] = [];
  root.traverse((node) => {
    if (!(node instanceof Mesh)) return;
    node.geometry.morphAttributes = {};
    node.morphTargetInfluences = undefined;
    if (node instanceof SkinnedMesh) skinned.push(node);
  });
  for (const node of skinned) {
    const parent = node.parent;
    const mesh = new Mesh(node.geometry, node.material);
    mesh.position.copy(node.position);
    mesh.quaternion.copy(node.quaternion);
    mesh.scale.copy(node.scale);
    mesh.visible = node.visible;
    mesh.add(...node.children.slice());
    parent?.add(mesh);
    node.removeFromParent();
  }
}

function mountModel(host: HTMLElement, model: Object3D) {
  useStaticMeshes(model);
  const bounds = new Box3().setFromObject(model);
  if (bounds.isEmpty()) throw new Error('Core did not supply a renderable mesh.');
  const center = bounds.getCenter(new Vector3());
  const radius = Math.max(bounds.getBoundingSphere(new Sphere()).radius, 0.0001);
  const camera = new PerspectiveCamera(38, 1, Math.max(radius / 1000, 0.00001), Math.max(radius * 100, 1));
  const renderer = new WebGLRenderer({ alpha: true, antialias: true });
  const scene = new Scene();
  const lights = [[3, 5, 4, 2.1], [-4, 2, 2, 0.8], [1, 4, -5, 1.3]];
  for (const [x, y, z, intensity] of lights) {
    const light = new DirectionalLight(0xffffff, intensity);
    light.position.set(x, y, z);
    scene.add(light);
  }
  scene.add(new AmbientLight(0xffffff, 0.45), model);

  const viewport = el('div', undefined, 'viewport');
  const canvas = renderer.domElement;
  canvas.tabIndex = 0;
  canvas.setAttribute('aria-label', 'Static 3D preview. Drag or use Shift and arrow keys to orbit. Scroll to zoom.');
  viewport.append(canvas);
  host.replaceChildren(viewport);
  const controls = new OrbitControls(camera, canvas);
  controls.enablePan = false;
  controls.listenToKeyEvents(canvas);
  const render = () => renderer.render(scene, camera);
  const reset = () => {
    const radians = camera.fov * Math.PI / 360;
    const angle = Math.min(radians, Math.atan(Math.tan(radians) * camera.aspect));
    const distance = radius / Math.sin(angle) * 1.12;
    camera.position.copy(center).addScaledVector(new Vector3(1.3, 0.85, 1.65).normalize(), distance);
    controls.target.copy(center);
    controls.minDistance = radius * 0.15;
    controls.maxDistance = Math.max(radius * 40, distance * 2);
    controls.update();
    render();
  };
  const resize = () => {
    const width = viewport.clientWidth;
    const height = viewport.clientHeight;
    if (!width || !height) return;
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setSize(width, height, false);
    render();
  };
  const observer = new ResizeObserver(resize);
  observer.observe(viewport);
  controls.addEventListener('change', render);
  resize();
  reset();

  return {
    reset,
    wireframe(enabled: boolean) {
      model.traverse((node) => {
        if (node instanceof Mesh) {
          for (const material of materials(node)) {
            if ('wireframe' in material) material.wireframe = enabled;
          }
        }
      });
      render();
    },
    dispose() {
      observer.disconnect();
      controls.dispose();
      renderer.dispose();
      renderer.forceContextLoss();
      disposeModel(model);
    },
  };
}

export function previewStage(host: HTMLElement) {
  const title = el('h2', 'Preview');
  const subtitle = el('p', '', 'muted mono');
  const identification = el('div');
  identification.append(title, subtitle);
  const actions = el('div', undefined, 'actions');
  const bar = el('header', undefined, 'stage-bar');
  bar.append(identification, actions);
  const surface = el('div', undefined, 'stage-view');
  const footer = makeNotice();
  host.append(bar, surface, footer);

  let generation = 0;
  let liveModel: ReturnType<typeof mountModel> | undefined;
  const clear = () => {
    generation++;
    liveModel?.dispose();
    liveModel = undefined;
    surface.classList.remove('image');
    surface.replaceChildren();
    actions.replaceChildren();
    footer.replaceChildren();
  };
  const message = (text: string) => surface.replaceChildren(el('p', text, 'empty muted'));
  const fail = (text: string) => {
    clear();
    message(text);
    footer.append(createStatusBadge('ERROR'));
  };

  return {
    selection(row: VersionRow | null) {
      clear();
      title.textContent = row?.assetId ?? 'Preview';
      subtitle.textContent = row ? `${row.versionId} · ${row.sourceFormat}` : '';
      message(row ? 'Loading preview…' : 'Select a version from the catalog.');
    },
    error: fail,
    async render(preview: Preview) {
      clear();
      const gen = generation;
      if (preview.kind === 'UNAVAILABLE' || preview.kind === 'UNSUPPORTED') {
        message(preview.reason);
        footer.append(createStatusBadge(preview.kind), ' ', createStatusBadge(preview.availability));
        return;
      }
      if (preview.kind === 'PNG' || preview.kind === 'WAV') {
        const media = preview.kind === 'PNG' ? el('img') : el('audio');
        media.addEventListener('error', () => {
          if (gen === generation) fail('The local file could not be decoded.');
        });
        media.src = preview.dataUrl;
        if (media instanceof HTMLImageElement && preview.kind === 'PNG') {
          media.alt = `Preview of ${preview.versionId}`;
          surface.classList.add('image');
          footer.textContent = [preview.previewKind, preview.note].filter(Boolean).join(' · ');
        }
        if (media instanceof HTMLAudioElement && preview.kind === 'WAV') {
          media.controls = true;
          media.setAttribute('aria-label', `Audio preview of ${preview.versionId}`);
          footer.textContent = `${preview.byteLength.toLocaleString()} bytes`;
        }
        surface.replaceChildren(media);
        return;
      }
      footer.append(el('span', preview.notice, 'badge'));
      if (preview.validation.formatStatus !== 'PASS' || preview.validation.role.status !== 'PASS') {
        message('Core did not validate a static mesh for this GLB.');
        footer.append(' ', createStatusBadge(preview.validation.formatStatus),
          ' ', createStatusBadge(preview.validation.role.status));
        return;
      }
      message('Decoding static mesh…');
      let model: Object3D | undefined;
      try {
        const manager = new LoadingManager();
        manager.setURLModifier((url) => {
          if (!/^(blob:|data:)/i.test(url)) throw new Error('External GLB resources are unavailable.');
          return url;
        });
        const bytes = Uint8Array.from(atob(preview.glbBase64), (char) => char.charCodeAt(0));
        model = (await new GLTFLoader(manager).parseAsync(bytes.buffer, '')).scene;
        if (gen !== generation) {
          disposeModel(model);
          return;
        }
        liveModel = mountModel(surface, model);
        const reset = button('Reset view', () => liveModel?.reset());
        const wireframe = button('Wireframe', () => {
          const enabled = wireframe.getAttribute('aria-pressed') !== 'true';
          wireframe.setAttribute('aria-pressed', String(enabled));
          liveModel?.wireframe(enabled);
        });
        wireframe.setAttribute('aria-pressed', 'false');
        actions.append(reset, wireframe);
      } catch (error) {
        if (model && !liveModel) disposeModel(model);
        if (gen === generation) fail(errorMessage(error));
      }
    },
  };
}
