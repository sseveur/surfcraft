//! GPU 3D view: the board mesh rendered offscreen with wgpu (4x MSAA, depth), shown as an egui image.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use glam::{Mat4, Vec3};
use surfcraft_core::Mesh;
use wgpu::util::DeviceExt as _;

use crate::preview3d::Orbit;

const SAMPLES: u32 = 4;
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    /// 0 board, 1 fin.
    kind: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    light: [f32; 4],
    base: [f32; 4],
    params: [f32; 4],
}

struct Targets {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
    id: egui::TextureId,
}

struct Gpu {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
    index_count: u32,
    mesh: Option<Arc<Mesh>>,
    fins: Option<Arc<Mesh>>,
    /// Bounding box of the uploaded mesh.
    bounds: (Vec3, Vec3),
    targets: Option<Targets>,
}

#[derive(Default)]
pub struct Viewer3d {
    gpu: Option<Gpu>,
    pub zebra: bool,
}

impl Gpu {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("board3d"),
            source: wgpu::ShaderSource::Wgsl(include_str!("viewer3d.wgsl").into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("board3d uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("board3d bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("board3d bg"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("board3d layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("board3d pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32],
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            uniforms,
            bind_group,
            vertices: None,
            indices: None,
            index_count: 0,
            mesh: None,
            fins: None,
            bounds: (Vec3::ZERO, Vec3::ONE),
            targets: None,
        }
    }

    fn upload(&mut self, device: &wgpu::Device, mesh: &Arc<Mesh>, fins: Option<&Arc<Mesh>>) {
        let same = |a: &Option<Arc<Mesh>>, b: Option<&Arc<Mesh>>| match (a, b) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same(&self.mesh, Some(mesh)) && same(&self.fins, fins) {
            return;
        }
        let vertices = |m: &Mesh, kind: f32| -> Vec<Vertex> {
            m.positions
                .iter()
                .zip(&m.normals)
                .map(|(p, n)| Vertex {
                    pos: [p.x as f32, p.y as f32, p.z as f32],
                    normal: [n.x as f32, n.y as f32, n.z as f32],
                    kind,
                })
                .collect()
        };
        let mut verts = vertices(mesh, 0.0);
        let mut idx: Vec<u32> = mesh.triangles.iter().flatten().copied().collect();
        // The camera frames the board alone, so showing or hiding fins doesn't move the view.
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in &verts {
            let p = Vec3::from(v.pos);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        if let Some(f) = fins.filter(|f| !f.triangles.is_empty()) {
            let offset = verts.len() as u32;
            verts.extend(vertices(f, 1.0));
            idx.extend(f.triangles.iter().flatten().map(|i| i + offset));
        }
        self.mesh = Some(mesh.clone());
        self.fins = fins.cloned();
        if idx.is_empty() || verts.is_empty() {
            // Degenerate board: nothing to draw (and wgpu rejects empty buffers).
            self.vertices = None;
            self.indices = None;
            self.index_count = 0;
            return;
        }
        self.bounds = (lo, hi);
        self.vertices = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("board3d vertices"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
        self.indices = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("board3d indices"),
                contents: bytemuck::cast_slice(&idx),
                usage: wgpu::BufferUsages::INDEX,
            }),
        );
        self.index_count = idx.len() as u32;
        self.mesh = Some(mesh.clone());
    }

    fn ensure_targets(&mut self, rs: &egui_wgpu::RenderState, size: [u32; 2]) -> egui::TextureId {
        if let Some(t) = &self.targets
            && t.size == size
        {
            return t.id;
        }
        let device = &rs.device;
        let make = |samples: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("board3d target"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let msaa = make(SAMPLES, COLOR, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolve = make(
            1,
            COLOR,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = make(SAMPLES, DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let mut renderer = rs.renderer.write();
        let id = match self.targets.take() {
            Some(old) => {
                renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &resolve,
                    wgpu::FilterMode::Linear,
                    old.id,
                );
                old.id
            }
            None => renderer.register_native_texture(device, &resolve, wgpu::FilterMode::Linear),
        };
        self.targets = Some(Targets {
            size,
            msaa,
            resolve,
            depth,
            id,
        });
        id
    }

    fn render(&self, rs: &egui_wgpu::RenderState, uniforms: &Uniforms, clear: [f64; 3]) {
        let Some(t) = &self.targets else { return };
        rs.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(uniforms));
        let mut encoder = rs
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("board3d"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("board3d pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa,
                    resolve_target: Some(&t.resolve),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let (Some(vb), Some(ib)) = (&self.vertices, &self.indices) {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.index_count, 0, 0..1);
            }
        }
        rs.queue.submit([encoder.finish()]);
    }
}

/// Right-handed view matrix.
fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        glam::Vec4::new(s.x, u.x, -f.x, 0.0),
        glam::Vec4::new(s.y, u.y, -f.y, 0.0),
        glam::Vec4::new(s.z, u.z, -f.z, 0.0),
        glam::Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}

/// Right-handed perspective projection with wgpu's 0..1 depth range.
fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let h = 1.0 / (fov_y / 2.0).tan();
    let r = far / (near - far);
    Mat4::from_cols(
        glam::Vec4::new(h / aspect, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, h, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, r, -1.0),
        glam::Vec4::new(0.0, 0.0, r * near, 0.0),
    )
}

fn srgb_to_linear(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

impl Viewer3d {
    /// Show the board. Returns false when wgpu isn't available (caller falls back to the CPU preview).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        mesh: Option<&Arc<Mesh>>,
        fins: Option<&Arc<Mesh>>,
        orbit: &mut Orbit,
        bg: egui::Color32,
    ) -> bool {
        let Some(rs) = rs else { return false };
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        if response.dragged_by(egui::PointerButton::Primary) {
            let d = response.drag_delta();
            orbit.yaw -= d.x * 0.01;
            orbit.pitch = (orbit.pitch + d.y * 0.01).clamp(-1.55, 1.55);
        }
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            orbit.pan += response.drag_delta();
        }
        if response.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta(), i.zoom_delta()));
            orbit.zoom = (orbit.zoom * zoom * (scroll.y * 0.0025).exp()).clamp(0.2, 20.0);
        }
        if response.double_clicked() {
            orbit.reset();
        }
        let ppp = ui.ctx().pixels_per_point();
        let size = [
            ((rect.width() * ppp) as u32).clamp(1, 8192),
            ((rect.height() * ppp) as u32).clamp(1, 8192),
        ];
        let gpu = self.gpu.get_or_insert_with(|| Gpu::new(&rs.device));
        if let Some(m) = mesh {
            gpu.upload(&rs.device, m, fins);
        }
        let id = gpu.ensure_targets(rs, size);

        let (lo, hi) = gpu.bounds;
        let centre = (lo + hi) / 2.0;
        let radius = ((hi - lo).length() / 2.0).max(1.0);
        let aspect = rect.width() / rect.height().max(1.0);
        let fov = 30f32.to_radians();
        let dist = radius / (fov / 2.0).sin() / orbit.zoom
            * if aspect < 1.0 { 1.0 / aspect } else { 0.75 };
        // z up; yaw around z, pitch above the horizon.
        let dir = Vec3::new(
            orbit.pitch.cos() * orbit.yaw.sin(),
            -orbit.pitch.cos() * orbit.yaw.cos(),
            orbit.pitch.sin(),
        );
        let right = Vec3::Z.cross(dir).normalize_or_zero();
        let up = dir.cross(right).normalize_or_zero();
        let pan_scale = dist * (fov / 2.0).tan() * 2.0 / rect.height().max(1.0);
        let target = centre - right * orbit.pan.x * pan_scale + up * orbit.pan.y * pan_scale;
        let eye = target + dir * dist;
        let view = look_at_rh(eye, target, Vec3::Z);
        let proj = perspective_rh(
            fov,
            aspect,
            (dist - radius * 2.0).max(1.0),
            dist + radius * 4.0,
        );
        let light = (up * 0.8 + right * 0.4 + dir * 0.6).normalize();
        let uniforms = Uniforms {
            view_proj: (proj * view).to_cols_array_2d(),
            eye: eye.extend(1.0).to_array(),
            light: light.extend(0.0).to_array(),
            base: [0.80, 0.83, 0.87, 1.0],
            params: [if self.zebra { 1.0 } else { 0.0 }, 24.0, 0.0, 0.0],
        };
        let clear = [
            srgb_to_linear(bg.r()),
            srgb_to_linear(bg.g()),
            srgb_to_linear(bg.b()),
        ];
        gpu.render(rs, &uniforms, clear);
        ui.painter().image(
            id,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        if mesh.is_none() {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::tr("Computing…"),
                egui::FontId::proportional(14.0),
                egui::Color32::GRAY,
            );
        }
        ui.painter().text(
            rect.left_bottom() + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            crate::i18n::tr("drag: rotate · right-drag: pan · wheel: zoom · double-click: reset"),
            egui::FontId::proportional(11.0),
            egui::Color32::GRAY,
        );
        true
    }
}
