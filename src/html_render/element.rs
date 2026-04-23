use std::any::Any;
use std::sync::Arc;

use gpui::*;
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions, Scene};
use wgpu::{self, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages};

use super::document::prepare_document;

pub struct HtmlEmailView {
    content_height: f32,
    texture: Option<Arc<wgpu::Texture>>,
    texture_size: Size<DevicePixels>,
    preparing: bool,
    html: String,
}

impl HtmlEmailView {
    pub fn new(html: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let gpu_ctx = window.gpu_context();
        let mut view = Self {
            html,
            content_height: 0.0,
            texture: None,
            texture_size: size(DevicePixels(0), DevicePixels(0)),
            preparing: false,
        };
        view.start_prepare(gpu_ctx, cx);
        view
    }

    fn start_prepare(
        &mut self,
        gpu_ctx: Option<Box<dyn Any>>,
        cx: &mut Context<Self>,
    ) {
        if self.preparing || self.html.is_empty() {
            return;
        }
        self.preparing = true;

        let html = self.html.clone();
        let viewport_width = 640u32;
        let scale = 1.0f64;

        // Extract the wgpu device/queue from gpui
        let Some(ctx) = gpu_ctx else {
            tracing::warn!("No GPU context available for HTML rendering");
            self.preparing = false;
            return;
        };
        let (device, queue) = *ctx
            .downcast::<(Arc<wgpu::Device>, Arc<wgpu::Queue>)>()
            .expect("gpu_context should be (Arc<wgpu::Device>, Arc<wgpu::Queue>)");

        cx.spawn(async move |this, cx| {
            // Step 1: Prepare blitz document AND build vello scene on background thread.
            // BaseDocument is !Send, so both must happen on the same thread.
            let (scene, render_width, render_height, content_height) = cx
                .background_executor()
                .spawn(async move {
                    let prepared = prepare_document(&html, viewport_width, scale);
                    let content_height = prepared.content_height;
                    let render_height = (content_height.max(1.0) as u32).min(8192);
                    let render_width = prepared.viewport_width;

                    // Paint the blitz document into a vello Scene
                    let mut scene = Scene::new();
                    {
                        let mut painter = anyrender_vello::VelloScenePainter::new(&mut scene);
                        blitz_paint::paint_scene(
                            &mut painter,
                            &prepared.doc,
                            prepared.scale,
                            render_width,
                            render_height,
                            0,
                            0,
                        );
                    }

                    (scene, render_width, render_height, content_height)
                })
                .await;

            // Step 2: Create vello renderer, GPU texture, and render the scene.
            // This uses gpui's wgpu device so the texture can be displayed directly.
            let mut vello_renderer = vello::Renderer::new(
                &device,
                RendererOptions {
                    use_cpu: false,
                    num_init_threads: None,
                    antialiasing_support: AaSupport::area_only(),
                    pipeline_cache: None,
                },
            )
            .expect("Failed to create vello renderer");

            let texture = device.create_texture(&TextureDescriptor {
                label: Some("html_email_texture"),
                size: wgpu::Extent3d {
                    width: render_width,
                    height: render_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            });

            let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            vello_renderer
                .render_to_texture(
                    &device,
                    &queue,
                    &scene,
                    &texture_view,
                    &RenderParams {
                        base_color: Color::WHITE,
                        width: render_width,
                        height: render_height,
                        antialiasing_method: AaConfig::Area,
                    },
                )
                .expect("vello render_to_texture failed");

            let texture = Arc::new(texture);
            let texture_size = size(
                DevicePixels(render_width as i32),
                DevicePixels(render_height as i32),
            );

            // Step 3: Store texture and notify for redraw
            let _ = this.update(cx, |this, cx| {
                this.texture = Some(texture);
                this.texture_size = texture_size;
                this.content_height = content_height;
                this.preparing = false;
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for HtmlEmailView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(texture) = &self.texture {
            div()
                .w_full()
                .h(px(self.content_height))
                .child(
                    surface(SurfaceSource::Texture {
                        texture: texture.clone(),
                        size: self.texture_size,
                    })
                    .h(px(self.content_height))
                    .w_full()
                    .object_fit(ObjectFit::Fill),
                )
        } else {
            div().size_full()
        }
    }
}
