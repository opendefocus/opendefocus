struct Params {
    dimensions: vec4<u32>, // width, height, mode, output real depth values
    chunk: vec4<u32>,      // origin x/y, chunk width/height
    shape: vec4<f32>,      // radius, aspect ratio, angle, curvature
    ring: vec4<f32>,       // ring size, outer blur, inner blur, ring color
    color: vec4<f32>,      // inner color, blades, disc flag, reserved
};

@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<storage, read> depth: array<f32>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;

fn bokeh_pixel(x: u32, y: u32) -> f32 {
    let width = params.dimensions.x;
    let height = params.dimensions.y;
    let resolution = max(width, height);
    let center = vec2<f32>(f32(resolution) * 0.5);
    let radius_setting = params.shape.x;
    let outer_blur = params.ring.y;
    let radius = max((center * radius_setting - vec2<f32>(1.0) -
        (center * radius_setting - vec2<f32>(1.0)) * abs(outer_blur) * 0.3).x,
        (center * radius_setting - vec2<f32>(1.0) -
        (center * radius_setting - vec2<f32>(1.0)) * abs(outer_blur) * 0.3).y);
    let aspect = params.shape.y;
    let offset = vec2<f32>(3.0 - min(aspect, 1.0) * 2.0, max(aspect, 1.0) * 2.0 - 1.0);
    let position = vec2<f32>(f32(x), f32(y)) * offset;
    let relative = position - center;
    var angle = params.shape.z;
    if relative.x != 0.0 || relative.y != 0.0 {
        angle += atan2(relative.y, relative.x) * 57.2957;
    }
    let blades = u32(params.color.y);
    if blades == 0u {
        return 0.0;
    }
    let blade_degree = 360.0 / f32(blades);
    var blade_offset = angle - floor(angle / blade_degree) * blade_degree;
    blade_offset = abs(blade_offset / blade_degree - 0.5);
    var curvature = params.shape.w;
    if params.color.z > 0.5 {
        curvature = 1.0;
    }
    let radius_multiplier = (blade_offset - blade_offset * blade_offset) *
        (1.0 - min(curvature, 1.0)) * 2.0;
    let calculated_radius = max(radius - (radius / f32(blades)) * radius_multiplier, 0.0);
    let pixel_percentage = distance(position, center) / calculated_radius;
    let ring_range = 0.1 * params.ring.x;
    var ring_multiplier = 0.0;
    if 1.0 - pixel_percentage < ring_range && 1.0 - pixel_percentage > 0.0 {
        ring_multiplier = 1.0;
    }
    var inner_blur_multiplier = 0.0;
    let inner_blur = params.ring.z;
    if inner_blur != 0.0 && pixel_percentage < 1.0 {
        inner_blur_multiplier = clamp(pixel_percentage / (1.0 - ring_range), 0.0, 1.0);
        inner_blur_multiplier = clamp(
            (inner_blur_multiplier - (1.0 - inner_blur * 2.0)) /
                (1.0 - (1.0 - inner_blur * 2.0)),
            0.0,
            1.0,
        );
        inner_blur_multiplier *= inner_blur_multiplier;
    }
    var outer_blur_multiplier = 0.0;
    if outer_blur != 0.0 && pixel_percentage > 1.0 {
        outer_blur_multiplier = clamp(
            (pixel_percentage - (1.0 + abs(outer_blur) * 0.4)) /
                (1.0 - (1.0 + abs(outer_blur) * 0.4)),
            0.0,
            1.0,
        );
        outer_blur_multiplier *= outer_blur_multiplier;
    }
    ring_multiplier = ring_multiplier + inner_blur_multiplier - ring_multiplier * inner_blur_multiplier;
    ring_multiplier = ring_multiplier + outer_blur_multiplier - ring_multiplier * outer_blur_multiplier;
    var inner = 0.0;
    if pixel_percentage < 1.0 {
        inner = 1.0;
    }
    inner = max(inner - ring_multiplier, 0.0);
    return ring_multiplier * params.ring.w + inner * params.color.x -
        ring_multiplier * params.ring.w * inner * params.color.x;
}

fn focal_value(value: f32, channel: u32, output_real: bool) -> f32 {
    if output_real {
        if channel == 3u { return 1.0; }
        if channel == 0u { return select(0.0, abs(value), value > 0.0); }
        if channel == 1u { return select(0.0, 1.0, value == 0.0); }
        if channel == 2u { return select(0.0, abs(value), value < 0.0); }
    } else {
        if channel == 0u { return select(0.0, 1.0, value > 0.0); }
        if channel == 1u { return select(0.0, 1.0, value == 0.0); }
        if channel == 2u { return select(0.0, 1.0, value < 0.0); }
        if channel == 3u { return 0.5; }
    }
    return 0.0;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.chunk.z;
    let height = params.chunk.w;
    if id.x >= width || id.y >= height {
        return;
    }
    let x = id.x + params.chunk.x;
    let y = id.y + params.chunk.y;
    let pixel = id.y * width + id.x;
    let base = pixel * 4u;
    if params.dimensions.z == 0u {
        let value = bokeh_pixel(x, y);
        output[base] = value;
        output[base + 1u] = value;
        output[base + 2u] = value;
        output[base + 3u] = value;
    } else {
        let value = depth[pixel];
        let alpha = focal_value(value, 3u, params.dimensions.w != 0u);
        for (var channel = 0u; channel < 4u; channel += 1u) {
            let overlay = focal_value(value, channel, params.dimensions.w != 0u);
            output[base + channel] = overlay * alpha + source[base + channel] * (1.0 - alpha);
        }
    }
}
