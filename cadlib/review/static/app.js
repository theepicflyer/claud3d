import * as THREE from 'three';
import { OrbitControls } from '/static/vendor/OrbitControls.js';
import { STLLoader } from '/static/vendor/STLLoader.js';

const PALETTE = [0x5a8cdc, 0xeb963c, 0x6eb96e, 0xd25a6e, 0xa078c8, 0x78bec8];
const COLORS = { draft: 0xd9731a, open: 0x2f6fd6, resolved: 0x8a8a8f, measure: 0x222222 };
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const fmt = (v) => '(' + v.map((x) => x.toFixed(1)).join(', ') + ')';

const state = {
  project: null, view: null, builtAt: null, threads: [], version: null, connected: true,
  layout: 'print', mode: 'pin', showResolved: false,
  parts: {},          // key -> {name, key, group, front, back, edges, color, frame, visible}
  drafts: [],         // {key, anchor, camera, text, screenshot}
  replies: {},        // thread id -> {text, status}
  measure: null,      // {a, b} hits while measuring
  focus: null,
};

// ---------- scene ----------
const stage = $('stage');
const canvas = $('gl');
const overlay = $('overlay');
const octx = overlay.getContext('2d');
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(window.devicePixelRatio);
renderer.localClippingEnabled = true;
const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(35, 1, 0.1, 20000);
camera.up.set(0, 0, 1);
camera.position.set(250, -250, 200);
scene.add(camera);
const controls = new OrbitControls(camera, canvas);
controls.screenSpacePanning = true;
// Left button belongs to the comment tools. Right-drag pans; OrbitControls turns Ctrl+right-drag into rotate.
// Shift+right-drag can't be used: Firefox-based browsers always open their context menu on Shift+right-click.
controls.mouseButtons = { LEFT: null, MIDDLE: THREE.MOUSE.ROTATE, RIGHT: THREE.MOUSE.PAN };
scene.add(new THREE.HemisphereLight(0xffffff, 0x777777, 1.7));
const headlight = new THREE.DirectionalLight(0xffffff, 1.5);
headlight.position.set(0.4, 0.6, 1);
camera.add(headlight);
const world = new THREE.Group();
const helpers = new THREE.Group();
const measureGroup = new THREE.Group();
scene.add(world, helpers, measureGroup);
const clipPlane = new THREE.Plane(new THREE.Vector3(1, 0, 0), 0);
const section = { axis: '', t: 0.5, flip: false, min: 0, max: 1 };
const raycaster = new THREE.Raycaster();
const loader = new STLLoader();

function resize() {
  const w = stage.clientWidth, h = stage.clientHeight;
  renderer.setSize(w, h, false);
  overlay.width = w * devicePixelRatio;
  overlay.height = h * devicePixelRatio;
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
}
window.addEventListener('resize', resize);

function sceneBox(visibleOnly = true) {
  const box = new THREE.Box3();
  for (const p of Object.values(state.parts)) {
    if (!visibleOnly || p.group.visible) box.expandByObject(p.front);
  }
  return box;
}
function sceneSize() {
  const b = sceneBox();
  return b.isEmpty() ? 100 : b.getSize(new THREE.Vector3()).length();
}

// ---------- parts ----------
function clipList() { return section.axis ? [clipPlane] : []; }

function makePart(name, key, geom, color, frame) {
  const group = new THREE.Group();
  group.matrixAutoUpdate = false;
  const front = new THREE.Mesh(geom, new THREE.MeshStandardMaterial({
    color, roughness: 0.75, metalness: 0, flatShading: true, side: THREE.FrontSide,
    polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1, clippingPlanes: clipList() }));
  const back = new THREE.Mesh(geom, new THREE.MeshBasicMaterial({
    color: new THREE.Color(color).multiplyScalar(0.45), side: THREE.BackSide, clippingPlanes: clipList() }));
  const edges = new THREE.LineSegments(new THREE.EdgesGeometry(geom, 30),
    new THREE.LineBasicMaterial({ color: 0x1a1a1a, transparent: true, opacity: 0.55, clippingPlanes: clipList() }));
  front.userData.partKey = key;
  group.add(front, back, edges);
  world.add(group);
  return { name, key, group, front, back, edges, color, frame, visible: true };
}

function disposePart(p) {
  world.remove(p.group);
  p.front.geometry.dispose();
  p.edges.geometry.dispose();
  for (const m of [p.front, p.back, p.edges]) m.material.dispose();
}

async function loadParts(view) {
  Object.values(state.parts).forEach(disposePart);
  const visible = Object.fromEntries(Object.values(state.parts).map((p) => [p.key, p.visible]));
  state.parts = {};
  const v = encodeURIComponent(view.built_at);
  let i = 0;
  for (const [name, info] of Object.entries(view.parts)) {
    const geom = await loader.loadAsync(`/out/${info.stl}?v=${v}`);
    state.parts[name] = makePart(name, name, geom, PALETTE[i++ % PALETTE.length], 'part');
  }
  for (const [name, a] of Object.entries(view.assembly || {})) {
    if (!a.stl) continue;
    const geom = await loader.loadAsync(`/out/${a.stl}?v=${v}`);
    const color = state.parts[name]?.color ?? PALETTE[i++ % PALETTE.length];
    state.parts[`${name}@assembly`] = makePart(name, `${name}@assembly`, geom, color, 'assembly');
  }
  for (const p of Object.values(state.parts)) if (p.key in visible) p.visible = visible[p.key];
  applyLayout();
}

function applyLayout() {
  const view = state.view;
  const asm = view?.assembly || {};
  const hasAsm = Object.keys(asm).length > 0;
  if (!hasAsm && state.layout === 'assembly') state.layout = 'print';
  document.querySelector('[data-layout="assembly"]').disabled = !hasAsm;
  document.querySelectorAll('#layouts button').forEach((b) => b.classList.toggle('on', b.dataset.layout === state.layout));
  for (const p of Object.values(state.parts)) {
    const m = new THREE.Matrix4();
    let shown;
    if (state.layout === 'print') {
      shown = p.frame === 'part';
      if (shown) m.makeTranslation(...view.parts[p.name].plate_offset);
    } else if (p.frame === 'assembly') {
      shown = true;
    } else {
      const a = asm[p.name];
      shown = !!(a && a.matrix);
      if (shown) m.set(...a.matrix.flat());
    }
    p.group.matrix.copy(m);
    p.group.matrixWorldNeedsUpdate = true;
    p.group.visible = shown && p.visible;
  }
  scene.updateMatrixWorld(true);
  buildHelpers();
  rebuildMarkers();
  updateSectionRange();
}

function buildHelpers() {
  helpers.clear();
  const lineMat = (color, opacity = 1) => new THREE.LineBasicMaterial({ color, transparent: opacity < 1, opacity });
  if (state.layout === 'print') {
    const nozzles = state.view?.plate?.nozzles || [];
    const main = nozzles[0] || { x: [0, 256], y: [0, 256] };
    const [x0, x1] = main.x, [y0, y1] = main.y;
    const plate = new THREE.Mesh(new THREE.PlaneGeometry(x1 - x0, y1 - y0),
      new THREE.MeshBasicMaterial({ color: 0x9aa0a8, transparent: true, opacity: 0.18, depthWrite: false }));
    plate.position.set((x0 + x1) / 2, (y0 + y1) / 2, -0.05);
    helpers.add(plate);
    const grid = new THREE.GridHelper(x1 - x0, (x1 - x0) / 16, 0x8c9096, 0x8c9096);
    grid.rotation.x = Math.PI / 2;
    grid.position.set((x0 + x1) / 2, (y0 + y1) / 2, -0.04);
    grid.material.transparent = true;
    grid.material.opacity = 0.25;
    helpers.add(grid);
    const outline = (xa, xb, ya, yb, color, opacity) => {
      const g = new THREE.BufferGeometry().setFromPoints([
        new THREE.Vector3(xa, ya, 0), new THREE.Vector3(xb, ya, 0), new THREE.Vector3(xb, yb, 0), new THREE.Vector3(xa, yb, 0)]);
      helpers.add(new THREE.LineLoop(g, lineMat(color, opacity)));
    };
    outline(x0, x1, y0, y1, 0x555a60, 0.9);
    for (const n of nozzles.slice(1)) {
      // Strip of the bed this nozzle can't reach.
      if (n.x[0] > x0) {
        const strip = new THREE.Mesh(new THREE.PlaneGeometry(n.x[0] - x0, y1 - y0),
          new THREE.MeshBasicMaterial({ color: 0xc0392b, transparent: true, opacity: 0.12, depthWrite: false }));
        strip.position.set((x0 + n.x[0]) / 2, (y0 + y1) / 2, -0.03);
        helpers.add(strip);
      }
    }
  } else {
    const b = sceneBox();
    const size = b.isEmpty() ? 100 : Math.ceil(Math.max(b.max.x - b.min.x, b.max.y - b.min.y) * 1.6 / 10) * 10;
    const grid = new THREE.GridHelper(size, size / 10, 0x8c9096, 0x8c9096);
    grid.rotation.x = Math.PI / 2;
    const c = b.isEmpty() ? new THREE.Vector3() : b.getCenter(new THREE.Vector3());
    grid.position.set(c.x, c.y, b.isEmpty() ? 0 : b.min.z - 0.05);
    grid.material.transparent = true;
    grid.material.opacity = 0.25;
    helpers.add(grid);
  }
}

// ---------- camera ----------
const VIEWS = { iso: [1, -1, 0.8], front: [0, -1, 0], right: [1, 0, 0], top: [0, -0.0001, 1] };
function setView(name) {
  const b = sceneBox();
  if (b.isEmpty()) return;
  const center = b.getCenter(new THREE.Vector3());
  const radius = b.getSize(new THREE.Vector3()).length() / 2;
  const dir = name === 'fit'
    ? camera.position.clone().sub(controls.target).normalize()
    : new THREE.Vector3(...VIEWS[name]).normalize();
  const dist = radius / Math.sin(THREE.MathUtils.degToRad(camera.fov / 2)) * 1.05;
  controls.target.copy(center);
  camera.position.copy(center).addScaledVector(dir, dist);
  camera.near = Math.max(dist / 1000, 0.05);
  camera.far = dist * 20;
  camera.updateProjectionMatrix();
  controls.update();
}
const getCamera = () => ({ position: camera.position.toArray(), target: controls.target.toArray(), layout: state.layout });
function goToCamera(c) {
  if (!c) return;
  if (c.layout && c.layout !== state.layout) { state.layout = c.layout; applyLayout(); }
  camera.position.fromArray(c.position);
  controls.target.fromArray(c.target);
  controls.update();
}

// ---------- section ----------
function updateSectionRange() {
  const axis = section.axis;
  $('sec-pos').disabled = !axis;
  $('sec-flip').disabled = !axis;
  for (const p of Object.values(state.parts)) {
    for (const m of [p.front.material, p.back.material, p.edges.material]) {
      m.clippingPlanes = clipList();
      m.needsUpdate = true;
    }
  }
  if (!axis) return;
  const b = sceneBox();
  if (b.isEmpty()) return;
  section.min = b.min[axis];
  section.max = b.max[axis];
  applySection();
}
function applySection() {
  const pos = section.min + (section.max - section.min) * section.t;
  const n = new THREE.Vector3(section.axis === 'x' ? 1 : 0, section.axis === 'y' ? 1 : 0, section.axis === 'z' ? 1 : 0);
  if (section.flip) n.negate();
  clipPlane.normal.copy(n);
  clipPlane.constant = section.flip ? pos : -pos;
}
$('sec-axis').addEventListener('change', (e) => {
  section.axis = e.target.value;
  section.t = 0.5;
  $('sec-pos').value = 500;
  updateSectionRange();
  rebuildMarkers();
});
$('sec-pos').addEventListener('input', (e) => { section.t = e.target.value / 1000; applySection(); });
$('sec-flip').addEventListener('click', () => { section.flip = !section.flip; applySection(); });

// ---------- picking ----------
function ndc(clientX, clientY) {
  const r = canvas.getBoundingClientRect();
  return new THREE.Vector2(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
}
function pick(clientX, clientY) {
  raycaster.setFromCamera(ndc(clientX, clientY), camera);
  const targets = Object.values(state.parts).filter((p) => p.group.visible).map((p) => p.front);
  for (const hit of raycaster.intersectObjects(targets, false)) {
    if (section.axis && clipPlane.distanceToPoint(hit.point) < 0) continue;
    const part = state.parts[hit.object.userData.partKey];
    const local = part.group.worldToLocal(hit.point.clone());
    return { part, world: hit.point.clone(), local, normal: hit.face.normal.clone().normalize() };
  }
  return null;
}
function anchorBase(hit) {
  return {
    part: hit.part.name, point: hit.local.toArray().map(round3), normal: hit.normal.toArray().map(round3),
    frame: hit.part.frame, view: state.layout,
  };
}
const round3 = (x) => Math.round(x * 1000) / 1000;

// ---------- markers & labels ----------
const labels = []; // {el, key, world: () => Vector3|null}
function clearLabels() { labels.forEach((l) => l.el.remove()); labels.length = 0; }
function addLabel(text, cls, worldFn, onClick, key) {
  const el = document.createElement('div');
  el.className = `label ${cls}`;
  el.textContent = text;
  if (onClick) el.addEventListener('click', onClick);
  $('labels').appendChild(el);
  labels.push({ el, key, world: worldFn });
}

function partForAnchor(a) {
  return state.parts[a.frame === 'assembly' ? `${a.part}@assembly` : a.part];
}

// Unit sphere; markers rescale it every frame so dots stay the same size on screen at any zoom.
const DOT = new THREE.SphereGeometry(1, 16, 12);
const DOT_PX = 5;
function scaleDots() {
  const perPx = 2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) / canvas.clientHeight;
  const w = new THREE.Vector3();
  scene.traverseVisible((o) => {
    if (!o.userData.screenDot) return;
    o.getWorldPosition(w);
    const s = camera.position.distanceTo(w) * perPx * DOT_PX * o.userData.screenDot;
    // Undo any scale inherited from the part's transform (rigid, so 1) and apply the screen size.
    o.scale.setScalar(s);
  });
}

function markerObject(a, color, scale) {
  const g = new THREE.Group();
  const matOpts = { color, depthTest: false, transparent: true, clippingPlanes: clipList() };
  const dot = new THREE.Mesh(DOT, new THREE.MeshBasicMaterial(matOpts));
  dot.userData.screenDot = 1;
  dot.position.fromArray(a.point);
  dot.renderOrder = 10;
  g.add(dot);
  if (a.kind === 'circle') {
    const n = new THREE.Vector3().fromArray(a.view_dir || a.normal).normalize();
    const width = Math.max(a.radius * 0.07, scale * 0.0015);
    const ring = new THREE.Mesh(new THREE.RingGeometry(a.radius - width, a.radius, 64),
      new THREE.MeshBasicMaterial({ ...matOpts, side: THREE.DoubleSide, opacity: 0.9 }));
    ring.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), n);
    ring.position.fromArray(a.point);
    ring.renderOrder = 10;
    g.add(ring);
  }
  return g;
}

function rebuildMarkers() {
  for (const p of Object.values(state.parts)) {
    p.group.children.filter((c) => c.userData.marker).forEach((c) => p.group.remove(c));
  }
  measureGroup.clear();
  clearLabels();
  const scale = sceneSize();
  const place = (a, color, text, cls, onClick, key) => {
    if (a.kind === 'measure') return placeMeasure(a, text, cls, onClick, key);
    const part = partForAnchor(a);
    if (!part) return;
    const m = markerObject(a, color, scale);
    m.userData.marker = true;
    m.userData.key = key;
    part.group.add(m);
    addLabel(text, cls, () => part.group.visible ? part.group.localToWorld(new THREE.Vector3().fromArray(a.point)) : null, onClick, key);
  };
  state.threads.forEach((t) => {
    if (!t.anchor || (t.status === 'resolved' && !state.showResolved)) return;
    place(t.anchor, COLORS[t.status], `#${t.id}`, t.status, () => focusCard(`t${t.id}`), `t${t.id}`);
  });
  state.drafts.forEach((d, i) => {
    if (d.anchor) place(d.anchor, COLORS.draft, `new ${i + 1}`, 'draft', () => focusCard(d.key), d.key);
  });
  if (state.measure?.b) {
    placeMeasure(measureAnchor(state.measure), `${state.measure.dist.toFixed(2)} mm`, 'measure');
  } else if (state.measure?.a) {
    const w = state.measure.a.world;
    const dot = new THREE.Mesh(DOT, new THREE.MeshBasicMaterial({ color: COLORS.measure, depthTest: false }));
    dot.userData.screenDot = 0.8;
    dot.position.copy(w);
    dot.renderOrder = 10;
    measureGroup.add(dot);
  }
}

function placeMeasure(a, text, cls, onClick, key) {
  const pa = state.parts[a.frame === 'assembly' ? `${a.part}@assembly` : a.part];
  const pb = state.parts[a.frame_b === 'assembly' ? `${a.part_b}@assembly` : a.part_b];
  if (!pa || !pb) return;
  const wa = () => pa.group.localToWorld(new THREE.Vector3().fromArray(a.point));
  const wb = () => pb.group.localToWorld(new THREE.Vector3().fromArray(a.point_b));
  const line = new THREE.Line(new THREE.BufferGeometry().setFromPoints([wa(), wb()]),
    new THREE.LineBasicMaterial({ color: cls === 'measure' ? COLORS.measure : COLORS[cls] ?? COLORS.draft, depthTest: false }));
  line.renderOrder = 10;
  line.userData.key = key;
  measureGroup.add(line);
  addLabel(text, cls, () => (pa.group.visible && pb.group.visible) ? wa().add(wb()).multiplyScalar(0.5) : null, onClick, key);
}

function measureAnchor(m) {
  return {
    kind: 'measure', part: m.a.part.name, frame: m.a.part.frame, point: m.a.local.toArray().map(round3),
    normal: m.a.normal.toArray().map(round3), part_b: m.b.part.name, frame_b: m.b.part.frame,
    point_b: m.b.local.toArray().map(round3), distance: round3(m.dist),
    delta: m.b.world.clone().sub(m.a.world).toArray().map(round3), view: state.layout,
  };
}

function updateLabels() {
  const r = canvas.getBoundingClientRect();
  for (const l of labels) {
    const w = l.world();
    if (!w) { l.el.style.display = 'none'; continue; }
    if (section.axis && clipPlane.distanceToPoint(w) < 0) { l.el.style.display = 'none'; continue; }
    const p = w.project(camera);
    if (p.z > 1) { l.el.style.display = 'none'; continue; }
    l.el.style.display = '';
    l.el.style.left = `${(p.x + 1) / 2 * r.width}px`;
    l.el.style.top = `${(1 - p.y) / 2 * r.height}px`;
  }
}

// ---------- interaction ----------
let lasso = null;
function setMode(mode) {
  state.mode = mode;
  stage.className = `mode-${mode}`;
  document.querySelectorAll('#modes button').forEach((b) => b.classList.toggle('on', b.dataset.mode === mode));
  cancelGesture();
}
function cancelGesture() {
  lasso = null;
  octx.clearRect(0, 0, overlay.width, overlay.height);
  if (state.measure) { state.measure = null; rebuildMarkers(); }
  setHint();
}
const NAV = 'Right-drag pan · Ctrl+right-drag or middle-drag rotate · wheel zoom';
const HINTS = {
  pin: `Click the model to pin a comment. ${NAV}`,
  circle: `Drag a loop around an area to comment on it. ${NAV}`,
  measure: `Click two points to measure between them. ${NAV}`,
};
function setHint(html) { $('hint').innerHTML = html ?? HINTS[state.mode]; $('hint').style.pointerEvents = html ? 'auto' : 'none'; }

let down = null;
stage.addEventListener('contextmenu', (e) => e.preventDefault());
canvas.addEventListener('pointerdown', (e) => {
  down = { x: e.clientX, y: e.clientY, button: e.button, moved: 0 };
  if (state.mode === 'circle' && e.button === 0) {
    lasso = [[e.clientX, e.clientY]];
    canvas.setPointerCapture(e.pointerId);
  }
});
canvas.addEventListener('pointermove', (e) => {
  if (down) down.moved = Math.max(down.moved, Math.hypot(e.clientX - down.x, e.clientY - down.y));
  if (lasso) { lasso.push([e.clientX, e.clientY]); drawLasso(); return; }
  hoverAt(e.clientX, e.clientY);
});
canvas.addEventListener('pointerup', (e) => {
  const moved = down ? down.moved : 99;
  const wasLeft = down?.button === 0;
  down = null;
  if (lasso) { finishLasso(); return; }
  if (!wasLeft || moved > 4) return;
  if (state.mode === 'pin') {
    const hit = pick(e.clientX, e.clientY);
    if (hit) createDraft({ kind: 'point', ...anchorBase(hit) });
  } else if (state.mode === 'measure') {
    measureClick(e.clientX, e.clientY);
  }
});

let hoverQueued = false;
function hoverAt(x, y) {
  if (hoverQueued) return;
  hoverQueued = true;
  requestAnimationFrame(() => {
    hoverQueued = false;
    const hit = pick(x, y);
    $('hover').textContent = hit ? `${hit.part.name}  ${fmt(hit.local.toArray())} mm  (part frame)` : '';
  });
}

function drawLasso(path = lasso, ctx = octx, scale = devicePixelRatio, offset = canvas.getBoundingClientRect()) {
  if (ctx === octx) ctx.clearRect(0, 0, overlay.width, overlay.height);
  if (!path || path.length < 2) return;
  ctx.save();
  ctx.strokeStyle = '#d9731a';
  ctx.lineWidth = 2.5 * scale;
  ctx.setLineDash([6 * scale, 4 * scale]);
  ctx.beginPath();
  path.forEach(([x, y], i) => {
    const px = (x - offset.left) * scale, py = (y - offset.top) * scale;
    i ? ctx.lineTo(px, py) : ctx.moveTo(px, py);
  });
  ctx.closePath();
  ctx.stroke();
  ctx.restore();
}

function inPolygon(x, y, poly) {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

function finishLasso() {
  const path = lasso;
  lasso = null;
  octx.clearRect(0, 0, overlay.width, overlay.height);
  if (path.length < 5) return;
  const xs = path.map((p) => p[0]), ys = path.map((p) => p[1]);
  const cx = xs.reduce((a, b) => a + b) / xs.length, cy = ys.reduce((a, b) => a + b) / ys.length;
  const minX = Math.min(...xs), maxX = Math.max(...xs), minY = Math.min(...ys), maxY = Math.max(...ys);
  const step = Math.max(3, Math.sqrt(((maxX - minX) * (maxY - minY)) / 500));
  const hits = [];
  for (let x = minX; x <= maxX; x += step) {
    for (let y = minY; y <= maxY; y += step) {
      if (!inPolygon(x, y, path)) continue;
      const h = pick(x, y);
      if (h) hits.push({ h, d: Math.hypot(x - cx, y - cy) });
    }
  }
  if (!hits.length) { flash('That circle missed the model.'); return; }
  const center = pick(cx, cy) || hits.sort((a, b) => a.d - b.d)[0].h;
  const screenR = path.reduce((s, [x, y]) => s + Math.hypot(x - cx, y - cy), 0) / path.length;
  const dist = camera.position.distanceTo(center.world);
  const worldPerPx = (2 * dist * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) / canvas.clientHeight;
  const viewDir = camera.position.clone().sub(center.world).normalize();
  const inv = new THREE.Matrix3().setFromMatrix4(center.part.group.matrixWorld).invert();
  const parts = [...new Set(hits.map((x) => x.h.part.name))];
  createDraft({
    kind: 'circle', ...anchorBase(center), radius: round3(screenR * worldPerPx),
    view_dir: viewDir.applyMatrix3(inv).normalize().toArray().map(round3), parts,
  }, path);
}

function measureClick(x, y) {
  const hit = pick(x, y);
  if (!hit) return;
  if (!state.measure || state.measure.b) {
    state.measure = { a: hit };
    setHint('Click a second point.');
  } else {
    state.measure.b = hit;
    state.measure.dist = hit.world.distanceTo(state.measure.a.world);
    const d = hit.world.clone().sub(state.measure.a.world);
    setHint(`<b>${state.measure.dist.toFixed(2)} mm</b> &nbsp;dx ${d.x.toFixed(2)} dy ${d.y.toFixed(2)} dz ${d.z.toFixed(2)} (${state.layout} frame) &nbsp;<button id="measure-comment" class="link">Comment on this</button>`);
    $('measure-comment').addEventListener('click', () => {
      const a = measureAnchor(state.measure);
      state.measure = null;
      setHint();
      createDraft(a);
    });
  }
  rebuildMarkers();
}

window.addEventListener('keydown', (e) => {
  if (e.target.matches('textarea, input, select')) {
    if (e.key === 'Escape') e.target.blur();
    return;
  }
  const k = e.key.toLowerCase();
  const tool = { 1: 'pin', 2: 'circle', 3: 'measure' }[k];
  if (tool) setMode(tool);
  else if (k === 'f') setView('fit');
  else if (k === 'escape') cancelGesture();
});
document.querySelectorAll('#modes button').forEach((b) => b.addEventListener('click', () => setMode(b.dataset.mode)));
document.querySelectorAll('#layouts button').forEach((b) => b.addEventListener('click', () => {
  state.layout = b.dataset.layout;
  applyLayout();
  setView('iso');
}));
document.querySelectorAll('#cams button').forEach((b) => b.addEventListener('click', () => setView(b.dataset.cam)));

// ---------- drafts, screenshots ----------
function snapshot(path, key) {
  // Show only this comment's marker, labelled "this", so the image can't be confused with thread numbers.
  const others = [];
  scene.traverse((o) => { if ((o.userData.marker || o.parent === measureGroup) && o.userData.key !== key && o.visible) { o.visible = false; others.push(o); } });
  scaleDots();
  renderer.render(scene, camera);
  others.forEach((o) => { o.visible = true; });
  updateLabels();
  const scale = Math.min(1, 1400 / canvas.width);
  const c = document.createElement('canvas');
  c.width = Math.round(canvas.width * scale);
  c.height = Math.round(canvas.height * scale);
  const ctx = c.getContext('2d');
  ctx.fillStyle = getComputedStyle(stage).backgroundColor;
  ctx.fillRect(0, 0, c.width, c.height);
  ctx.drawImage(canvas, 0, 0, c.width, c.height);
  const px = (canvas.width / canvas.clientWidth) * scale;
  const rect = canvas.getBoundingClientRect();
  if (path) drawLasso(path, ctx, px, rect);
  ctx.font = `600 ${12 * px}px system-ui, sans-serif`;
  for (const l of labels) {
    if (l.el.style.display === 'none' || l.key !== key) continue;
    const text = l.el.classList.contains('measure') || /mm$/.test(l.el.textContent) ? l.el.textContent : 'this';
    const x = parseFloat(l.el.style.left) * px, y = parseFloat(l.el.style.top) * px;
    const w = ctx.measureText(text).width + 10 * px, h = 16 * px;
    ctx.fillStyle = getComputedStyle(l.el).backgroundColor;
    ctx.beginPath();
    ctx.roundRect(x - w / 2, y - h * 1.3, w, h, 8 * px);
    ctx.fill();
    ctx.fillStyle = '#fff';
    ctx.fillText(text, x - w / 2 + 5 * px, y - h * 1.3 + 12 * px);
  }
  return c.toDataURL('image/png');
}

function createDraft(anchor, lassoPath) {
  const d = { key: `d${Date.now()}`, anchor, camera: getCamera(), text: '' };
  state.drafts.push(d);
  rebuildMarkers();
  d.screenshot = snapshot(lassoPath, d.key);
  persist();
  renderComments();
  focusCard(d.key, true);
}
$('add-general').addEventListener('click', () => createDraft(null));

function anchorText(a) {
  if (!a) return 'general comment on this view';
  if (a.kind === 'measure') return `${a.part} → ${a.part_b}: ${a.distance.toFixed(2)} mm`;
  const where = `${a.part} ${fmt(a.point)}`;
  return a.kind === 'circle' ? `circle r≈${a.radius.toFixed(1)} mm on ${where}` : `point on ${where}`;
}

function focusCard(key, edit) {
  state.focus = key;
  document.querySelectorAll('.card').forEach((c) => c.classList.toggle('focus', c.dataset.key === key));
  const card = document.querySelector(`.card[data-key="${key}"]`);
  if (card) {
    card.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    if (edit) card.querySelector('textarea')?.focus();
  }
}

function renderComments() {
  const drafts = $('drafts');
  drafts.innerHTML = state.drafts.map((d, i) => `
    <div class="card draft" data-key="${d.key}">
      <div class="head"><span class="num">new ${i + 1}</span><span class="where" title="${esc(anchorText(d.anchor))}">${esc(anchorText(d.anchor))}</span></div>
      <textarea placeholder="What should change here?">${esc(d.text)}</textarea>
      <div class="actions"><button data-act="goto">View</button><button data-act="delete">Delete</button></div>
    </div>`).join('');
  drafts.querySelectorAll('.card').forEach((card) => {
    const d = state.drafts.find((x) => x.key === card.dataset.key);
    card.querySelector('textarea').addEventListener('input', (e) => { d.text = e.target.value; persist(); updateSend(); });
    card.querySelector('[data-act="goto"]').addEventListener('click', () => goToCamera(d.camera));
    card.querySelector('[data-act="delete"]').addEventListener('click', () => {
      state.drafts = state.drafts.filter((x) => x !== d);
      persist(); rebuildMarkers(); renderComments();
    });
  });

  const shown = state.threads.filter((t) => t.status !== 'resolved' || state.showResolved).slice().reverse();
  const threads = $('threads');
  threads.innerHTML = shown.map((t) => {
    const r = state.replies[t.id] || {};
    const status = r.status || t.status;
    return `
    <div class="card thread ${t.status}" data-key="t${t.id}" data-id="${t.id}">
      <div class="head"><span class="num">#${t.id}</span><span class="where">${esc(anchorText(t.anchor))}</span><span class="muted small">${esc(status)}${r.status ? ' (unsent)' : ''}</span></div>
      ${t.messages.map((m) => `<div class="msg ${m.by}"><b>${m.by === 'claude' ? 'Claude' : 'You'}:</b> ${esc(m.text)}</div>`).join('')}
      ${t.screenshot ? `<div class="small"><a href="/review/${esc(t.screenshot)}" target="_blank">screenshot</a></div>` : ''}
      <textarea placeholder="Reply…">${esc(r.text || '')}</textarea>
      <div class="actions">
        ${t.camera ? '<button data-act="goto">View</button>' : ''}
        <button data-act="status">${status === 'resolved' ? 'Reopen' : 'Resolve'}</button>
      </div>
    </div>`;
  }).join('') || (state.threads.length || state.drafts.length ? '' : '<div class="muted small">No comments yet. Pin or circle something on the model.</div>');
  threads.querySelectorAll('.card').forEach((card) => {
    const t = state.threads.find((x) => x.id === Number(card.dataset.id));
    const r = () => (state.replies[t.id] ||= {});
    card.querySelector('textarea').addEventListener('input', (e) => { r().text = e.target.value; persist(); updateSend(); });
    card.querySelector('[data-act="goto"]')?.addEventListener('click', () => goToCamera(t.camera));
    card.querySelector('[data-act="status"]').addEventListener('click', () => {
      const cur = r().status || t.status;
      r().status = cur === 'resolved' ? 'open' : 'resolved';
      if (r().status === t.status) delete r().status;
      persist(); renderComments();
    });
  });
  if (state.focus) document.querySelector(`.card[data-key="${state.focus}"]`)?.classList.add('focus');
  updateSend();
}

function pendingReplies() {
  return Object.entries(state.replies)
    .filter(([, r]) => (r.text || '').trim() || r.status)
    .map(([id, r]) => ({ id: Number(id), text: r.text || '', status: r.status }));
}
function updateSend() {
  const n = state.drafts.filter((d) => d.text.trim()).length;
  const m = pendingReplies().length;
  const empty = state.drafts.length - n;
  $('send').disabled = !(n + m) || !state.connected;
  $('send').textContent = n + m ? `Send to Claude (${[n && `${n} new`, m && `${m} repl${m > 1 ? 'ies' : 'y'}`].filter(Boolean).join(', ')})` : 'Send to Claude';
  $('send-note').textContent = !state.connected ? 'Review server is not running.'
    : empty ? `${empty} empty draft${empty > 1 ? 's' : ''} will be skipped.` : '';
}

$('send').addEventListener('click', async () => {
  const payload = {
    new: state.drafts.filter((d) => d.text.trim()).map(({ anchor, camera: cam, text, screenshot }) => ({ anchor, camera: cam, text, screenshot })),
    replies: pendingReplies(),
  };
  $('send').disabled = true;
  try {
    const res = await fetch('/api/send', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
    const j = await res.json();
    state.drafts = state.drafts.filter((d) => !d.text.trim());
    state.replies = {};
    state.threads = j.threads;
    state.version = j.version;
    state.sentAndClosed = j.closing;
    persist();
    rebuildMarkers();
    renderComments();
    showBanner(j.closing ? SENT_CLOSED : 'Sent to Claude.', !j.closing);
  } catch (err) {
    flash(`Send failed: ${err}`);
    updateSend();
  }
});

// ---------- sidebar: header, parts, report ----------
function renderHeader() {
  const v = state.view;
  $('project').textContent = state.project || '';
  document.title = `${state.project} · CAD Review`;
  $('badge').textContent = v?.status || 'not built';
  $('badge').className = `badge ${v?.status || ''}`;
  $('buildinfo').textContent = v ? `built ${v.built_at} · ${v.commit || 'no commit'}${v.dirty ? ' (uncommitted changes)' : ''}` : 'Run `uv run cad build` first.';
  $('parts').innerHTML = Object.values(state.parts).filter((p) => p.frame === 'part' || !state.parts[p.name]).map((p) => {
    const dims = v.parts[p.name]?.bbox;
    return `<li><input type="checkbox" data-key="${esc(p.key)}" ${p.visible ? 'checked' : ''}>
      <span class="swatch" style="background:#${p.color.toString(16).padStart(6, '0')}"></span>${esc(p.name)}
      <span class="dims">${dims ? dims.map((x) => x.toFixed(1)).join(' × ') + ' mm' : ''}</span></li>`;
  }).join('');
  $('parts').querySelectorAll('input').forEach((cb) => cb.addEventListener('change', () => {
    for (const p of Object.values(state.parts)) if (p.name === state.parts[cb.dataset.key].name) p.visible = cb.checked;
    applyLayout();
  }));
  const items = [];
  for (const [name, r] of Object.entries(v?.report || {})) {
    for (const f of r.findings) items.push(`<li><span class="st ${f.status}">${f.status}</span>${esc(name)} · ${esc(f.check)}: ${esc(f.detail)}</li>`);
  }
  for (const f of v?.spec || []) if (f.status !== 'PASS') items.push(`<li><span class="st ${f.status}">${f.status}</span>${esc(f.detail)}</li>`);
  $('report').innerHTML = items.join('') || '<li class="muted">All checks pass.</li>';
}

// ---------- state, persistence, polling ----------
const storeKey = () => `cad-review:${state.project}`;
function persist() {
  try { localStorage.setItem(storeKey(), JSON.stringify({ drafts: state.drafts, replies: state.replies })); } catch { /* storage full or blocked */ }
}
function restore() {
  try {
    const s = JSON.parse(localStorage.getItem(storeKey()) || 'null');
    if (s) { state.drafts = s.drafts || []; state.replies = s.replies || {}; }
  } catch { /* ignore */ }
}

const SENT_CLOSED = 'Sent to Claude. This page reconnects when Claude reopens the review.';
let bannerTimer = null;
function showBanner(text, autoHide) {
  const b = $('banner');
  b.textContent = text;
  b.hidden = false;
  clearTimeout(bannerTimer);
  if (autoHide) bannerTimer = setTimeout(() => { b.hidden = true; }, 3500);
}
const flash = (text) => showBanner(text, true);

let loading = null;
function loadState() {
  loading ||= doLoadState().finally(() => { loading = null; });
  return loading;
}
async function doLoadState() {
  const s = await (await fetch('/api/state', { cache: 'no-store' })).json();
  const first = state.project === null;
  if (first) { state.project = s.project; restore(); }
  state.threads = s.threads;
  state.version = s.version;
  const rebuilt = s.view && s.view.built_at !== state.builtAt;
  state.view = s.view;
  if (rebuilt) {
    const hadModel = state.builtAt !== null;
    state.builtAt = s.view.built_at;
    await loadParts(s.view);
    if (first || !hadModel) {
      if (!(first && restoreView())) setView('iso');
    } else {
      flash('Model rebuilt.');
    }
  } else {
    rebuildMarkers();
  }
  renderHeader();
  renderComments();
}

async function poll() {
  try {
    const j = await (await fetch('/api/version', { cache: 'no-store' })).json();
    if (state.project && j.project !== state.project) { location.reload(); return; }
    if (state.app && j.app !== state.app) { reloadKeepingView(); return; }
    state.app = j.app;
    if (!state.connected) { state.connected = true; state.sentAndClosed = false; $('banner').hidden = true; updateSend(); }
    if (j.version !== state.version) await loadState();
  } catch {
    if (state.connected) {
      state.connected = false;
      showBanner(state.sentAndClosed ? SENT_CLOSED
        : 'Review server stopped. Comments you write are kept and can be sent when Claude reopens the review.');
      updateSend();
    }
  }
  setTimeout(poll, 2000);
}

$('show-resolved').addEventListener('change', (e) => { state.showResolved = e.target.checked; rebuildMarkers(); renderComments(); });

// The app's own code changed (e.g. Claude updated the viewer): reload, keeping the camera.
function reloadKeepingView() {
  try { sessionStorage.setItem('cad-review:camera', JSON.stringify(getCamera())); } catch { /* ignore */ }
  location.reload();
}
function restoreView() {
  try {
    const c = JSON.parse(sessionStorage.getItem('cad-review:camera') || 'null');
    sessionStorage.removeItem('cad-review:camera');
    if (c) { goToCamera(c); return true; }
  } catch { /* ignore */ }
  return false;
}

function animate() {
  requestAnimationFrame(animate);
  scaleDots();
  renderer.render(scene, camera);
  updateLabels();
}

resize();
setMode('pin');
loadState().then(() => {
  if (!state.view) showBanner('No build yet. Run `uv run cad build` for this project.');
}).catch((e) => showBanner(`Failed to load: ${e}`));
poll();
animate();
