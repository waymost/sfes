//! Drawing a frame: RGB555 to RGBA8 on the CPU, a `wgpu` texture, and one nearest-neighbour
//! blit into a rectangle of the window (technical design section 11). No other pipeline.

use std::{borrow::Cow, sync::Arc};

use anyhow::{Context, Result, anyhow, bail};
use sfes_api::{Rgb555, VideoFrame};
use winit::window::Window;

use crate::layout::Rect;

/// A full-screen triangle that samples the frame; the viewport picks where it lands.
const SHADER: &str = "
struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) coordinate: vec2<f32>,
}

@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> Vertex {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: Vertex;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.coordinate = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

@group(0) @binding(0) var frame_texture: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;

@fragment
fn fragment_main(in: Vertex) -> @location(0) vec4<f32> {
    return textureSample(frame_texture, frame_sampler, in.coordinate);
}
";

/// The frame's texture, remade when its size changes.
struct FrameTexture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// The window's surface and what draws a frame onto it.
pub(crate) struct Video {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture_format: wgpu::TextureFormat,
    frame: Option<FrameTexture>,
    rgba: Vec<u8>,
}

impl Video {
    /// Creates the surface of `window` and the blit pipeline.
    ///
    /// # Errors
    ///
    /// Returns an error when there is no usable adapter or surface.
    pub(crate) fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window)
            .context("creating the window's surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..wgpu::RequestAdapterOptions::default()
        }))
        .context("finding a graphics adapter")?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .context("opening the graphics device")?;

        let capabilities = surface.get_capabilities(&adapter);
        let Some(format) = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .or_else(|| capabilities.formats.first().copied())
        else {
            bail!("the surface supports no texture format");
        };
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow!("the surface and adapter do not match"))?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        // A frame's bytes are the colours to show: sample and write without a transfer curve.
        let texture_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("frame sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        let pipeline = make_pipeline(&device, &layout, format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            layout,
            sampler,
            texture_format,
            frame: None,
            rgba: Vec::new(),
        })
    }

    /// Reconfigures the surface for a window of this size. A size of 0 is ignored.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Uploads `frame`, remaking the texture when its size differs from the last. An empty
    /// frame, as the machine holds before its first, is ignored.
    pub(crate) fn upload(&mut self, frame: &VideoFrame<'_>) {
        let (width, height) = (u32::from(frame.width), u32::from(frame.height));
        if width == 0 || height == 0 {
            return;
        }
        self.rgba.clear();
        self.rgba
            .extend(frame.pixels.iter().flat_map(|pixel| rgba(*pixel)));
        let remake = self
            .frame
            .as_ref()
            .is_none_or(|current| (current.width, current.height) != (width, height));
        if remake {
            self.frame = Some(self.make_texture(width, height));
        }
        if let Some(current) = &self.frame {
            self.queue.write_texture(
                current.texture.as_image_copy(),
                &self.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// A texture and its bind group for a frame of this size.
    fn make_texture(&self, width: u32, height: u32) -> FrameTexture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame bind group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        FrameTexture {
            texture,
            bind_group,
            width,
            height,
        }
    }

    /// Clears the window to black and draws the last uploaded frame into `rect`.
    ///
    /// # Errors
    ///
    /// Returns an error when the surface fails in a way reconfiguring cannot mend.
    pub(crate) fn draw(&mut self, rect: Rect) -> Result<()> {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                drop(texture);
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                bail!("the surface failed validation");
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(current) = &self.frame
                && rect.width > 0
                && rect.height > 0
            {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &current.bind_group, &[]);
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "window sizes are far below 2^24, where f32 stays exact"
                )]
                pass.set_viewport(
                    rect.x as f32,
                    rect.y as f32,
                    rect.width as f32,
                    rect.height as f32,
                    0.0,
                    1.0,
                );
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(surface_texture);
        Ok(())
    }
}

/// The pipeline that draws the frame texture over the whole viewport.
fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("frame shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("frame pipeline layout"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("frame pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fragment_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState::from(format))],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// An RGB555 pixel as RGBA8 bytes: each 5-bit channel `c` becomes `c << 3 | c >> 2`, so 0 is 0
/// and 31 is 255.
const fn rgba(pixel: Rgb555) -> [u8; 4] {
    const fn expand(channel: u8) -> u8 {
        (channel << 3) | (channel >> 2)
    }
    [
        expand(pixel.red()),
        expand(pixel.green()),
        expand(pixel.blue()),
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_expand_to_the_full_byte_range() {
        assert_eq!(rgba(Rgb555::BLACK), [0, 0, 0, 255]);
        assert_eq!(
            rgba(Rgb555::from_channels(31, 31, 31)),
            [255, 255, 255, 255]
        );
        assert_eq!(rgba(Rgb555::from_channels(16, 8, 1)), [132, 66, 8, 255]);
    }
}
