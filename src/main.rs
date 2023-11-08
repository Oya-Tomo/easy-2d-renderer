use bytemuck::{Pod, Zeroable};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    Buffer, BufferUsages, CommandEncoderDescriptor, Device, Dx12Compiler, Instance,
    InstanceDescriptor, InstanceFlags, Queue, RenderPass, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RequestAdapterOptions, ShaderModule, Surface,
    SurfaceConfiguration, TextureFormat,
};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoopBuilder,
    window::Window,
};

fn main() -> Result<(), winit::error::EventLoopError> {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let event_loop = EventLoopBuilder::new().build().unwrap();
    let window = Window::new(&event_loop).unwrap();

    let mut state = rt.block_on(State::new(&window));

    return event_loop.run(move |event, elwt| match event {
        Event::WindowEvent { window_id, event } => {
            if window_id == window.id() {
                match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::RedrawRequested => state.render(),
                    WindowEvent::Resized(size) => {
                        state.resize(size);
                    }
                    _ => (),
                }
            }
        }
        Event::AboutToWait => {
            window.request_redraw();
        }
        _ => (),
    });
}

pub struct State {
    surface: Surface,
    config: SurfaceConfiguration,
    device: Device,
    queue: Queue,
}

impl State {
    pub async fn new(window: &Window) -> Self {
        let instance = Instance::new(InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: InstanceFlags::default(),
            dx12_shader_compiler: Dx12Compiler::default(),
            gles_minor_version: wgpu::Gles3MinorVersion::Automatic,
        });
        let surface = unsafe { instance.create_surface(&window) }.unwrap();

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let size = window.inner_size();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        return Self {
            surface,
            config,
            device,
            queue,
        };
    }

    pub fn render(&self) {
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Command Encoder"),
            });

        let output = self.surface.get_current_texture().unwrap();
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let pipeline = TrianglePipeline::new(&self.device, output.texture.format());

        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pipeline.draw(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct TriangleVertex {
    pub position: [f32; 2],
}

impl TriangleVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![
        0 => Float32x2
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        return wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        };
    }
}

const TRIANGLE_VERTICES: [TriangleVertex; 3] = [
    TriangleVertex {
        position: [0.0, 0.5],
    },
    TriangleVertex {
        position: [-0.5, -0.5],
    },
    TriangleVertex {
        position: [0.5, -0.5],
    },
];

pub struct TrianglePipeline {
    pipeline: RenderPipeline,
    vertex_buffer: Buffer,
}

impl TrianglePipeline {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        let vertex_buffer = Self::create_vertex_buffer(device);

        let shader = Self::create_shader_module(device);

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[TriangleVertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });
        return Self {
            pipeline,
            vertex_buffer,
        };
    }

    pub fn draw<'a>(&'a self, render_pass: &mut RenderPass<'a>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..TRIANGLE_VERTICES.len() as _, 0..1);
    }

    pub fn create_shader_module(device: &Device) -> ShaderModule {
        return device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
    }

    pub fn create_vertex_buffer(device: &Device) -> Buffer {
        return device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Triangle Vertex"),
            usage: BufferUsages::VERTEX,
            contents: bytemuck::cast_slice(&TRIANGLE_VERTICES),
        });
    }
}
