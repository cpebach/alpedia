//! alpedia_view：Alpedia の画面（Rust・wgpu・winit）。Python からは `import alpedia_view` で使う。
//!
//! Python が「ノード（親子の行列）」「メッシュ（頂点 (n,12) float32：位置・法線・色・不透明度・UV）」「テクスチャ」
//! 「カメラ」を作って動かし、`pump()` で入力を受け取り、`render()` で描く。陰影は頂点の色に焼き込み済み
//! （Panda3D 版と同じ頂点をそのまま使える）。行列は Panda3D と同じ行ベクトル形式（p' = p M、平行移動は 4 行目）。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use numpy::{PyReadonlyArrayDyn, PyUntypedArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::{CursorGrabMode, Window, WindowId};

const IDENT: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    // 列優先：(A B)[c][r] = Σ A[k][r] B[c][k]
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + r] * b[c * 4 + k];
            }
            o[c * 4 + r] = s;
        }
    }
    o
}

struct Node {
    parent: Option<usize>,
    local: [f32; 16],
    visible: bool,
    alive: bool,
    tint: [f32; 4],
}

struct Mesh {
    node: usize,
    verts: Vec<f32>,
    idx: Vec<u32>,
    dirty_v: bool,
    dirty_i: bool,
    vbuf: Option<(wgpu::Buffer, u64)>,
    ibuf: Option<(wgpu::Buffer, u64)>,
    n_idx: u32,
    tex: [Option<usize>; 2],
    blend: bool,
    depth_write: bool,
    two_sided: bool,
    depth_offset: i32,
    bin: i32,
    alive: bool,
    overlay: bool,
}

struct Tex {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    gpu: Option<wgpu::TextureView>,
    nearest: bool,
    dirty: bool,
}

/// 文字の形（文字の置き場の中の位置と、ずらし・送り。ラスタの画素単位）
#[derive(Clone, Copy)]
struct Glyph {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    xmin: f32,
    ymin: f32,
    adv: f32,
}

/// フォントと文字の置き場（テクスチャ）
struct Font {
    font: fontdue::Font,
    px: f32,
    tex: usize,
    glyphs: HashMap<char, Glyph>,
    cx: u32,
    cy: u32,
    row_h: u32,
    line_h: f32,
    ascent: f32,
}

const ATLAS: u32 = 2048;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct PipeKey {
    blend: bool,
    depth_write: bool,
    cull: bool,
    offset: i32,
    overlay: bool,
}

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    pipes: HashMap<PipeKey, wgpu::RenderPipeline>,
    cam_buf: wgpu::Buffer,
    cam_bg: wgpu::BindGroup,
    ui_buf: wgpu::Buffer,
    ui_bg: wgpu::BindGroup,
    model_bgl: wgpu::BindGroupLayout,
    model_buf: Option<(wgpu::Buffer, u64, wgpu::BindGroup)>,
    tex_bgl: wgpu::BindGroupLayout,
    sampler_lin: wgpu::Sampler,
    sampler_near: wgpu::Sampler,
    white: wgpu::TextureView,
    tex_bg: HashMap<(i64, i64), wgpu::BindGroup>,
}

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MODEL_STRIDE: u64 = 256;

impl Gfx {
    fn new(window: Arc<Window>, vsync: bool) -> Result<Gfx, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or("グラフィックの装置が見つかりません")?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()), memory_hints: Default::default() },
            None,
        ))
        .map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        // Panda3D と同じく色は変換しない（sRGB でない形式）
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0].remove_srgb_suffix());
        let present_mode = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        let config = wgpu::SurfaceConfiguration {
            usage: if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
            } else {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            },
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let depth = Self::make_depth(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("alpedia"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let ub = |b: u32, dynamic: bool, vis: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
            binding: b,
            visibility: vis,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: dynamic, min_binding_size: None },
            count: None,
        };
        let cam_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[ub(0, false, wgpu::ShaderStages::VERTEX)],
        });
        let model_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[ub(0, true, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let texe = |b: u32| wgpu::BindGroupLayoutEntry {
            binding: b,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[texe(0), texe(1), wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&cam_bgl, &model_bgl, &tex_bgl],
            push_constant_ranges: &[],
        });
        let cam_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cam_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &cam_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: cam_buf.as_entire_binding() }],
        });
        let ui_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ui_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &cam_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: ui_buf.as_entire_binding() }],
        });
        let mk_sampler = |f: wgpu::FilterMode| device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: f,
            min_filter: f,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let sampler_lin = mk_sampler(wgpu::FilterMode::Linear);
        let sampler_near = mk_sampler(wgpu::FilterMode::Nearest);
        let white = Self::upload_tex(&device, &queue, 1, 1, &[255, 255, 255, 255]);
        Ok(Gfx {
            window, surface, device, queue, config, depth, shader, layout, pipes: HashMap::new(), cam_buf, cam_bg, ui_buf, ui_bg, model_bgl,
            model_buf: None, tex_bgl, sampler_lin, sampler_near, white, tex_bg: HashMap::new(),
        })
    }

    fn make_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }

    fn upload_tex(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, rgba: &[u8]) -> wgpu::TextureView {
        let t = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        t.create_view(&Default::default())
    }

    fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.depth = Self::make_depth(&self.device, &self.config);
    }

    fn pipe(&mut self, k: PipeKey) -> &wgpu::RenderPipeline {
        if !self.pipes.contains_key(&k) {
            let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32, 4 => Float32x2];
            let p = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&self.layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout { array_stride: 48, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.config.format,
                        blend: if k.blend { Some(wgpu::BlendState::ALPHA_BLENDING) } else { None },
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: if k.cull { Some(wgpu::Face::Back) } else { None },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: k.depth_write && !k.overlay,
                    // 奥行きは逆向き（近い＝1）。画面の上の文字・板（overlay）は奥行きを見ない
                    depth_compare: if k.overlay { wgpu::CompareFunction::Always } else { wgpu::CompareFunction::GreaterEqual },
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState { constant: k.offset * 8, slope_scale: k.offset as f32 * 1.0, clamp: 0.0 },
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            });
            self.pipes.insert(k, p);
        }
        &self.pipes[&k]
    }
}

#[derive(Default)]
struct App {
    gfx: Option<Gfx>,
    err: Option<String>,
    events: Vec<(String, f64, f64, i64)>,
    title: String,
    size: (u32, u32),
    vsync: bool,
    grab: bool,
}

fn key_name(k: &Key) -> String {
    match k {
        Key::Character(c) => c.to_lowercase(),
        Key::Named(n) => match n {
            NamedKey::Space => "space".into(),
            NamedKey::Escape => "escape".into(),
            NamedKey::Enter => "enter".into(),
            NamedKey::Shift => "shift".into(),
            NamedKey::Control => "control".into(),
            NamedKey::Alt => "alt".into(),
            NamedKey::Tab => "tab".into(),
            NamedKey::Backspace => "backspace".into(),
            NamedKey::ArrowUp => "up arrow".into(),
            NamedKey::ArrowDown => "down arrow".into(),
            NamedKey::ArrowLeft => "left arrow".into(),
            NamedKey::ArrowRight => "right arrow".into(),
            other => format!("{other:?}").to_lowercase(),
        },
        _ => String::new(),
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() || self.err.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(self.size.0, self.size.1));
        match el.create_window(attrs) {
            Ok(w) => match Gfx::new(Arc::new(w), self.vsync) {
                Ok(g) => self.gfx = Some(g),
                Err(e) => self.err = Some(e),
            },
            Err(e) => self.err = Some(e.to_string()),
        }
    }

    fn window_event(&mut self, _el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => self.events.push(("close".into(), 0.0, 0.0, 0)),
            WindowEvent::Resized(s) => {
                if let Some(g) = self.gfx.as_mut() {
                    g.resize(s.width, s.height);
                }
                self.events.push(("resize".into(), s.width as f64, s.height as f64, 0));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let name = key_name(&event.logical_key);
                if !name.is_empty() {
                    let down = event.state == ElementState::Pressed;
                    self.events.push((format!("key:{name}"), if down { 1.0 } else { 0.0 }, 0.0, event.repeat as i64));
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    _ => 3,
                };
                self.events.push(("button".into(), if state == ElementState::Pressed { 1.0 } else { 0.0 }, 0.0, b));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                self.events.push(("wheel".into(), dy, 0.0, 0));
            }
            WindowEvent::Focused(f) => self.events.push(("focus".into(), if f { 1.0 } else { 0.0 }, 0.0, 0)),
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, ev: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = ev {
            if self.grab {
                self.events.push(("mouse".into(), delta.0, delta.1, 0));
            }
        }
    }
}

#[pyclass(unsendable)]
struct View {
    el: EventLoop<()>,
    app: App,
    nodes: Vec<Node>,
    free_nodes: Vec<usize>,
    meshes: Vec<Mesh>,
    free_meshes: Vec<usize>,
    textures: Vec<Tex>,
    view_proj: [f32; 16],
    clear: [f64; 3],
    capture: Option<String>,
    font: Option<Font>,
}

fn ro<'a, T: numpy::Element>(a: &'a PyReadonlyArrayDyn<'_, T>, name: &str) -> PyResult<&'a [T]> {
    a.as_slice().map_err(|_| PyValueError::new_err(format!("{name}: 連続した配列ではありません")))
}

impl View {
    fn world(&self, n: usize, cache: &mut Vec<Option<([f32; 16], bool)>>) -> ([f32; 16], bool) {
        if let Some(v) = cache[n] {
            return v;
        }
        let nd = &self.nodes[n];
        let r = match nd.parent {
            Some(p) if p < self.nodes.len() && self.nodes[p].alive => {
                let (pm, pv) = self.world(p, cache);
                (mat_mul(&pm, &nd.local), pv && nd.visible)
            }
            _ => (nd.local, nd.visible),
        };
        cache[n] = Some(r);
        r
    }

    fn check_node(&self, n: usize) -> PyResult<()> {
        if n < self.nodes.len() && self.nodes[n].alive {
            Ok(())
        } else {
            Err(PyValueError::new_err("ノードがありません"))
        }
    }

    fn check_mesh(&self, m: usize) -> PyResult<()> {
        if m < self.meshes.len() && self.meshes[m].alive {
            Ok(())
        } else {
            Err(PyValueError::new_err("メッシュがありません"))
        }
    }
}

#[pymethods]
impl View {
    #[new]
    #[pyo3(signature = (width=1280, height=720, title="Alpedia".to_string(), vsync=true))]
    fn new(width: u32, height: u32, title: String, vsync: bool) -> PyResult<Self> {
        let el = EventLoop::new().map_err(|e| PyValueError::new_err(e.to_string()))?;
        let mut v = View {
            el,
            app: App { title, size: (width, height), vsync, ..Default::default() },
            nodes: vec![Node { parent: None, local: IDENT, visible: true, alive: true, tint: [1.0; 4] }],
            free_nodes: vec![],
            meshes: vec![],
            free_meshes: vec![],
            textures: vec![],
            view_proj: IDENT,
            clear: [0.45, 0.65, 0.9],
            capture: None,
            font: None,
        };
        // 窓ができるまで回す
        for _ in 0..200 {
            let _ = v.el.pump_app_events(Some(Duration::from_millis(5)), &mut v.app);
            if v.app.gfx.is_some() {
                break;
            }
            if let Some(e) = &v.app.err {
                return Err(PyValueError::new_err(format!("画面を作れません: {e}")));
            }
        }
        if v.app.gfx.is_none() {
            return Err(PyValueError::new_err("画面を作れません（窓が開きません）"));
        }
        Ok(v)
    }

    /// 入力を受け取る：[(種類, a, b, n)]。種類："key:<名前>"（a=1 押した/0 離した、n=くり返し）・"mouse"（a,b=動き）・
    /// "button"（a=押した、n=ボタン）・"wheel"（a）・"resize"（a,b）・"focus"（a）・"close"
    fn pump(&mut self) -> Vec<(String, f64, f64, i64)> {
        let _ = self.el.pump_app_events(Some(Duration::ZERO), &mut self.app);
        std::mem::take(&mut self.app.events)
    }

    fn size(&self) -> (u32, u32) {
        self.app.gfx.as_ref().map(|g| (g.config.width, g.config.height)).unwrap_or(self.app.size)
    }

    fn set_title(&mut self, t: &str) {
        if let Some(g) = &self.app.gfx {
            g.window.set_title(t);
        }
    }

    /// マウスを窓の中に閉じ込めて隠す（視点を回す）か、戻す
    fn grab(&mut self, on: bool) {
        self.app.grab = on;
        if let Some(g) = &self.app.gfx {
            if on {
                let _ = g.window.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| g.window.set_cursor_grab(CursorGrabMode::Confined));
            } else {
                let _ = g.window.set_cursor_grab(CursorGrabMode::None);
            }
            g.window.set_cursor_visible(!on);
        }
    }

    fn set_clear(&mut self, r: f64, g: f64, b: f64) {
        self.clear = [r, g, b];
    }

    /// カメラ：目の位置・前・上（ワールド）、縦の視野角（度）、近い面
    fn camera(&mut self, eye: [f64; 3], fwd: [f64; 3], up: [f64; 3], fov: f64, near: f64) {
        let n = |v: [f64; 3]| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
            [v[0] / l, v[1] / l, v[2] / l]
        };
        let f = n(fwd);
        let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let r = n(cross(up, f));
        let u = cross(f, r);
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let (w, h) = self.size();
        let aspect = w as f64 / h.max(1) as f64;
        let fy = 1.0 / (fov.to_radians() * 0.5).tan();
        let fx = fy / aspect;
        // ビュー（行：右・上・前）と、逆向きの奥行きの無限遠の透視投影（近い＝1、遠い→0）
        let rows = [
            [fx * r[0], fx * r[1], fx * r[2], -fx * dot(r, eye)],
            [fy * u[0], fy * u[1], fy * u[2], -fy * dot(u, eye)],
            [0.0, 0.0, 0.0, near],
            [f[0], f[1], f[2], -dot(f, eye)],
        ];
        let mut m = [0f32; 16];
        for c in 0..4 {
            for rr in 0..4 {
                m[c * 4 + rr] = rows[rr][c] as f32;
            }
        }
        self.view_proj = m;
    }

    // ---- ノード ----
    #[pyo3(signature = (parent=0))]
    fn node_new(&mut self, parent: usize) -> PyResult<usize> {
        self.check_node(parent)?;
        let n = Node { parent: Some(parent), local: IDENT, visible: true, alive: true, tint: [1.0; 4] };
        Ok(if let Some(k) = self.free_nodes.pop() {
            self.nodes[k] = n;
            k
        } else {
            self.nodes.push(n);
            self.nodes.len() - 1
        })
    }

    /// 行ベクトル形式の 4x4（16 個、行の順）
    fn node_mat(&mut self, n: usize, m: Vec<f64>) -> PyResult<()> {
        self.check_node(n)?;
        if m.len() != 16 {
            return Err(PyValueError::new_err("行列は 16 個"));
        }
        let mut a = [0f32; 16];
        for (k, x) in m.iter().enumerate() {
            a[k] = *x as f32;
        }
        self.nodes[n].local = a;
        Ok(())
    }

    fn node_pos(&mut self, n: usize, x: f64, y: f64, z: f64) -> PyResult<()> {
        self.check_node(n)?;
        let l = &mut self.nodes[n].local;
        l[12] = x as f32;
        l[13] = y as f32;
        l[14] = z as f32;
        Ok(())
    }

    fn node_show(&mut self, n: usize, on: bool) -> PyResult<()> {
        self.check_node(n)?;
        self.nodes[n].visible = on;
        Ok(())
    }

    fn node_tint(&mut self, n: usize, r: f32, g: f32, b: f32, a: f32) -> PyResult<()> {
        self.check_node(n)?;
        self.nodes[n].tint = [r, g, b, a];
        Ok(())
    }

    fn node_parent(&mut self, n: usize, p: usize) -> PyResult<()> {
        self.check_node(n)?;
        self.check_node(p)?;
        self.nodes[n].parent = Some(p);
        Ok(())
    }

    /// ノードとその子・メッシュを消す
    fn node_remove(&mut self, n: usize) -> PyResult<()> {
        if n == 0 || n >= self.nodes.len() || !self.nodes[n].alive {
            return Ok(());
        }
        let mut gone = vec![false; self.nodes.len()];
        gone[n] = true;
        loop {
            let mut changed = false;
            for k in 0..self.nodes.len() {
                if !gone[k] && self.nodes[k].alive {
                    if let Some(p) = self.nodes[k].parent {
                        if gone[p] {
                            gone[k] = true;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for k in 0..self.nodes.len() {
            if gone[k] {
                self.nodes[k].alive = false;
                self.free_nodes.push(k);
            }
        }
        for (m, mesh) in self.meshes.iter_mut().enumerate() {
            if mesh.alive && gone[mesh.node] {
                mesh.alive = false;
                mesh.vbuf = None;
                mesh.ibuf = None;
                mesh.verts = Vec::new();
                mesh.idx = Vec::new();
                self.free_meshes.push(m);
            }
        }
        Ok(())
    }

    // ---- メッシュ ----
    fn mesh_new(&mut self, node: usize) -> PyResult<usize> {
        self.check_node(node)?;
        let m = Mesh {
            node, verts: vec![], idx: vec![], dirty_v: false, dirty_i: false, vbuf: None, ibuf: None, n_idx: 0, tex: [None, None],
            blend: false, depth_write: true, two_sided: true, depth_offset: 0, bin: 0, alive: true, overlay: false,
        };
        Ok(if let Some(k) = self.free_meshes.pop() {
            self.meshes[k] = m;
            k
        } else {
            self.meshes.push(m);
            self.meshes.len() - 1
        })
    }

    /// 頂点 (n,12) float32 と、三角形 (m,3)（None なら前と同じ）
    #[pyo3(signature = (m, verts, tris=None))]
    fn mesh_set(&mut self, m: usize, verts: PyReadonlyArrayDyn<'_, f32>, tris: Option<PyReadonlyArrayDyn<'_, i64>>) -> PyResult<()> {
        self.check_mesh(m)?;
        let v = ro(&verts, "verts")?;
        if v.len() % 12 != 0 {
            return Err(PyValueError::new_err("頂点は (n,12)"));
        }
        let nv = v.len() / 12;
        let mesh = &mut self.meshes[m];
        if let Some(t) = &tris {
            let t = ro(t, "tris")?;
            if t.iter().any(|&i| i < 0 || i as usize >= nv) {
                return Err(PyValueError::new_err("三角形の頂点の番号が範囲外"));
            }
            mesh.idx = t.iter().map(|&i| i as u32).collect();
            mesh.dirty_i = true;
        } else if mesh.idx.iter().any(|&i| i as usize >= nv) {
            return Err(PyValueError::new_err("頂点の数が変わるときは三角形も渡す"));
        }
        mesh.verts.clear();
        mesh.verts.extend_from_slice(v);
        mesh.dirty_v = true;
        let _ = verts.shape();
        Ok(())
    }

    /// 見え方：半透明・奥行きを書く・両面・手前に寄せる量・描く順（大きいほど後）
    #[pyo3(signature = (m, blend=false, depth_write=true, two_sided=true, depth_offset=0, bin=0))]
    fn mesh_props(&mut self, m: usize, blend: bool, depth_write: bool, two_sided: bool, depth_offset: i32, bin: i32) -> PyResult<()> {
        self.check_mesh(m)?;
        let x = &mut self.meshes[m];
        x.blend = blend;
        x.depth_write = depth_write;
        x.two_sided = two_sided;
        x.depth_offset = depth_offset.clamp(0, 8);
        x.bin = bin;
        Ok(())
    }

    /// テクスチャ：slot 0（本来のテクスチャ）・1（掛け合わせる2枚目）。tex < 0 で外す
    fn mesh_tex(&mut self, m: usize, slot: usize, tex: i64) -> PyResult<()> {
        self.check_mesh(m)?;
        if slot > 1 || (tex >= 0 && tex as usize >= self.textures.len()) {
            return Err(PyValueError::new_err("mesh_tex"));
        }
        self.meshes[m].tex[slot] = if tex < 0 { None } else { Some(tex as usize) };
        Ok(())
    }

    fn mesh_remove(&mut self, m: usize) -> PyResult<()> {
        if m < self.meshes.len() && self.meshes[m].alive {
            let x = &mut self.meshes[m];
            x.alive = false;
            x.vbuf = None;
            x.ibuf = None;
            x.verts = Vec::new();
            x.idx = Vec::new();
            self.free_meshes.push(m);
        }
        Ok(())
    }

    // ---- テクスチャ ----
    /// 画像ファイル（PNG）を読む。戻り値はテクスチャの番号（読めなければ -1）。nearest：ドット絵のようにくっきり
    #[pyo3(signature = (path, nearest=true))]
    fn tex_load(&mut self, path: &str, nearest: bool) -> i64 {
        match image::open(path) {
            Ok(img) => {
                // Panda3D と同じく v は下から上（画像の行を上下逆に）
                let rgba = image::imageops::flip_vertical(&img.to_rgba8());
                let (w, h) = rgba.dimensions();
                self.textures.push(Tex { w, h, rgba: rgba.into_raw(), gpu: None, nearest, dirty: false });
                self.textures.len() as i64 - 1
            }
            Err(_) => -1,
        }
    }

    /// 画面の上（2D、奥行きなし・最後に描く）に置くか。座標は縦の高さが 1、真ん中が原点、x 右・y 上
    fn mesh_overlay(&mut self, m: usize, on: bool) -> PyResult<()> {
        self.check_mesh(m)?;
        self.meshes[m].overlay = on;
        Ok(())
    }

    /// フォント（.ttf / .ttc。index は .ttc の中の番号）を読む
    #[pyo3(signature = (path, index=0, px=40.0))]
    fn font_load(&mut self, path: &str, index: u32, px: f32) -> PyResult<()> {
        let data = std::fs::read(path).map_err(|e| PyValueError::new_err(format!("フォントを読めません: {e}")))?;
        let font = fontdue::Font::from_bytes(data, fontdue::FontSettings { collection_index: index, scale: px, ..Default::default() })
            .map_err(|e| PyValueError::new_err(format!("フォントを読めません: {e}")))?;
        let lm = font.horizontal_line_metrics(px);
        let (ascent, line_h) = match lm {
            Some(l) => (l.ascent, l.new_line_size),
            None => (px * 0.9, px * 1.25),
        };
        self.textures.push(Tex { w: ATLAS, h: ATLAS, rgba: vec![0; (ATLAS * ATLAS * 4) as usize], gpu: None, nearest: false, dirty: false });
        let tex = self.textures.len() - 1;
        self.font = Some(Font { font, px, tex, glyphs: HashMap::new(), cx: 1, cy: 1, row_h: 0, line_h, ascent });
        Ok(())
    }

    /// 文字のメッシュを作る（メッシュ m の頂点・三角形・テクスチャを置きかえる）。座標は size が1行の高さになる単位で、
    /// 1行目の上が y=0（下へ負）、x は右へ。align：0 左・1 中央・2 右。max_width > 0 なら折り返す。戻り値 (幅, 高さ)
    #[pyo3(signature = (m, text, size, color, max_width=0.0, align=0, line_height=1.25))]
    fn text_mesh(&mut self, m: usize, text: &str, size: f32, color: [f32; 4], max_width: f32, align: i32, line_height: f32)
        -> PyResult<(f32, f32)> {
        self.check_mesh(m)?;
        let View { font, textures, meshes, .. } = self;
        let f = font.as_mut().ok_or_else(|| PyValueError::new_err("フォントがありません（font_load）"))?;
        let k = size / f.px;
        // 行に分ける（折り返し）
        let mut lines: Vec<Vec<char>> = vec![vec![]];
        let mut wline = 0.0f32;
        for ch in text.chars() {
            if ch == '\n' {
                lines.push(vec![]);
                wline = 0.0;
                continue;
            }
            let adv = f.font.metrics(ch, f.px).advance_width * k;
            if max_width > 0.0 && wline + adv > max_width && !lines.last().unwrap().is_empty() {
                lines.push(vec![]);
                wline = 0.0;
            }
            lines.last_mut().unwrap().push(ch);
            wline += adv;
        }
        let mut verts: Vec<f32> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let lh = size * line_height;
        let mut maxw = 0.0f32;
        let widths: Vec<f32> = lines.iter().map(|l| l.iter().map(|&c| f.font.metrics(c, f.px).advance_width * k).sum()).collect();
        for w in &widths {
            maxw = maxw.max(*w);
        }
        let atlas = &mut textures[f.tex];
        for (li, line) in lines.iter().enumerate() {
            let mut x = match align {
                1 => (maxw - widths[li]) * 0.5,
                2 => maxw - widths[li],
                _ => 0.0,
            };
            let base = -(li as f32) * lh - f.ascent * k;
            for &ch in line {
                let g = match f.glyphs.get(&ch) {
                    Some(g) => *g,
                    None => {
                        let (mt, bmp) = f.font.rasterize(ch, f.px);
                        let (w, h) = (mt.width as u32, mt.height as u32);
                        if f.cx + w + 1 >= ATLAS {
                            f.cx = 1;
                            f.cy += f.row_h + 1;
                            f.row_h = 0;
                        }
                        if f.cy + h + 1 >= ATLAS {
                            // 置き場がいっぱい：最初から（前の文字は次に使うとき描き直す）
                            f.glyphs.clear();
                            f.cx = 1;
                            f.cy = 1;
                            f.row_h = 0;
                        }
                        for yy in 0..h {
                            for xx in 0..w {
                                let a = bmp[(yy * w + xx) as usize];
                                let o = (((f.cy + yy) * ATLAS + f.cx + xx) * 4) as usize;
                                atlas.rgba[o..o + 4].copy_from_slice(&[255, 255, 255, a]);
                            }
                        }
                        atlas.dirty = true;
                        let g = Glyph { x: f.cx, y: f.cy, w, h, xmin: mt.xmin as f32, ymin: mt.ymin as f32, adv: mt.advance_width };
                        f.cx += w + 1;
                        f.row_h = f.row_h.max(h);
                        f.glyphs.insert(ch, g);
                        g
                    }
                };
                if g.w > 0 && g.h > 0 {
                    let x0 = x + g.xmin * k;
                    let y0 = base + g.ymin * k;
                    let (x1, y1) = (x0 + g.w as f32 * k, y0 + g.h as f32 * k);
                    let (u0, v0) = (g.x as f32 / ATLAS as f32, g.y as f32 / ATLAS as f32);
                    let (u1, v1) = ((g.x + g.w) as f32 / ATLAS as f32, (g.y + g.h) as f32 / ATLAS as f32);
                    let n = (verts.len() / 12) as u32;
                    for (px, py, u, v) in [(x0, y0, u0, v1), (x1, y0, u1, v1), (x1, y1, u1, v0), (x0, y1, u0, v0)] {
                        verts.extend_from_slice(&[px, py, 0.0, 0.0, 0.0, -1.0, color[0], color[1], color[2], color[3], u, v]);
                    }
                    idx.extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
                }
                x += g.adv * k;
            }
        }
        let mesh = &mut meshes[m];
        mesh.verts = verts;
        mesh.idx = idx;
        mesh.dirty_v = true;
        mesh.dirty_i = true;
        mesh.tex = [Some(f.tex), None];
        mesh.blend = true;
        Ok((maxw, lines.len() as f32 * lh))
    }

    /// 次に描く画面を PNG に保存する
    fn screenshot(&mut self, path: String) {
        self.capture = Some(path);
    }

    /// RGBA の画素（上の行から、w×h×4 バイト）からテクスチャを作る
    #[pyo3(signature = (w, h, rgba, nearest=false))]
    fn tex_rgba(&mut self, w: u32, h: u32, rgba: Vec<u8>, nearest: bool) -> PyResult<i64> {
        if rgba.len() != (w * h * 4) as usize || w == 0 || h == 0 {
            return Err(PyValueError::new_err("tex_rgba: 大きさが合いません"));
        }
        let mut flipped = Vec::with_capacity(rgba.len());
        for y in (0..h as usize).rev() {
            flipped.extend_from_slice(&rgba[y * w as usize * 4..(y + 1) * w as usize * 4]);
        }
        self.textures.push(Tex { w, h, rgba: flipped, gpu: None, nearest, dirty: false });
        Ok(self.textures.len() as i64 - 1)
    }

    /// 描く。戻り値：描いたメッシュの数
    fn render(&mut self) -> PyResult<usize> {
        // 行列（親子）を計算
        let mut cache: Vec<Option<([f32; 16], bool)>> = vec![None; self.nodes.len()];
        let mut draw: Vec<(usize, [f32; 16], [f32; 4])> = Vec::new();
        for (k, m) in self.meshes.iter().enumerate() {
            if !m.alive || m.idx.is_empty() || m.verts.is_empty() || m.node >= self.nodes.len() || !self.nodes[m.node].alive {
                continue;
            }
            let (w, vis) = self.world(m.node, &mut cache);
            if vis {
                draw.push((k, w, self.nodes[m.node].tint));
            }
        }
        // 半透明は後に（描く順 bin で）
        draw.sort_by_key(|&(k, _, _)| (self.meshes[k].overlay, self.meshes[k].blend, self.meshes[k].bin));
        let View { app, meshes, textures, view_proj, clear, capture, .. } = self;
        let g = match app.gfx.as_mut() {
            Some(g) => g,
            None => return Ok(0),
        };
        // テクスチャ・頂点を GPU へ
        for t in textures.iter_mut() {
            if t.dirty {
                t.dirty = false;
                t.gpu = None;
                g.tex_bg.clear();
            }
            if t.gpu.is_none() {
                t.gpu = Some(Gfx::upload_tex(&g.device, &g.queue, t.w, t.h, &t.rgba));
            }
        }
        for &(k, _, _) in &draw {
            let m = &mut meshes[k];
            if m.dirty_v {
                let bytes: &[u8] = bytemuck::cast_slice(&m.verts);
                let need = bytes.len() as u64;
                match &m.vbuf {
                    Some((b, cap)) if *cap >= need => g.queue.write_buffer(b, 0, bytes),
                    _ => {
                        let b = g.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None, contents: bytes, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST });
                        m.vbuf = Some((b, need));
                    }
                }
                m.dirty_v = false;
            }
            if m.dirty_i {
                let bytes: &[u8] = bytemuck::cast_slice(&m.idx);
                let need = bytes.len() as u64;
                match &m.ibuf {
                    Some((b, cap)) if *cap >= need => g.queue.write_buffer(b, 0, bytes),
                    _ => {
                        let b = g.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None, contents: bytes, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST });
                        m.ibuf = Some((b, need));
                    }
                }
                m.n_idx = m.idx.len() as u32;
                m.dirty_i = false;
            }
        }
        // 行列などをまとめて1つの uniform に
        let need = (draw.len().max(1) as u64) * MODEL_STRIDE;
        if g.model_buf.as_ref().map(|x| x.1 < need).unwrap_or(true) {
            let cap = need.next_power_of_two().max(MODEL_STRIDE * 64);
            let b = g.device.create_buffer(&wgpu::BufferDescriptor {
                label: None, size: cap, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let bg = g.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &g.model_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &b, offset: 0, size: wgpu::BufferSize::new(96) }),
                }],
            });
            g.model_buf = Some((b, cap, bg));
        }
        let mut data = vec![0u8; need as usize];
        for (q, &(k, w, tint)) in draw.iter().enumerate() {
            let m = &meshes[k];
            let flags = [if m.tex[1].is_some() { 1.0f32 } else { 0.0 }, if m.tex[0].is_some() { 1.0 } else { 0.0 }, 0.0, 0.0];
            let o = q * MODEL_STRIDE as usize;
            data[o..o + 64].copy_from_slice(bytemuck::cast_slice(&w));
            data[o + 64..o + 80].copy_from_slice(bytemuck::cast_slice(&tint));
            data[o + 80..o + 96].copy_from_slice(bytemuck::cast_slice(&flags));
        }
        g.queue.write_buffer(&g.model_buf.as_ref().unwrap().0, 0, &data);
        g.queue.write_buffer(&g.cam_buf, 0, bytemuck::cast_slice(view_proj));
        // 画面の上（overlay）：縦の高さが 1、真ん中が原点、x 右・y 上
        let aspect = g.config.width as f32 / g.config.height.max(1) as f32;
        let ui: [f32; 16] = [2.0 / aspect, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0];
        g.queue.write_buffer(&g.ui_buf, 0, bytemuck::cast_slice(&ui));
        // テクスチャの組の bind group
        for &(k, _, _) in &draw {
            let key = (meshes[k].tex[0].map(|x| x as i64).unwrap_or(-1), meshes[k].tex[1].map(|x| x as i64).unwrap_or(-1));
            if !g.tex_bg.contains_key(&key) {
                let v0 = if key.0 >= 0 { textures[key.0 as usize].gpu.as_ref().unwrap() } else { &g.white };
                let v1 = if key.1 >= 0 { textures[key.1 as usize].gpu.as_ref().unwrap() } else { &g.white };
                let near = key.0 < 0 || textures[key.0 as usize].nearest;
                let bg = g.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &g.tex_bgl,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(v0) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(v1) },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(if near { &g.sampler_near } else { &g.sampler_lin }),
                        },
                    ],
                });
                g.tex_bg.insert(key, bg);
            }
        }
        for &(k, _, _) in &draw {
            let m = &meshes[k];
            let pk = PipeKey { blend: m.blend, depth_write: m.depth_write, cull: !m.two_sided, offset: m.depth_offset, overlay: m.overlay };
            g.pipe(pk);
        }
        let frame = match g.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                g.surface.configure(&g.device, &g.config);
                return Ok(0);
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = g.device.create_command_encoder(&Default::default());
        {
            let lin = |x: f64| x;
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: lin(clear[0]), g: lin(clear[1]), b: lin(clear[2]), a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &g.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_bind_group(0, &g.cam_bg, &[]);
            let mbg = &g.model_buf.as_ref().unwrap().2;
            for (q, &(k, _, _)) in draw.iter().enumerate() {
                let m = &meshes[k];
                let (vb, ib) = match (&m.vbuf, &m.ibuf) {
                    (Some(v), Some(i)) => (&v.0, &i.0),
                    _ => continue,
                };
                let pk = PipeKey { blend: m.blend, depth_write: m.depth_write, cull: !m.two_sided, offset: m.depth_offset, overlay: m.overlay };
                rp.set_pipeline(&g.pipes[&pk]);
                rp.set_bind_group(0, if m.overlay { &g.ui_bg } else { &g.cam_bg }, &[]);
                rp.set_bind_group(1, mbg, &[(q as u64 * MODEL_STRIDE) as u32]);
                let key = (m.tex[0].map(|x| x as i64).unwrap_or(-1), m.tex[1].map(|x| x as i64).unwrap_or(-1));
                rp.set_bind_group(2, &g.tex_bg[&key], &[]);
                rp.set_vertex_buffer(0, vb.slice(..));
                rp.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..m.n_idx, 0, 0..1);
            }
        }
        let shot = capture.take().and_then(|path| {
            if !g.config.usage.contains(wgpu::TextureUsages::COPY_SRC) {
                return None;
            }
            let (w, h) = (g.config.width, g.config.height);
            let row = (w * 4).div_ceil(256) * 256;
            let buf = g.device.create_buffer(&wgpu::BufferDescriptor {
                label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false });
            enc.copy_texture_to_buffer(
                frame.texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            Some((path, buf, row, w, h))
        });
        g.queue.submit(Some(enc.finish()));
        if let Some((path, buf, row, w, h)) = shot {
            let sl = buf.slice(..);
            sl.map_async(wgpu::MapMode::Read, |_| {});
            let _ = g.device.poll(wgpu::Maintain::Wait);
            let data = sl.get_mapped_range();
            let bgr = matches!(g.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
            let mut px = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                for x in 0..w {
                    let o = (y * row + x * 4) as usize;
                    let (r, gg, b) = if bgr { (data[o + 2], data[o + 1], data[o]) } else { (data[o], data[o + 1], data[o + 2]) };
                    px.extend_from_slice(&[r, gg, b, 255]);
                }
            }
            drop(data);
            buf.unmap();
            let _ = image::save_buffer(&path, &px, w, h, image::ColorType::Rgba8);
        }
        frame.present();
        Ok(draw.len())
    }
}

#[pymodule]
fn alpedia_view(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<View>()?;
    m.add("VERSION", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
