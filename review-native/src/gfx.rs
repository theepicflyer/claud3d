//! The compositor: renders the parts into an offscreen MSAA target with a depth buffer, then
//! paints that image at the viewport's effect slot inside Rune's own frame, so the floating
//! panels drawn after it blur and cover it like any other content.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use rune::compose::DeviceKey;
use rune::{Compositor, Frame};

use crate::app::{App, Layout, MARGIN, SIDEBAR_W, Shared, Shot};
use crate::data::Scene;

pub const SLOT: u32 = 1;
const SAMPLES: u32 = 4;
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SHOT_MAX_W: u32 = 1400;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    plane: [f32; 4],
    view: [f32; 4],
    light: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct Obj {
    model: [[f32; 4]; 4],
    color: [f32; 4],
    extra: [f32; 4],
    rect: [f32; 4],
    strip: [f32; 4],
}

struct Slot {
    buf: wgpu::Buffer,
    bg: wgpu::BindGroup,
}

struct GpuPart {
    mesh: wgpu::Buffer,
    mesh_n: u32,
    edges: wgpu::Buffer,
    edges_n: u32,
    slot: Slot,
}

struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve: wgpu::TextureView,
    blit_bg: wgpu::BindGroup,
}

struct Pending {
    id: u64,
    buf: wgpu::Buffer,
    size: (u32, u32),
    row: u32,
}

struct Res {
    key: DeviceKey,
    obj_bgl: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    mesh: wgpu::RenderPipeline,
    edge: wgpu::RenderPipeline,
    bed: wgpu::RenderPipeline,
    mark: wgpu::RenderPipeline,
    blit_layout: wgpu::PipelineLayout,
    blit_bgl: wgpu::BindGroupLayout,
    blit_mod: wgpu::ShaderModule,
    blits: HashMap<(wgpu::TextureFormat, bool), wgpu::RenderPipeline>,
    shot: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bg_color: wgpu::Buffer,
    generation: u64,
    parts: Vec<GpuPart>,
    beds: Vec<Slot>,
    marks: Vec<Slot>,
    targets: Option<Targets>,
}

pub struct Gfx {
    res: Option<Res>,
    pending: Option<Pending>,
    taken: u64,
    shared: Arc<Mutex<Shared>>,
}

impl Gfx {
    pub fn new(shared: Arc<Mutex<Shared>>) -> Gfx {
        Gfx { res: None, pending: None, taken: 0, shared }
    }
}

fn mat(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

fn uniform(device: &wgpu::Device, size: u64, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn vertex_buffer(device: &wgpu::Device, data: &[f32], label: &str) -> wgpu::Buffer {
    let bytes: &[u8] = bytemuck::cast_slice(data);
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (bytes.len() as u64).max(16),
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    if let Ok(mut view) = buf.slice(..).get_mapped_range_mut() {
        view.slice(..bytes.len()).copy_from_slice(bytes);
    }
    buf.unmap();
    buf
}

fn premul_blend() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
    }
}

impl Res {
    fn new(device: &wgpu::Device, key: DeviceKey) -> Res {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("scene"), source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()) });
        let blit_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("blit"), source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()) });
        let ubo = |binding: u32, vis: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vis,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let vf = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let globals_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("globals"), entries: &[ubo(0, vf)] });
        let obj_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("obj"), entries: &[ubo(0, vf)] });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("scene"), bind_group_layouts: &[Some(&globals_bgl), Some(&obj_bgl)], immediate_size: 0 });
        let globals = uniform(device, std::mem::size_of::<Globals>() as u64, "globals");
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let ms = wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: false };
        let depth = |write: bool, cmp: wgpu::CompareFunction| {
            Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(write), depth_compare: Some(cmp), stencil: Default::default(), bias: Default::default() })
        };
        let pipe = |label: &str, vs: &str, fs: &str, buffers: &[Option<wgpu::VertexBufferLayout>], blend: Option<wgpu::BlendState>, ds: Option<wgpu::DepthStencilState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &module, entry_point: Some(vs), compilation_options: Default::default(), buffers },
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: ds,
                multisample: ms,
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: COLOR, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let mesh_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let edge_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        let mesh = pipe(
            "mesh",
            "vs_mesh",
            "fs_mesh",
            &[Some(wgpu::VertexBufferLayout { array_stride: 24, step_mode: wgpu::VertexStepMode::Vertex, attributes: &mesh_attrs })],
            None,
            depth(true, wgpu::CompareFunction::Less),
        );
        let edge = pipe(
            "edges",
            "vs_edge",
            "fs_edge",
            &[Some(wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &edge_attrs })],
            Some(premul_blend()),
            depth(false, wgpu::CompareFunction::LessEqual),
        );
        let bed = pipe("bed", "vs_bed", "fs_bed", &[], Some(premul_blend()), depth(false, wgpu::CompareFunction::LessEqual));
        let mark = pipe("mark", "vs_mark", "fs_mark", &[], Some(premul_blend()), depth(false, wgpu::CompareFunction::Always));
        let tex = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[
                tex(0),
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                ubo(2, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let blit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("blit"), bind_group_layouts: &[Some(&blit_bgl)], immediate_size: 0 });
        let shot = blit_pipeline(device, &blit_layout, &blit_mod, wgpu::TextureFormat::Rgba8Unorm, "fs_shot", None);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("blit"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let bg_color = uniform(device, 32, "blit");
        Res {
            key,
            obj_bgl,
            globals,
            globals_bg,
            mesh,
            edge,
            bed,
            mark,
            blit_layout,
            blit_bgl,
            blit_mod,
            blits: HashMap::new(),
            shot,
            sampler,
            bg_color,
            generation: 0,
            parts: Vec::new(),
            beds: Vec::new(),
            marks: Vec::new(),
            targets: None,
        }
    }

    fn slot(&self, device: &wgpu::Device) -> Slot {
        let buf = uniform(device, std::mem::size_of::<Obj>() as u64, "obj");
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("obj"),
            layout: &self.obj_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
        });
        Slot { buf, bg }
    }

    fn sync_scene(&mut self, device: &wgpu::Device, scene: &Scene) {
        if self.generation == scene.generation {
            return;
        }
        self.generation = scene.generation;
        self.parts = scene
            .parts
            .iter()
            .map(|p| {
                let mut v = Vec::with_capacity(p.mesh.tris.len() * 18);
                for (t, n) in p.mesh.tris.iter().zip(&p.mesh.normals) {
                    for q in t {
                        v.extend_from_slice(&[q.x, q.y, q.z, n.x, n.y, n.z]);
                    }
                }
                let mut e = Vec::with_capacity(p.edges.len() * 48);
                for [a, b] in &p.edges {
                    for (cx, cy) in [(0.0, -1.0), (1.0, -1.0), (1.0, 1.0), (0.0, -1.0), (1.0, 1.0), (0.0, 1.0)] {
                        e.extend_from_slice(&[a.x, a.y, a.z, b.x, b.y, b.z, cx, cy]);
                    }
                }
                GpuPart {
                    mesh: vertex_buffer(device, &v, "mesh"),
                    mesh_n: (p.mesh.tris.len() * 3) as u32,
                    edges: vertex_buffer(device, &e, "edges"),
                    edges_n: (p.edges.len() * 6) as u32,
                    slot: self.slot(device),
                }
            })
            .collect();
        if self.beds.is_empty() {
            self.beds = vec![self.slot(device), self.slot(device)];
            self.marks = (0..2).map(|_| self.slot(device)).collect();
        }
    }

    fn sync_targets(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let tex = |label: &str, format: wgpu::TextureFormat, samples: u32, usage: wgpu::TextureUsages| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attach = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let resolve = tex("resolve", COLOR, 1, attach | wgpu::TextureUsages::TEXTURE_BINDING);
        let blit_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit"),
            layout: &self.blit_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&resolve) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: self.bg_color.as_entire_binding() },
            ],
        });
        self.targets = Some(Targets {
            size,
            color: tex("color", COLOR, SAMPLES, attach),
            depth: tex("depth", DEPTH, SAMPLES, attach),
            resolve,
            blit_bg,
        });
    }

    fn blit_for(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, srgb: bool) {
        if !self.blits.contains_key(&(format, srgb)) {
            let p = blit_pipeline(device, &self.blit_layout, &self.blit_mod, format, if srgb { "fs_blit_srgb" } else { "fs_blit_linear" }, Some(premul_blend()));
            self.blits.insert((format, srgb), p);
        }
    }
}

fn blit_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, format: wgpu::TextureFormat, fs: &str, blend: Option<wgpu::BlendState>) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fs),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: Some("vs_blit"), compilation_options: Default::default(), buffers: &[] },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Everything the 3D pass needs from the app, read before Rune's renderer borrows the runtime.
struct Plan {
    size: (u32, u32),
    globals: Globals,
    parts: Vec<(usize, Obj)>,
    beds: [Obj; 2],
    marks: Vec<Obj>,
    shot: Option<u64>,
    /// Background colour, then the screenshot's share of the width.
    blit: [f32; 8],
}

pub fn background(dark: bool) -> ([f32; 3], [f32; 3]) {
    if dark { ([0.020, 0.022, 0.027], [0.0065, 0.007, 0.009]) } else { ([0.80, 0.83, 0.87], [0.62, 0.65, 0.70]) }
}

fn plan(s: &App, scene: &Scene, rect: [f32; 4], scale: f32, dark: bool, shot_taken: u64) -> Option<Plan> {
    let size = ((rect[2] * scale).round() as u32, (rect[3] * scale).round() as u32);
    if size.0 < 2 || size.1 < 2 {
        return None;
    }
    let lens = s.lens();
    let cam = s.cam.now;
    let eye = cam.eye();
    let (right, up) = cam.basis();
    let light = (eye - cam.target).normalize() + right * -0.45 + up * 0.75;
    let plane = s.plane().map_or([0.0; 4], |(n, d)| [n.x, n.y, n.z, d]);
    let globals = Globals {
        view_proj: mat(cam.view_proj(&lens)),
        eye: [eye.x, eye.y, eye.z, 1.0],
        plane,
        view: [size.0 as f32, size.1 as f32, 1.3 * scale, if dark { 1.0 } else { 0.0 }],
        light: [light.x, light.y, light.z, 0.0],
    };
    let hot = s.hover.as_ref().map(|h| h.part).filter(|_| s.mode == crate::app::Mode::Navigate);
    let parts = s
        .posed()
        .into_iter()
        .filter(|x| x.3 > 0.01)
        .map(|(i, p, m, a)| {
            let edge = if p.printed { 0.55 } else { 0.25 };
            (i, Obj { model: mat(m), color: [p.color[0], p.color[1], p.color[2], a], extra: [if hot == Some(i) { 0.35 } else { 0.0 }, edge, 0.0, 0.0], ..Default::default() })
        })
        .collect();
    let t = s.morph.x.clamp(0.0, 1.0);
    let (x0, x1, y0, y1) = scene.nozzles.first().map_or((0.0, 256.0, 0.0, 256.0), |n| (n.x[0], n.x[1], n.y[0], n.y[1]));
    let strip = scene.nozzles.iter().skip(1).find(|n| n.x[0] > x0).map_or([0.0; 4], |n| [x0, y0, n.x[0], y1]);
    let plate = Obj {
        model: mat(Mat4::from_translation(Vec3::new(0.0, 0.0, -0.05))),
        color: [1.0, 1.0, 1.0, 1.0 - t],
        rect: [x0, y0, x1, y1],
        strip,
        ..Default::default()
    };
    let (gmin, gmax) = {
        let parts: Vec<&crate::data::Part> = scene.parts.iter().collect();
        crate::camera::bounds(&parts, 1.0, s.explode.x, s.asm_center).unwrap_or((Vec3::ZERO, Vec3::ZERO))
    };
    let c = (gmin + gmax) * 0.5;
    let half = (((gmax - gmin).truncate().max_element() * 1.4) / 10.0).ceil() * 10.0 + 20.0;
    let ground = Obj {
        model: mat(Mat4::from_translation(Vec3::new(0.0, 0.0, gmin.z - 0.05))),
        color: [1.0, 1.0, 1.0, if s.layout == Layout::Assembly || t > 0.0 { t } else { 0.0 }],
        extra: [0.0, 0.0, 0.0, 1.0],
        rect: [c.x - half, c.y - half, c.x + half, c.y + half],
        ..Default::default()
    };
    let shot = s.shot_req.as_ref().filter(|r| r.id > shot_taken);
    let marks = shot.map_or_else(Vec::new, |r| {
        r.marks
            .iter()
            .take(2)
            .map(|w| Obj { model: mat(Mat4::from_translation(*w)), color: [0.85, 0.30, 0.04, 1.0], extra: [0.0, 0.0, 11.0 * scale, 0.0], ..Default::default() })
            .collect()
    });
    let (top, _) = background(dark);
    let crop = ((lens.w - SIDEBAR_W - MARGIN * 2.0) / lens.w).clamp(0.3, 1.0);
    Some(Plan { size, globals, parts, beds: [plate, ground], marks, shot: shot.map(|r| r.id), blit: [top[0], top[1], top[2], 1.0, crop, 1.0, 0.0, 0.0] })
}

impl Gfx {
    fn finish_shot(&mut self, device: &wgpu::Device) {
        let Some(p) = self.pending.take() else { return };
        let slice = p.buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        if !matches!(rx.recv(), Ok(Ok(()))) {
            return;
        }
        let (w, h) = p.size;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        if let Ok(data) = slice.get_mapped_range() {
            for y in 0..h {
                let row = &data[(y * p.row) as usize..(y * p.row + w * 4) as usize];
                rgba.extend_from_slice(row);
            }
        }
        p.buf.unmap();
        let shared = self.shared.clone();
        let id = p.id;
        std::thread::spawn(move || {
            let (sw, sh, small) = downscale(&rgba, w, h, SHOT_MAX_W);
            let mut png_bytes = Vec::new();
            {
                let mut enc = png::Encoder::new(&mut png_bytes, sw, sh);
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                if let Ok(mut wr) = enc.write_header() {
                    let _ = wr.write_image_data(&small);
                }
            }
            let data_url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png_bytes));
            let pixels = rune::Pixels::new();
            pixels.set(sw, sh, small);
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.shots.push((id, Shot { data_url, pixels, size: (sw, sh) }));
            s.wake();
        });
    }
}

fn downscale(rgba: &[u8], w: u32, h: u32, max_w: u32) -> (u32, u32, Vec<u8>) {
    if w <= max_w {
        return (w, h, rgba.to_vec());
    }
    let k = w.div_ceil(max_w);
    let (sw, sh) = (w / k, h / k);
    let mut out = Vec::with_capacity((sw * sh * 4) as usize);
    for y in 0..sh {
        for x in 0..sw {
            let mut acc = [0u32; 4];
            for dy in 0..k {
                for dx in 0..k {
                    let i = (((y * k + dy) * w + x * k + dx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += rgba[i + c] as u32;
                    }
                }
            }
            out.extend(acc.map(|a| (a / (k * k)) as u8));
        }
    }
    (sw, sh, out)
}

impl Compositor<App> for Gfx {
    fn animating(&self) -> bool {
        self.pending.is_some()
    }

    fn render(&mut self, f: &mut Frame<'_, App>) {
        let key = f.device_key();
        if self.res.as_ref().is_none_or(|r| r.key != key) {
            self.res = Some(Res::new(f.device, key));
            self.pending = None;
        }
        self.finish_shot(f.device);
        let scale = f.rt.env.scale;
        let dark = f.rt.env.system_dark;
        let srgb = f.srgb_blend();
        let rect = f.rt.presented("viewport").map(|(r, _)| r);
        let scene = f.rt.state.scene.clone();
        let plan = match (rect, &scene) {
            (Some(r), Some(sc)) => plan(&f.rt.state, sc, r, scale, dark, self.taken),
            _ => None,
        };
        let Some(res) = self.res.as_mut() else { return };
        res.blit_for(f.device, f.format, srgb);
        let mut drawn = false;
        if let (Some(p), Some(sc)) = (plan, scene) {
            res.sync_scene(f.device, &sc);
            res.sync_targets(f.device, p.size);
            f.queue.write_buffer(&res.globals, 0, bytemuck::bytes_of(&p.globals));
            f.queue.write_buffer(&res.bg_color, 0, bytemuck::cast_slice(&p.blit));
            for (i, o) in &p.parts {
                f.queue.write_buffer(&res.parts[*i].slot.buf, 0, bytemuck::bytes_of(o));
            }
            for (slot, o) in res.beds.iter().zip(&p.beds) {
                f.queue.write_buffer(&slot.buf, 0, bytemuck::bytes_of(o));
            }
            for (slot, o) in res.marks.iter().zip(&p.marks) {
                f.queue.write_buffer(&slot.buf, 0, bytemuck::bytes_of(o));
            }
            let Some(tg) = res.targets.as_ref() else { return };
            {
                let mut pass = f.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("scene"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &tg.color,
                        depth_slice: None,
                        resolve_target: Some(&tg.resolve),
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Discard },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &tg.depth,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, &res.globals_bg, &[]);
                pass.set_pipeline(&res.bed);
                for (slot, o) in res.beds.iter().zip(&p.beds) {
                    if o.color[3] > 0.001 {
                        pass.set_bind_group(1, &slot.bg, &[]);
                        pass.draw(0..6, 0..1);
                    }
                }
                pass.set_pipeline(&res.mesh);
                for (i, _) in &p.parts {
                    let gp = &res.parts[*i];
                    pass.set_bind_group(1, &gp.slot.bg, &[]);
                    pass.set_vertex_buffer(0, gp.mesh.slice(..));
                    pass.draw(0..gp.mesh_n, 0..1);
                }
                pass.set_pipeline(&res.edge);
                for (i, _) in &p.parts {
                    let gp = &res.parts[*i];
                    if gp.edges_n > 0 {
                        pass.set_bind_group(1, &gp.slot.bg, &[]);
                        pass.set_vertex_buffer(0, gp.edges.slice(..));
                        pass.draw(0..gp.edges_n, 0..1);
                    }
                }
                if !p.marks.is_empty() {
                    pass.set_pipeline(&res.mark);
                    for slot in res.marks.iter().take(p.marks.len()) {
                        pass.set_bind_group(1, &slot.bg, &[]);
                        pass.draw(0..6, 0..1);
                    }
                }
            }
            if let Some(id) = p.shot {
                self.taken = id;
                let h = p.size.1;
                let w = ((p.size.0 as f32 * p.blit[4]).round() as u32).max(2);
                let shot_tex = f.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("shot"),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let view = shot_tex.create_view(&Default::default());
                {
                    let mut pass = f.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("shot"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_pipeline(&res.shot);
                    pass.set_bind_group(0, &tg.blit_bg, &[]);
                    pass.draw(0..3, 0..1);
                }
                let row = (w * 4).div_ceil(256) * 256;
                let buf = f.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("shot readback"),
                    size: (row * h) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                f.encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo { texture: &shot_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
                self.pending = Some(Pending { id, buf, size: (w, h), row });
            }
            drawn = true;
        }
        let clear = f.rt.clear_color();
        f.ui.prepare(f.device, f.queue, &mut f.rt.text.atlas, &f.rt.instances, f.size);
        let device = f.device;
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
            if site.mark != SLOT || !drawn {
                return;
            }
            res.blit_for(device, site.format, site.srgb_blend);
            let (Some(pipe), Some(tg)) = (res.blits.get(&(site.format, site.srgb_blend)), res.targets.as_ref()) else { return };
            let r = site.instance.rect;
            pass.set_viewport(r[0] - site.origin[0], r[1] - site.origin[1], r[2].max(1.0), r[3].max(1.0), 0.0, 1.0);
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &tg.blit_bg, &[]);
            pass.draw(0..3, 0..1);
        });
    }
}
