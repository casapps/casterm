//! wgpu-based monospace glyph-grid renderer
//!
//! Rasterizes glyphs on demand with `fontdue` into a single RGBA texture
//! atlas (alpha channel carries glyph coverage, RGB is left white so the
//! fragment shader can tint it with each cell's resolved foreground color),
//! then draws the whole terminal grid as one vertex buffer of textured/solid
//! quads per frame: one quad per cell background (skipped when it matches
//! the surface clear color) and one quad per non-space glyph.

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::app::editor::EditorState;
use crate::app::file_browser::FileBrowserState;
use crate::app::terminal::CursorStyle;
use crate::support::error::{CastermError, Result};
use crate::ui::render_model::{ResolvedCell, Rgb};

const ATLAS_SIZE: u32 = 2048;

const SHADER_SRC: &str = r#"
struct Uniforms {
    viewport: vec2<f32>,
};
@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) mode: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) mode: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let ndc_x = (in.pos.x / uniforms.viewport.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.pos.y / uniforms.viewport.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    out.mode = in.mode;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.mode > 0.5) {
        let coverage = textureSample(atlas_tex, atlas_sampler, in.uv).a;
        return vec4<f32>(in.color.rgb, in.color.a * coverage);
    }
    return in.color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    mode: f32,
}

impl Vertex {
    const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

/// Serialize vertices to a byte buffer without depending on `bytemuck` —
/// every field is a plain `f32`, so this is just little-endian packing.
fn vertices_to_bytes(vertices: &[Vertex]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(vertices));
    for v in vertices {
        bytes.extend_from_slice(&v.pos[0].to_le_bytes());
        bytes.extend_from_slice(&v.pos[1].to_le_bytes());
        bytes.extend_from_slice(&v.uv[0].to_le_bytes());
        bytes.extend_from_slice(&v.uv[1].to_le_bytes());
        bytes.extend_from_slice(&v.color[0].to_le_bytes());
        bytes.extend_from_slice(&v.color[1].to_le_bytes());
        bytes.extend_from_slice(&v.color[2].to_le_bytes());
        bytes.extend_from_slice(&v.color[3].to_le_bytes());
        bytes.extend_from_slice(&v.mode.to_le_bytes());
    }
    bytes
}

/// One glyph's location inside the atlas texture, in both pixel and
/// normalized UV coordinates, plus the offsets needed to place its bitmap
/// inside a terminal cell.
#[derive(Clone, Copy)]
struct GlyphInfo {
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    width: f32,
    height: f32,
    left: f32,
    top: f32,
}

/// CPU-side glyph rasterization cache plus the GPU texture it's packed
/// into. Glyphs are rasterized lazily the first time a character is drawn.
struct GlyphAtlas {
    font: fontdue::Font,
    px: f32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    cache: HashMap<char, Option<GlyphInfo>>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    full: bool,
}

impl GlyphAtlas {
    fn new(device: &wgpu::Device, font: fontdue::Font, px: f32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("casterm-glyph-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            font,
            px,
            texture,
            view,
            cache: HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            full: false,
        }
    }

    /// Rasterize (if not cached) and upload `ch`'s glyph bitmap, returning
    /// its atlas placement. Returns `None` for glyphs with an empty bitmap
    /// (e.g. space) or once the atlas has run out of room.
    fn glyph(&mut self, ch: char, queue: &wgpu::Queue) -> Option<GlyphInfo> {
        if let Some(info) = self.cache.get(&ch) {
            return *info;
        }

        let (metrics, bitmap) = self.font.rasterize(ch, self.px);
        if metrics.width == 0 || metrics.height == 0 || self.full {
            self.cache.insert(ch, None);
            return None;
        }

        let w = metrics.width as u32;
        let h = metrics.height as u32;
        if self.cursor_x + w + 1 > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height + 1;
            self.row_height = 0;
        }
        if self.cursor_y + h + 1 > ATLAS_SIZE {
            // Atlas exhausted: stop trying to pack further glyphs this run.
            tracing::warn!("GUI glyph atlas full; some characters will render blank");
            self.full = true;
            self.cache.insert(ch, None);
            return None;
        }

        // Expand the coverage-only bitmap to RGBA: white RGB (tinted by the
        // cell's foreground color in the shader) with coverage as alpha.
        let mut rgba = Vec::with_capacity(bitmap.len() * 4);
        for coverage in &bitmap {
            rgba.extend_from_slice(&[255u8, 255, 255, *coverage]);
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: self.cursor_x,
                    y: self.cursor_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let atlas = ATLAS_SIZE as f32;
        let info = GlyphInfo {
            uv_min: [self.cursor_x as f32 / atlas, self.cursor_y as f32 / atlas],
            uv_max: [
                (self.cursor_x + w) as f32 / atlas,
                (self.cursor_y + h) as f32 / atlas,
            ],
            width: w as f32,
            height: h as f32,
            left: metrics.xmin as f32,
            top: metrics.ymin as f32,
        };

        self.cursor_x += w + 1;
        self.row_height = self.row_height.max(h);
        self.cache.insert(ch, Some(info));
        Some(info)
    }
}

/// Everything needed to draw one terminal grid frame to a window's surface.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    atlas: GlyphAtlas,
    cell_w: f32,
    cell_h: f32,
    ascent: f32,
}

/// A cell-coordinate range to highlight with the theme's selection colors.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub start: (u16, u16),
    pub end: (u16, u16),
}

impl Renderer {
    /// Create the GPU device/surface/pipeline for `window` and rasterize the
    /// font's ASCII printable range up front so the first frame isn't stalled
    /// on glyph misses.
    pub fn new(window: Arc<Window>, font: fontdue::Font, font_px: f32) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| CastermError::Gui(format!("failed to create GPU surface: {e}")))?;

        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| CastermError::Gui("no compatible GPU adapter found".to_string()))?;

        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("casterm-gui-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .map_err(|e| CastermError::Gui(format!("failed to acquire GPU device: {e}")))?;

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let metrics = font
            .horizontal_line_metrics(font_px)
            .ok_or_else(|| CastermError::Gui("font has no horizontal line metrics".to_string()))?;
        let cell_h = (metrics.ascent - metrics.descent + metrics.line_gap).ceil();
        let cell_w = font.metrics('M', font_px).advance_width.ceil();
        let ascent = metrics.ascent;

        let mut atlas = GlyphAtlas::new(&device, font, font_px);
        for ch in (0x20u8..0x7f).map(|b| b as char) {
            atlas.glyph(ch, &queue);
        }

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("casterm-gui-uniforms"),
            contents: &viewport_uniform_bytes(size.width.max(1) as f32, size.height.max(1) as f32),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("casterm-gui-atlas-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("casterm-gui-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("casterm-gui-bind-group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("casterm-gui-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("casterm-gui-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("casterm-gui-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            surface,
            config,
            pipeline,
            bind_group,
            uniform_buffer,
            atlas,
            cell_w,
            cell_h,
            ascent,
        })
    }

    /// The pixel size of one terminal cell, used by the window to compute
    /// cols/rows on resize and by mouse handling to map pixels to cells.
    pub fn cell_size(&self) -> (f32, f32) {
        (self.cell_w, self.cell_h)
    }

    /// Resize the swapchain to match the window's new physical size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            &viewport_uniform_bytes(width as f32, height as f32),
        );
    }

    /// Draw one full-grid frame: `cells` is `resolve_grid`'s row-major
    /// output, `cols`/`rows` its dimensions, `bg` the theme default
    /// background (used for the surface clear color), `selection` an
    /// optional highlighted cell range, `cursor_style` the DECSCUSR shape
    /// to draw for the cell flagged `is_cursor`, and `cursor_color` the
    /// fill used for that shape's non-block geometry.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        cells: &[ResolvedCell],
        cols: u16,
        rows: u16,
        bg: Rgb,
        selection: Option<Selection>,
        selection_bg: Rgb,
        selection_fg: Rgb,
        cursor_style: CursorStyle,
        cursor_color: Rgb,
        term_x_offset: f32,
        panel: Option<FileBrowserPanelView<'_>>,
        editor: Option<EditorPanelView<'_>>,
    ) -> Result<()> {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface
                    .get_current_texture()
                    .map_err(|e| CastermError::Gui(format!("failed to acquire frame: {e}")))?
            }
            Err(e) => return Err(CastermError::Gui(format!("failed to acquire frame: {e}"))),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut vertices: Vec<Vertex> = Vec::new();
        if let Some(editor) = &editor {
            // The built-in text editor takes over the full window (a
            // narrow tree-panel strip is unusably narrow for editing), so
            // it replaces the terminal-grid loop and any tree panel below
            // rather than sharing space with them.
            self.push_editor_panel(&mut vertices, editor);
        } else {
            for row in 0..rows {
                for col in 0..cols {
                    let idx = row as usize * cols as usize + col as usize;
                    let Some(cell) = cells.get(idx) else {
                        continue;
                    };
                    let x0 = term_x_offset + col as f32 * self.cell_w;
                    let y0 = row as f32 * self.cell_h;

                    let selected = selection.is_some_and(|s| in_selection(s, row, col));
                    let cell_bg = if selected { selection_bg } else { cell.bg };
                    if cell.is_cursor && !selected {
                        let (qy0, qh, qx0, qw) = match cursor_style {
                            CursorStyle::Block => (y0, self.cell_h, x0, self.cell_w),
                            CursorStyle::Underline => {
                                let h = self.cell_h * 0.15;
                                (y0 + self.cell_h - h, h, x0, self.cell_w)
                            }
                            CursorStyle::Bar => {
                                let w = self.cell_w * 0.12;
                                (y0, self.cell_h, x0, w)
                            }
                        };
                        push_quad(
                            &mut vertices,
                            qx0,
                            qy0,
                            qw,
                            qh,
                            [0.0, 0.0],
                            [0.0, 0.0],
                            rgb_to_f32(cursor_color, 1.0),
                            0.0,
                        );
                    } else if cell_bg != bg {
                        push_quad(
                            &mut vertices,
                            x0,
                            y0,
                            self.cell_w,
                            self.cell_h,
                            [0.0, 0.0],
                            [0.0, 0.0],
                            rgb_to_f32(cell_bg, 1.0),
                            0.0,
                        );
                    }

                    if cell.hidden || cell.ch == ' ' {
                        continue;
                    }
                    let Some(glyph) = self.atlas.glyph(cell.ch, &self.queue) else {
                        continue;
                    };
                    let gx = x0 + glyph.left.max(0.0);
                    let gy = y0 + (self.ascent - glyph.height - glyph.top);
                    let fg = if selected { selection_fg } else { cell.fg };
                    push_quad(
                        &mut vertices,
                        gx,
                        gy,
                        glyph.width,
                        glyph.height,
                        glyph.uv_min,
                        glyph.uv_max,
                        rgb_to_f32(fg, 1.0),
                        1.0,
                    );
                }
            }

            if let Some(panel) = &panel {
                self.push_file_browser_panel(&mut vertices, panel);
            }
        }

        let vertex_bytes = vertices_to_bytes(&vertices);
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("casterm-gui-vertices"),
                contents: &vertex_bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("casterm-gui-encoder"),
            });
        {
            let (r, g, b) = (bg.0, bg.1, bg.2);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("casterm-gui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: r as f64 / 255.0,
                            g: g as f64 / 255.0,
                            b: b as f64 / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !vertices.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Emit the file-browser panel's quads: a solid highlight quad behind
    /// the selected row, plus one glyph-atlas quad per visible non-space
    /// character. Mirrors `ui::tui::file_browser::FileBrowserPanel`'s row
    /// layout (depth indent, `▼`/`▶` marker, selected-row highlight) but as
    /// screen-space quads instead of ratatui buffer cells; the panel's
    /// unselected background relies on the surface clear color already
    /// matching the theme background (see `render`'s clear-color setup).
    fn push_file_browser_panel(&mut self, out: &mut Vec<Vertex>, panel: &FileBrowserPanelView<'_>) {
        let visible_rows = (self.config.height as f32 / self.cell_h).floor().max(0.0) as usize;
        let max_chars = (panel.width / self.cell_w).floor().max(0.0) as usize;

        for row_layout in file_browser_panel_rows(panel.state, visible_rows, max_chars) {
            let y0 = row_layout.visible_row as f32 * self.cell_h;
            if row_layout.is_selected {
                push_quad(
                    out,
                    panel.x,
                    y0,
                    panel.width,
                    self.cell_h,
                    [0.0, 0.0],
                    [0.0, 0.0],
                    rgb_to_f32(panel.selected_bg, 1.0),
                    0.0,
                );
            }

            let fg = if row_layout.is_selected {
                panel.selected_fg
            } else {
                panel.fg
            };

            for (col, ch) in row_layout.label.chars().enumerate() {
                if ch == ' ' {
                    continue;
                }
                let Some(glyph) = self.atlas.glyph(ch, &self.queue) else {
                    continue;
                };
                let gx = panel.x + col as f32 * self.cell_w + glyph.left.max(0.0);
                let gy = y0 + (self.ascent - glyph.height - glyph.top);
                push_quad(
                    out,
                    gx,
                    gy,
                    glyph.width,
                    glyph.height,
                    glyph.uv_min,
                    glyph.uv_max,
                    rgb_to_f32(fg, 1.0),
                    1.0,
                );
            }
        }
    }

    /// Default nano-style key-hint bar text, shown when no transient
    /// `status` message is set — verbatim match of
    /// `ui::tui::editor::HINT_BAR`.
    const HINT_BAR: &str = "^S Save  ^X Exit  ^T Close Panel";

    /// Emit the built-in editor's quads: a header row (file name plus
    /// `[Modified]` while dirty), the visible text lines, and a bottom
    /// key-hint bar (or a transient status message in its place) —
    /// screen-space mirror of `ui::tui::editor::EditorPanel`, but spanning
    /// the full window instead of a narrow panel strip.
    fn push_editor_panel(&mut self, out: &mut Vec<Vertex>, view: &EditorPanelView<'_>) {
        let width_px = self.config.width as f32;
        let max_chars = (width_px / self.cell_w).floor().max(0.0) as usize;
        let total_rows = (self.config.height as f32 / self.cell_h).floor().max(0.0) as usize;
        let text_rows = total_rows.saturating_sub(2);

        push_quad(
            out,
            0.0,
            0.0,
            width_px,
            self.cell_h,
            [0.0, 0.0],
            [0.0, 0.0],
            rgb_to_f32(view.bar_bg, 1.0),
            0.0,
        );
        let name = view
            .state
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("[No Name]");
        let header_text = if view.state.dirty() {
            format!("{name} [Modified]")
        } else {
            name.to_string()
        };
        self.push_editor_text_row(out, &header_text, 0.0, max_chars, view.bar_fg);

        for row_layout in editor_panel_rows(view.state, text_rows, max_chars) {
            let y0 = self.cell_h + row_layout.visible_row as f32 * self.cell_h;
            self.push_editor_text_row(out, &row_layout.text, y0, max_chars, view.fg);
        }

        let hint_y = total_rows.saturating_sub(1) as f32 * self.cell_h;
        push_quad(
            out,
            0.0,
            hint_y,
            width_px,
            self.cell_h,
            [0.0, 0.0],
            [0.0, 0.0],
            rgb_to_f32(view.bar_bg, 1.0),
            0.0,
        );
        let hint_text = view.status.unwrap_or(Self::HINT_BAR);
        self.push_editor_text_row(out, hint_text, hint_y, max_chars, view.bar_fg);
    }

    /// Shared glyph-emission helper for `push_editor_panel`'s header/text/
    /// hint-bar rows — one glyph-atlas quad per non-space character, at a
    /// fixed row `y`, truncated to `max_chars`.
    fn push_editor_text_row(
        &mut self,
        out: &mut Vec<Vertex>,
        text: &str,
        y: f32,
        max_chars: usize,
        color: Rgb,
    ) {
        for (col, ch) in text.chars().take(max_chars).enumerate() {
            if ch == ' ' {
                continue;
            }
            let Some(glyph) = self.atlas.glyph(ch, &self.queue) else {
                continue;
            };
            let gx = col as f32 * self.cell_w + glyph.left.max(0.0);
            let gy = y + (self.ascent - glyph.height - glyph.top);
            push_quad(
                out,
                gx,
                gy,
                glyph.width,
                glyph.height,
                glyph.uv_min,
                glyph.uv_max,
                rgb_to_f32(color, 1.0),
                1.0,
            );
        }
    }
}

/// One laid-out row of the built-in editor's text area, ready to turn into
/// quads: its position among the *visible* (scrolled-into-view) rows and
/// its already-truncated text. Split out from `Renderer::push_editor_panel`
/// as a pure function (no `wgpu` types) so the scroll/truncation logic is
/// unit-testable without a GPU device — same precedent as
/// `file_browser_panel_rows`.
struct EditorRowLayout {
    visible_row: usize,
    text: String,
}

fn editor_panel_rows(
    state: &EditorState,
    text_rows: usize,
    max_chars: usize,
) -> Vec<EditorRowLayout> {
    let (cursor_row, _) = state.cursor();
    let scroll = if text_rows == 0 {
        0
    } else if cursor_row >= text_rows {
        cursor_row + 1 - text_rows
    } else {
        0
    };

    state
        .lines()
        .iter()
        .enumerate()
        .skip(scroll)
        .take(text_rows)
        .map(|(row, line)| EditorRowLayout {
            visible_row: row - scroll,
            text: line.chars().take(max_chars).collect(),
        })
        .collect()
}

/// One laid-out row of the file-browser panel, ready to turn into quads:
/// its position among the *visible* (scrolled-into-view) rows, whether it's
/// the selected entry, and its already-indented/marked/truncated label
/// text. Split out from `Renderer::push_file_browser_panel` as a pure
/// function (no `wgpu` types) so the row layout — indent-by-depth,
/// `▼`/`▶` marker, scroll-offset windowing, width truncation — is
/// unit-testable without a GPU device.
struct FileBrowserRowLayout {
    visible_row: usize,
    is_selected: bool,
    label: String,
}

fn file_browser_panel_rows(
    state: &FileBrowserState,
    visible_rows: usize,
    max_chars: usize,
) -> Vec<FileBrowserRowLayout> {
    let offset = state.scroll_offset();
    let selected = state.selected_index();

    state
        .entries()
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible_rows)
        .map(|(row, entry)| {
            let marker = if entry.is_dir {
                if state.is_expanded(&entry.path) {
                    "▼ "
                } else {
                    "▶ "
                }
            } else {
                "  "
            };
            let name = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            let indent = "  ".repeat(entry.depth);
            let label: String = format!("{indent}{marker}{name}")
                .chars()
                .take(max_chars)
                .collect();

            FileBrowserRowLayout {
                visible_row: row - offset,
                is_selected: row == selected,
                label,
            }
        })
        .collect()
}

/// Screen-space placement + palette for the file-browser tree panel,
/// passed into `Renderer::render` when the panel is open. Built by the GUI
/// window layer (`ui::gui::window`) each frame from the shared
/// `app::file_browser::FileBrowserState` plus the active theme.
pub struct FileBrowserPanelView<'a> {
    pub state: &'a FileBrowserState,
    pub x: f32,
    pub width: f32,
    pub fg: Rgb,
    pub selected_bg: Rgb,
    pub selected_fg: Rgb,
}

/// Screen-space palette for the built-in text editor, passed into
/// `Renderer::render` when `ViewerContent::Editor` is active. Unlike
/// `FileBrowserPanelView` this always spans the full window rather than a
/// narrow strip (see phase 5 of `.claude/plans/inherited-painting-lark.md`
/// — a 30-column panel is unusably narrow for editing).
pub struct EditorPanelView<'a> {
    pub state: &'a EditorState,
    pub status: Option<&'a str>,
    pub fg: Rgb,
    pub bar_bg: Rgb,
    pub bar_fg: Rgb,
}

fn in_selection(sel: Selection, row: u16, col: u16) -> bool {
    let (start, end) =
        if sel.start.0 < sel.end.0 || (sel.start.0 == sel.end.0 && sel.start.1 <= sel.end.1) {
            (sel.start, sel.end)
        } else {
            (sel.end, sel.start)
        };
    if row < start.0 || row > end.0 {
        return false;
    }
    if start.0 == end.0 {
        return col >= start.1 && col <= end.1;
    }
    if row == start.0 {
        return col >= start.1;
    }
    if row == end.0 {
        return col <= end.1;
    }
    true
}

fn rgb_to_f32(color: Rgb, alpha: f32) -> [f32; 4] {
    [
        color.0 as f32 / 255.0,
        color.1 as f32 / 255.0,
        color.2 as f32 / 255.0,
        alpha,
    ]
}

#[allow(clippy::too_many_arguments)]
fn push_quad(
    out: &mut Vec<Vertex>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    color: [f32; 4],
    mode: f32,
) {
    let top_left = Vertex {
        pos: [x, y],
        uv: [uv_min[0], uv_min[1]],
        color,
        mode,
    };
    let top_right = Vertex {
        pos: [x + w, y],
        uv: [uv_max[0], uv_min[1]],
        color,
        mode,
    };
    let bottom_left = Vertex {
        pos: [x, y + h],
        uv: [uv_min[0], uv_max[1]],
        color,
        mode,
    };
    let bottom_right = Vertex {
        pos: [x + w, y + h],
        uv: [uv_max[0], uv_max[1]],
        color,
        mode,
    };
    out.extend_from_slice(&[
        top_left,
        bottom_left,
        top_right,
        top_right,
        bottom_left,
        bottom_right,
    ]);
}

fn viewport_uniform_bytes(width: f32, height: f32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    // std140 uniform blocks pad vec2<f32> to a 16-byte alignment boundary.
    bytes.extend_from_slice(&[0u8; 8]);
    bytes
}

/// Block the current thread on a `Future` using a throwaway single-thread
/// Tokio runtime. wgpu's native futures (adapter/device requests) resolve
/// synchronously under the hood, so this never actually parks on I/O — it's
/// just the simplest way to drive a `Future` to completion without adding a
/// dedicated executor crate (`tokio` is already a full dependency).
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build blocking executor for GPU init")
        .block_on(fut)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Headless GPU smoke test: create an `Instance` and try to acquire an
    /// adapter/device without a real display socket. The Docker toolchain
    /// image this runs in has no GPU drivers installed by default, so a
    /// missing adapter is not a test failure — only an adapter that exists
    /// but fails to hand back a device is.
    #[test]
    fn headless_instance_and_optional_device() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }));

        let Some(adapter) = adapter else {
            eprintln!("no GPU adapter available in this environment; skipping device request");
            return;
        };

        let device_result = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("casterm-headless-smoke-test"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ));
        assert!(
            device_result.is_ok(),
            "adapter was found but device request failed: {:?}",
            device_result.err()
        );
    }

    #[test]
    fn selection_range_single_row() {
        let sel = Selection {
            start: (2, 3),
            end: (2, 6),
        };
        assert!(!in_selection(sel, 2, 2));
        assert!(in_selection(sel, 2, 3));
        assert!(in_selection(sel, 2, 6));
        assert!(!in_selection(sel, 2, 7));
        assert!(!in_selection(sel, 3, 4));
    }

    #[test]
    fn selection_range_multi_row_normalizes_reversed_order() {
        let sel = Selection {
            start: (5, 2),
            end: (3, 8),
        };
        assert!(in_selection(sel, 3, 8));
        assert!(in_selection(sel, 4, 0));
        assert!(in_selection(sel, 5, 2));
        assert!(!in_selection(sel, 5, 3));
        assert!(!in_selection(sel, 3, 7));
    }

    /// Panel row-layout smoke test (Phase 3 of
    /// `.claude/plans/inherited-painting-lark.md`): builds a real
    /// `FileBrowserState` over a temp directory and asserts
    /// `file_browser_panel_rows` produces the expected windowed,
    /// selection-flagged, width-truncated labels — the same logic
    /// `Renderer::push_file_browser_panel` turns into quads, minus the
    /// `wgpu` device/queue calls that require a real GPU (see the
    /// `headless_instance_and_optional_device` test above for why a full
    /// `Renderer` can't be constructed in this environment: it needs a real
    /// `winit::Window` surface).
    #[test]
    fn file_browser_panel_rows_windows_and_flags_selection() {
        let dir = std::env::temp_dir().join(format!(
            "casterm-gui-renderer-panel-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        std::fs::write(dir.join("b.txt"), b"x").unwrap();
        std::fs::write(dir.join("c.txt"), b"x").unwrap();

        let mut state = FileBrowserState::new(dir.clone(), false);
        assert_eq!(state.entries().len(), 3);
        state.move_selection(1);

        // A 2-row window should show only the scrolled-into-view slice.
        state.set_scroll_offset(1);
        let rows = file_browser_panel_rows(&state, 2, 80);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].visible_row, 0);
        assert!(
            rows[0].is_selected,
            "selected entry (index 1) is row 0 of the 1-offset window"
        );
        assert!(!rows[1].is_selected);

        // A narrow panel truncates the label to `max_chars`.
        state.set_scroll_offset(0);
        let narrow = file_browser_panel_rows(&state, 3, 3);
        assert!(narrow.iter().all(|r| r.label.chars().count() <= 3));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Editor row-layout smoke test (Phase 5 of
    /// `.claude/plans/inherited-painting-lark.md`): builds a real
    /// `EditorState` over a temp file and asserts `editor_panel_rows`
    /// produces the expected scrolled-into-view, width-truncated lines —
    /// the same logic `Renderer::push_editor_panel` turns into quads,
    /// minus the `wgpu` calls. Mirrors
    /// `file_browser_panel_rows_windows_and_flags_selection` above.
    #[test]
    fn editor_panel_rows_scrolls_to_cursor_and_truncates() {
        let path = std::env::temp_dir().join(format!(
            "casterm-gui-renderer-editor-test-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "line0\nline1\nline2\nline3\nline4").unwrap();

        let mut editor = EditorState::load(path.clone()).unwrap();
        assert_eq!(editor.lines().len(), 5);

        // With a 2-row window and the cursor still at row 0, no scroll.
        let rows = editor_panel_rows(&editor, 2, 80);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "line0");
        assert_eq!(rows[1].text, "line1");

        // Moving the cursor past the window scrolls it into view.
        editor.move_cursor(4, 0);
        let scrolled = editor_panel_rows(&editor, 2, 80);
        assert_eq!(scrolled.len(), 2);
        assert_eq!(scrolled[0].visible_row, 0);
        assert_eq!(scrolled[0].text, "line3");
        assert_eq!(scrolled[1].text, "line4");

        // A narrow window truncates each line to `max_chars`.
        editor.move_cursor(-4, 0);
        let narrow = editor_panel_rows(&editor, 5, 3);
        assert!(narrow.iter().all(|r| r.text.chars().count() <= 3));

        std::fs::remove_file(&path).unwrap();
    }
}
