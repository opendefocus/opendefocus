//! The OFX Render action: reads the render window and settings, checks the Source, Depth and Output
//! images, and draws the window through the Adaptive WGPU renderer.

use super::{
    Array2, CLIP_DEPTH, CLIP_OUTPUT, CLIP_SOURCE, COMPONENTS_ALPHA, COMPONENTS_RGBA, DefocusMode,
    Failure, HostSuites, Image, Instance, OFX_IMAGE_CLIP_PROP_CONNECTED, OFX_PROP_TIME,
    OFX_STAT_FAILED, OFX_STAT_OK, OfxImageClipHandle, OfxImageEffectHandle, OfxPropertySetHandle,
    OfxStatus, OfxTime, Outcome, Props, Rect, RenderSpecs, Settings, check_abort, check_image,
    clear_window, clip_handles, clip_pixel_aspect, fail, fetch_image, instance_mut, intersect,
    read_effect_settings, read_window, reads_remote_source, rect_text, render_scale, render_window,
    report, s, set_frame, source_request, stored_suites, top_down, with_context, write_window,
};

/// The Render action. Returns `kOfxStatFailed` on any error; never unwinds.
pub fn render(effect: OfxImageEffectHandle, in_args: OfxPropertySetHandle) -> OfxStatus {
    match try_render(effect, in_args) {
        Ok(()) => OFX_STAT_OK,
        Err(Failure::Aborted) => OFX_STAT_FAILED,
        Err(Failure::Message(text)) => {
            report(effect, &text);
            OFX_STAT_FAILED
        }
    }
}
/// Performs one Render call and returns the first failure. Checks the in-args, the window and the
/// connected Depth clip, then draws the window. On failure the output window is cleared.
pub(super) fn try_render(
    effect: OfxImageEffectHandle,
    in_args: OfxPropertySetHandle,
) -> Outcome<()> {
    if in_args.is_null() {
        return fail("render arguments are missing");
    }
    let suites = stored_suites()?;
    let in_props = Props::new(in_args, suites.property);
    let window = render_window(&in_props)?;
    if window.width() == 0 || window.height() == 0 {
        return fail("render window is empty");
    }
    let time = in_props.get_double(OFX_PROP_TIME, 0)?;
    let (scale_x, scale_y) = render_scale(&in_props);
    let instance = instance_mut(&suites, effect)?;
    // SAFETY: `effect` and `in_props` are this action's handles, valid for the call.
    let mut settings = unsafe { read_effect_settings(&suites, effect, &in_props, time) }?;
    let depth_mode = settings.defocus.defocus_mode() == DefocusMode::Depth;

    let (depth_clip, depth_props) = clip_handles(&suites, effect, CLIP_DEPTH)?;
    let depth_connected =
        Props::new(depth_props, suites.property).get_int(OFX_IMAGE_CLIP_PROP_CONNECTED, 0)? != 0;
    if depth_mode && !depth_connected {
        return fail("Depth clip is not connected");
    }

    let (source_clip, source_props) = clip_handles(&suites, effect, CLIP_SOURCE)?;
    let (output_clip, _) = clip_handles(&suites, effect, CLIP_OUTPUT)?;
    let par = clip_pixel_aspect(source_props, &suites);

    let source = fetch_image(&suites, source_clip, time, "Source")?;
    // Fetched before any check, so a failure below can clear the window the host gave us.
    let mut output = fetch_image(&suites, output_clip, time, "Output")?;
    let result = render_frame(
        &suites,
        effect,
        instance,
        &mut settings,
        &source,
        FrameInputs {
            depth_mode,
            depth_clip,
            time,
            window,
            par,
            scale: (scale_x, scale_y),
        },
        &mut output,
    );
    if result.is_err() {
        // A failed render must not leave the window to whatever the host had in memory.
        clear_window(&mut output, window);
    }
    result
}
/// The inputs of [`render_frame`] besides the Source and Output images.
#[derive(Clone, Copy)]
pub(super) struct FrameInputs {
    /// Whether the Depth clip drives the defocus.
    pub(super) depth_mode: bool,
    /// Handle of the Depth clip; only fetched when `depth_mode` is set.
    pub(super) depth_clip: OfxImageClipHandle,
    /// Render time, passed to every image fetch.
    pub(super) time: OfxTime,
    /// Output window in full-frame pixel coordinates.
    pub(super) window: Rect,
    /// Pixel aspect ratio of the Source clip.
    pub(super) par: f64,
    /// Render scale in x and y, from the render-scale in-args.
    pub(super) scale: (f64, f64),
}

/// The checks and the render of one Render call, after the Source and Output are fetched.
#[allow(
    clippy::too_many_arguments,
    reason = "the render step takes the independent host handles, instance state and frame inputs it needs"
)]
pub(super) fn render_frame(
    suites: &HostSuites,
    effect: OfxImageEffectHandle,
    instance: &mut Instance,
    settings: &mut Settings,
    source: &Image<'_>,
    inputs: FrameInputs,
    output: &mut Image<'_>,
) -> Outcome<()> {
    let FrameInputs {
        depth_mode,
        depth_clip,
        time,
        window,
        par,
        scale: (scale_x, scale_y),
    } = inputs;
    // The frame is the Source region of definition, in pixels from the image. The window must
    // lie inside it, because the renderer's frame coordinates are defined there.
    let rod = source.region_of_definition;
    let context = format!(
        "(window {}, Source region of definition {}, Source bounds {}, render scale {scale_x} {scale_y}, PAR {par})",
        rect_text(window),
        rect_text(rod),
        rect_text(source.bounds)
    );
    if !rod.contains(window) {
        return fail(format!(
            "render window must lie inside the Source region of definition {context}"
        ));
    }
    set_frame(settings, rod);
    check_image(source, "Source", window, &[COMPONENTS_RGBA])
        .map_err(|e| with_context(e, &context))?;
    let depth_image = if depth_mode {
        let image = fetch_image(suites, depth_clip, time, "Depth")?;
        check_image(
            &image,
            "Depth",
            window,
            &[COMPONENTS_RGBA, COMPONENTS_ALPHA],
        )
        .map_err(|e| with_context(e, &context))?;
        Some(image)
    } else {
        None
    };
    draw(
        suites,
        effect,
        instance,
        settings,
        source,
        depth_image.as_ref(),
        output,
        Frame {
            window,
            rod,
            context: &context,
        },
    )
}

/// The geometry of one Render call, in pixels.
#[derive(Clone, Copy)]
pub(super) struct Frame<'a> {
    /// Output window in full-frame pixel coordinates.
    pub(super) window: Rect,
    /// Source region of definition; the frame the renderer's coordinates are defined in.
    pub(super) rod: Rect,
    /// Diagnostics appended to error messages.
    pub(super) context: &'a str,
}
/// Renders the window into `output`. The request is the window grown by the renderer's padding and
/// clamped to the region of definition and to the bounds of every input image, so a host image
/// that is a pixel smaller than the padded request still renders the window.
#[allow(
    clippy::too_many_arguments,
    reason = "the draw step takes the independent host handles, instance state and frame inputs it needs"
)]
pub(super) fn draw(
    suites: &HostSuites,
    effect: OfxImageEffectHandle,
    instance: &mut Instance,
    settings: &Settings,
    source: &Image<'_>,
    depth_image: Option<&Image<'_>>,
    output: &mut Image<'_>,
    frame: Frame<'_>,
) -> Outcome<()> {
    let Frame {
        window,
        rod,
        context,
    } = frame;
    check_image(output, "Output", window, &[COMPONENTS_RGBA])
        .map_err(|e| with_context(e, context))?;
    let renderer = instance.renderer(settings)?;
    // The renderer needs pristine source context around the window; see `source_request`.
    let padded = source_request(
        window,
        rod,
        renderer.padding(settings),
        reads_remote_source(settings),
    );
    let mut request = intersect(padded, source.bounds);
    if let Some(image) = depth_image {
        request = intersect(request, image.bounds);
    }
    if !request.contains(window) {
        return fail(format!(
            "Source image bounds do not cover the requested region {context}"
        ));
    }
    let source_channels = check_image(source, "Source", request, &[COMPONENTS_RGBA])
        .map_err(|e| with_context(e, context))?;
    let depth_channels = match depth_image {
        Some(image) => Some(
            check_image(
                image,
                "Depth",
                request,
                &[COMPONENTS_RGBA, COMPONENTS_ALPHA],
            )
            .map_err(|e| with_context(e, context))?,
        ),
        None => None,
    };
    check_abort(suites, effect)?;

    // SAFETY: each image was fetched in this action and its bounds cover the request, which is
    // only read here.
    let color = unsafe { read_window(source, request, source_channels) };
    let depth: Option<Array2<f32>> = match (depth_image, depth_channels) {
        // The first channel is used verbatim, like the CLI `--depth` input.
        (Some(image), Some(channels)) => {
            // SAFETY: as above; the depth image is fetched and its bounds cover the request.
            let values = unsafe { read_window(image, request, channels) };
            Some(values.slice(s![.., .., 0]).to_owned())
        }
        _ => None,
    };
    check_abort(suites, effect)?;

    // Frame coordinates follow the CLI `--render-mode striped` split: `color` covers the padded
    // request, and only `render_region` is written.
    let specs = RenderSpecs {
        full_region: top_down(request, rod),
        render_region: top_down(window, rod),
    };
    // The renderer works in place: the colour input goes in and the rendered result comes out.
    let mut result = color;
    pollster::block_on(renderer.render_stripe(
        specs,
        settings.clone(),
        result.view_mut(),
        depth,
        None,
    ))
    .map_err(|error| Failure::Message(format!("OpenDefocus render failed: {error} {context}")))?;
    check_abort(suites, effect)?;

    let top = (request.y2 - window.y2) as usize;
    let left = (window.x1 - request.x1) as usize;
    let rendered = result
        .slice(s![
            top..top + window.height(),
            left..left + window.width(),
            ..
        ])
        .to_owned();
    // SAFETY: the output image was checked to cover the window, and the host allows writes to
    // the output of a render.
    unsafe { write_window(output, window, &rendered) };
    Ok(())
}
