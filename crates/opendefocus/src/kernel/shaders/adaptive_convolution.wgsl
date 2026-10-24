// Authored convolution for every Adaptive GPU quality tier.
// One explicit resource contract covers circular and spatial apertures.

struct AdaptiveGatherSettings {
    frame: vec4<u32>,
    tile_origin: vec2<u32>,
    source_opaque: u32,
    _tile_padding: u32,
    samples: vec4<u32>,
    radius: vec4<f32>,
};

struct ConvolveSettings {
    process_region: vec4<i32>,
    full_region: vec4<i32>,
    resolution: vec2<u32>,
    filter_resolution: vec2<u32>,
    center: vec2<f32>,
    flags: u32,
    non_uniform_flags: u32,
    samples: u32,
    pixel_aspect: f32,
    size: f32,
    max_size: f32,
    catseye_amount: f32,
    catseye_gamma: f32,
    catseye_softness: f32,
    astigmatism_amount: f32,
    astigmatism_gamma: f32,
    barndoors_amount: f32,
    barndoors_gamma: f32,
    barndoors_top: f32,
    barndoors_bottom: f32,
    barndoors_left: f32,
    barndoors_right: f32,
    filter_aspect_ratio_normalizer: f32,
    image_elements: u32,
    ring_distance: f32,
    filter_aspect_ratio: f32,
    pixel_aspect_normalizer: f32,
    render_scale: u32,
    axial_aberration_amount: f32,
    axial_aberration_type: i32,
    _padding: u32,
};

struct IntegralNode {
    weighted_premul: vec4<f32>,
    bounds: vec4<u32>,
    children: vec4<u32>,
    coc_min: f32,
    coc_max: f32,
    weighted_valid: f32,
    occupied_area: f32,
};

struct ApertureBounds {
    min: vec4<f32>,
    max: vec4<f32>,
};

struct LayerSample {
    color: vec4<f32>,
    coc: f32,
    valid: f32,
};

struct IntegralPixel {
    premul: vec4<f32>,
    coc: f32,
    valid: f32,
    occupied: f32,
};

struct LensIntegral {
    premul: vec4<f32>,
    valid: vec4<f32>,
    presence: vec4<f32>,
};

// Far sources whose CoC exceeds `probe` lie partly, and above `behind` wholly,
// behind the destination's own far surface (`far_behind_weight`). `state` is
// FAR_UNOCCLUDED, FAR_HIDDEN or FAR_PARTIAL; a partial occluder is the
// probe-disc model of radius `probe` described at FAR_PROBE_SCALE, with its
// edge softened by `softness` pixels.
struct FarOcclusion {
    state: u32,
    probe: f32,
    behind: f32,
    softness: f32,
    normal: vec2<f32>,
    edge: f32,
    one_sided: f32,
    open: f32,
};

struct LocalIntegral {
    premul: vec4<f32>,
    local_weight: vec4<f32>,
    support_weight: vec4<f32>,
    occupied_support: vec4<f32>,
};

struct ApertureRange {
    min: vec4<f32>,
    max: vec4<f32>,
    valid: bool,
};

struct NodeClass {
    empty_mask: u32,
    accept_mask: u32,
    coefficient: vec4<f32>,
};

struct NonUniformValue {
    opacity: f32,
    offset: vec2<f32>,
};

struct Interval {
    lo: f32,
    hi: f32,
};

@group(0) @binding(0) var<uniform> settings: AdaptiveGatherSettings;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;
@group(0) @binding(3) var source: texture_2d<f32>;
@group(0) @binding(4) var near_color: texture_2d<f32>;
@group(0) @binding(5) var far_color: texture_2d<f32>;
@group(0) @binding(6) var near_metadata: texture_2d<f32>;
@group(0) @binding(7) var far_metadata: texture_2d<f32>;
@group(0) @binding(8) var<storage, read> tile_classes: array<u32>;
@group(0) @binding(9) var<storage, read> sample_offsets: array<f32>;
@group(0) @binding(10) var<storage, read> near_search_radii: array<f32>;
@group(0) @binding(11) var<uniform> lens: ConvolveSettings;
@group(0) @binding(12) var<storage, read> near_nodes: array<IntegralNode>;
@group(0) @binding(13) var<storage, read> far_nodes: array<IntegralNode>;
@group(0) @binding(14) var aperture: texture_2d<f32>;
@group(0) @binding(15) var<storage, read> aperture_bounds: array<ApertureBounds>;
@group(0) @binding(16) var<storage, read> subtree_ends: array<u32>;

override PLAIN_CIRCLE: bool = false;
override FUSED_CAPPED_CIRCLE: bool = false;

const EMPTY_REF: u32 = 0xffffffffu;
const LEAF_REF_BIT: u32 = 0x80000000u;
const CHANNEL_MASK: u32 = 0x0fu;
const GLOBAL_SIMPLE_DISC: u32 = 0x02u;
const GLOBAL_USE_NON_UNIFORM: u32 = 0x08u;
const GLOBAL_INVERSE_FOREGROUND: u32 = 0x10u;
const GLOBAL_AXIAL_ABERRATION: u32 = 0x20u;
const GLOBAL_INVERSE_Y: u32 = 0x40u;

fn component(value: vec4<f32>, channel: u32) -> f32 {
    if channel == 0u { return value.x; }
    if channel == 1u { return value.y; }
    if channel == 2u { return value.z; }
    return value.w;
}

fn set_component(value: ptr<function, vec4<f32>>, channel: u32, x: f32) {
    if channel == 0u { (*value).x = x; }
    else if channel == 1u { (*value).y = x; }
    else if channel == 2u { (*value).z = x; }
    else { (*value).w = x; }
}

fn add_component(value: ptr<function, vec4<f32>>, channel: u32, x: f32) {
    if channel == 0u { (*value).x += x; }
    else if channel == 1u { (*value).y += x; }
    else if channel == 2u { (*value).z += x; }
    else { (*value).w += x; }
}

// Returns `value` with one channel replaced. Used instead of `set_component` on a struct
// member: naga 30.0.1's SPIR-V writer panics on pointers to struct members passed as call
// arguments.
fn with_component(value: vec4<f32>, channel: u32, x: f32) -> vec4<f32> {
    var updated = value;
    set_component(&updated, channel, x);
    return updated;
}

// Returns `value` with `x` added to one channel; see `with_component` for why this exists.
fn with_added_component(value: vec4<f32>, channel: u32, x: f32) -> vec4<f32> {
    var updated = value;
    add_component(&updated, channel, x);
    return updated;
}

fn channel_mask_vec(mask: u32) -> vec4<f32> {
    return vec4<f32>(
        select(0.0, 1.0, (mask & 1u) != 0u),
        select(0.0, 1.0, (mask & 2u) != 0u),
        select(0.0, 1.0, (mask & 4u) != 0u),
        select(0.0, 1.0, (mask & 8u) != 0u),
    );
}

fn texture_uv(position: vec2<f32>) -> vec2<f32> {
    return position / vec2<f32>(settings.frame.xy);
}

fn sample_layer(position: vec2<f32>, sample_radius: f32, sample_count: u32, near: bool) -> LayerSample {
    if position.x < 0.5 || position.y < 0.5 ||
       position.x > f32(settings.frame.x) - 0.5 || position.y > f32(settings.frame.y) - 0.5 {
        return LayerSample(vec4<f32>(0.0), 0.0, 0.0);
    }
    let uv = texture_uv(position);
    let samples_per_axis = select(1.0, select(8.0, 16.0, sample_count >= 256u), sample_count >= 64u);
    let footprint = max(sample_radius / samples_per_axis, 1.0);
    let lod = clamp(log2(footprint), 0.0, settings.radius.y);
    let lower_lod = floor(lod);
    let upper_lod = min(ceil(lod), settings.radius.y);
    let lod_fraction = select(0.0, fract(lod), upper_lod > lower_lod);
    var base_meta: vec4<f32>;
    if near { base_meta = textureSampleLevel(near_metadata, linear_sampler, uv, 0.0); }
    else { base_meta = textureSampleLevel(far_metadata, linear_sampler, uv, 0.0); }
    if lower_lod == 0.0 && lod_fraction == 0.0 && clamp(base_meta.y, 0.0, 1.0) <= 0.0 {
        return LayerSample(vec4<f32>(0.0), 0.0, 0.0);
    }
    var lower_color: vec4<f32>;
    var upper_color: vec4<f32>;
    var lower_meta: vec4<f32>;
    var upper_meta: vec4<f32>;
    if near {
        lower_color = textureSampleLevel(near_color, linear_sampler, uv, lower_lod);
        upper_color = textureSampleLevel(near_color, linear_sampler, uv, upper_lod);
        lower_meta = textureSampleLevel(near_metadata, linear_sampler, uv, lower_lod);
        upper_meta = textureSampleLevel(near_metadata, linear_sampler, uv, upper_lod);
    } else {
        lower_color = textureSampleLevel(far_color, linear_sampler, uv, lower_lod);
        upper_color = textureSampleLevel(far_color, linear_sampler, uv, upper_lod);
        lower_meta = textureSampleLevel(far_metadata, linear_sampler, uv, lower_lod);
        upper_meta = textureSampleLevel(far_metadata, linear_sampler, uv, upper_lod);
    }
    let color_value = mix(lower_color, upper_color, lod_fraction);
    let metadata_value = mix(lower_meta, upper_meta, lod_fraction);
    let valid = clamp(metadata_value.y, 0.0, 1.0);
    if valid <= 0.0 { return LayerSample(vec4<f32>(0.0), 0.0, 0.0); }
    return LayerSample(color_value / valid, metadata_value.x / valid, valid);
}

fn axial_radii(coc: f32) -> vec4<f32> {
    if PLAIN_CIRCLE || (lens.flags & GLOBAL_AXIAL_ABERRATION) == 0u {
        return vec4<f32>(coc);
    }
    let amount = lens.axial_aberration_amount;
    let corrected = abs(amount * 0.25);
    let polarity = select(-1.0, 1.0, coc < 0.0);
    var offsets = vec3<f32>(0.0);
    switch lens.axial_aberration_type {
        case 1: {
            if polarity * amount > 0.0 { offsets.x = corrected; }
            offsets.y = corrected * 0.5;
            if polarity * amount < 0.0 { offsets.z = corrected; }
        }
        case 2: {
            if polarity * amount < 0.0 { offsets.x = corrected; offsets.y = corrected; }
            if polarity * amount > 0.0 { offsets.z = corrected; }
        }
        case 3: {
            if polarity * amount > 0.0 { offsets.x = corrected; offsets.z = corrected; }
            if polarity * amount < 0.0 { offsets.y = corrected * 0.5; }
        }
        default: {
            if polarity * amount > 0.0 { offsets.x = corrected; }
            offsets.y = corrected * 0.5;
            if polarity * amount < 0.0 { offsets.z = corrected; }
        }
    }
    return vec4<f32>(coc * (1.0 - offsets), coc);
}

fn axial_scale(coc_sign: f32, channel: u32) -> f32 {
    return component(axial_radii(coc_sign), channel) / coc_sign;
}

fn signed_pow(value: f32, exponent: f32) -> f32 {
    return sign(value) * pow(abs(value), exponent);
}

fn non_uniform(position: vec2<u32>, distance: vec2<f32>, coc: f32) -> NonUniformValue {
    var result = NonUniformValue(1.0, vec2<f32>(0.0));
    if PLAIN_CIRCLE || (lens.flags & GLOBAL_USE_NON_UNIFORM) == 0u { return result; }
    let flags = lens.non_uniform_flags;
    let aspect_scale = vec2<f32>(
        select(1.0, lens.pixel_aspect, lens.pixel_aspect > 1.0),
        select(1.0, lens.pixel_aspect_normalizer, lens.pixel_aspect < 1.0),
    );
    let corrected_distance = distance * aspect_scale;
    let normalizer = vec2<f32>(
        select(lens.center.x, f32(lens.resolution.x) * 0.5, lens.center.x == 0.0),
        select(lens.center.y, f32(lens.resolution.y) * 0.5, lens.center.y == 0.0),
    );
    let inverse_y = select(1.0, -1.0, (lens.flags & GLOBAL_INVERSE_Y) != 0u);
    let real_position = vec2<f32>(lens.full_region.xy) + vec2<f32>(position);
    let screen = (real_position + corrected_distance - lens.center) / normalizer * inverse_y;
    let kernel = corrected_distance / coc * 2.0;

    if (flags & 0x20u) != 0u {
        var multiplier = vec2<f32>(lens.barndoors_amount);
        multiplier *= clamp(abs(coc) * 0.2, 0.0, 1.0);
        multiplier.x *= select(lens.barndoors_left, lens.barndoors_right, screen.x < 0.0) * 0.02;
        multiplier.y *= select(lens.barndoors_bottom, lens.barndoors_top, screen.y < 0.0) * 0.02;
        let screen_gamma = vec2<f32>(
            signed_pow(screen.x, 1.0 / lens.barndoors_gamma),
            signed_pow(screen.y, 1.0 / lens.barndoors_gamma),
        );
        var inverse = select(-1.0, 1.0, (flags & 0x40u) != 0u);
        if coc > 0.0 && (flags & 0x80u) != 0u { inverse = -inverse; }
        let positive = select(vec2<f32>(-1.0), vec2<f32>(1.0), screen >= vec2<f32>(0.0));
        let edge = vec2<f32>(1.0) - (inverse * kernel + (multiplier - 0.5) * positive) * screen_gamma;
        result.opacity *= select(0.0, 1.0, edge.x > 1.0e-5 && edge.y > 1.0e-5);
    }

    if (flags & 0x10u) != 0u {
        let delta = lens.center - real_position;
        var rotation = vec4<f32>(0.0, 1.0, 0.0, 1.0);
        if dot(delta, delta) > 0.0 {
            let inv_length = inverseSqrt(dot(delta, delta));
            if real_position.y < lens.center.y {
                rotation.x = -delta.x * inv_length;
                rotation.y = abs(delta.y) * inv_length;
            } else {
                rotation.x = delta.x * inv_length;
                rotation.y = -delta.y * inv_length;
            }
            if real_position.x < lens.center.x {
                rotation.z = delta.y * inv_length;
                rotation.w = delta.x * inv_length;
            } else {
                rotation.z = -delta.y * inv_length;
                rotation.w = -delta.x * inv_length;
            }
        }
        let rotated = vec2<f32>(
            kernel.x * rotation.y + kernel.y * rotation.x,
            -kernel.x * rotation.z + kernel.y * rotation.w,
        ) * (coc * 0.4 * lens.astigmatism_amount);
        let offset = vec2<f32>(
            rotated.x * pow(abs(screen.y), 1.0 / lens.astigmatism_gamma),
            rotated.y * pow(abs(screen.x), 1.0 / lens.astigmatism_gamma),
        );
        if any(real_position - screen != vec2<f32>(0.0)) {
            result.offset += offset;
        }
    }

    if (flags & 0x01u) != 0u {
        var center = screen;
        var inverse = select(1.0, -1.0, coc > 0.0 && (flags & 0x02u) != 0u);
        inverse *= select(1.0, -1.0, coc > 0.0 && (flags & 0x04u) != 0u);
        center *= clamp(abs(coc * 0.2), 0.0, 1.0);
        let normalized_y = f32(lens.resolution.y) * lens.pixel_aspect_normalizer;
        let screen_relative = (flags & 0x08u) != 0u;
        let division = vec2<f32>(
            select(1.0, normalized_y / f32(lens.resolution.x), screen_relative && f32(lens.resolution.x) > normalized_y),
            select(1.0, f32(lens.resolution.x) / normalized_y, screen_relative && f32(lens.resolution.x) < normalized_y),
        );
        let exponent = division / lens.catseye_gamma;
        if exponent.x != 1.0 { center.x = signed_pow(center.x, exponent.x); }
        if exponent.y != 1.0 { center.y = signed_pow(center.y, exponent.y); }
        let aperture_center = kernel + inverse * center * lens.catseye_amount;
        var mask = -(dot(aperture_center, aperture_center) * 0.25 - 1.0);
        if lens.catseye_softness == 0.0 { mask = select(0.0, 1.0, mask > 0.0); }
        else { mask = clamp(mask / (lens.catseye_softness * 0.5), 0.0, 1.0); }
        result.opacity *= mask;
    }
    return result;
}

fn aperture_sample(radius: f32, distance: vec2<f32>, footprint: f32) -> vec4<f32> {
    if PLAIN_CIRCLE || (lens.flags & GLOBAL_SIMPLE_DISC) != 0u {
        var sample_distance = distance;
        let lens_disk = (lens.flags & (GLOBAL_USE_NON_UNIFORM | GLOBAL_AXIAL_ABERRATION)) != 0u;
        let support_radius = max(abs(radius), 0.5);
        if radius > 0.0 && (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u &&
           lens.filter_resolution.x > 0u && lens.filter_resolution.y > 0u {
            let phase_radius = select(radius, support_radius, lens_disk);
            sample_distance += 2.0 * phase_radius / vec2<f32>(lens.filter_resolution);
        } else if !lens_disk && radius > 0.0 {
            sample_distance += settings.radius.zw * radius;
        }
        return vec4<f32>(clamp((support_radius - length(sample_distance)) / max(footprint, 1.0) + 0.5, 0.0, 1.0));
    }
    let resolution = vec2<f32>(lens.filter_resolution);
    let diameter = max(abs(radius) * 2.0, 1.0);
    var coordinates = -distance * (resolution / diameter) + resolution * 0.5;
    if radius > 0.0 && (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u {
        coordinates -= vec2<f32>(1.0);
    }
    let uv = coordinates / resolution;
    let normalized = (uv - vec2<f32>(0.5)) * 2.0;
    if dot(normalized, normalized) >= 1.0 { return vec4<f32>(0.0); }
    let max_scale = max(diameter / resolution.x, diameter / resolution.y);
    let lod = max(log2(1.0 / max_scale), 0.0);
    return textureSampleLevel(aperture, linear_sampler, uv, lod);
}

fn aperture_weight(position: vec2<u32>, distance: vec2<f32>, coc: f32, search_radius: f32) -> vec4<f32> {
    return aperture_weight_with_footprint(position, distance, coc, search_radius, 1.0);
}
fn aperture_weight_with_footprint(position: vec2<u32>, distance: vec2<f32>, coc: f32, search_radius: f32, footprint: f32) -> vec4<f32> {
    if PLAIN_CIRCLE {
        let kernel = aperture_sample(coc, distance, footprint).x;
        let weight = kernel * search_radius * search_radius / max(coc * coc, 0.25);
        return vec4<f32>(weight);
    }
    let non_uniform_value = non_uniform(position, distance, coc);
    let transformed_distance = distance + non_uniform_value.offset;
    let radii = axial_radii(coc);
    var kernels = vec4<f32>(0.0);
    if all(radii == vec4<f32>(radii.x)) {
        kernels = aperture_sample(radii.x, transformed_distance, footprint);
    } else {
        kernels.x = aperture_sample(radii.x, transformed_distance, footprint).x;
        kernels.y = aperture_sample(radii.y, transformed_distance, footprint).y;
        kernels.z = aperture_sample(radii.z, transformed_distance, footprint).z;
        kernels.w = aperture_sample(radii.w, transformed_distance, footprint).w;
    }
    var result = vec4<f32>(0.0);
    for (var channel = 0u; channel < 4u; channel++) {
        let radius = abs(component(radii, channel));
        let value = component(kernels, channel) * search_radius * search_radius / max(radius * radius, 0.25);
        set_component(&result, channel, value);
    }
    return result * non_uniform_value.opacity;
}

fn ceil_log2(value: u32) -> u32 {
    if value <= 1u { return 0u; }
    return 32u - countLeadingZeros(value - 1u);
}

const APERTURE_LAYOUT_LEVELS: u32 = 16u;

fn filter_range(uv_min: vec2<f32>, uv_max: vec2<f32>, lod_min: f32, lod_max: f32) -> ApertureRange {
    let invalid = ApertureRange(vec4<f32>(0.0), vec4<f32>(0.0), false);
    if any(uv_min > uv_max) || lod_min > lod_max { return invalid; }
    let mip_count = textureNumLevels(aperture);
    if mip_count == 0u { return invalid; }
    let bounds_length = arrayLength(&aperture_bounds);
    let layout_length = mip_count * APERTURE_LAYOUT_LEVELS;
    if bounds_length < layout_length { return invalid; }
    let layout_base = bounds_length - layout_length;
    let first_mip = min(u32(max(floor(lod_min), 0.0)), mip_count - 1u);
    let last_mip = min(u32(max(ceil(lod_max), 0.0)), mip_count - 1u);
    var range_min = vec4<f32>(3.402823e38);
    var range_max = vec4<f32>(-3.402823e38);
    var has_values = false;
    var mip = first_mip;
    loop {
        let resolution = textureDimensions(aperture, mip);
        let texel_min = uv_min * vec2<f32>(resolution) - vec2<f32>(0.5);
        let texel_max = uv_max * vec2<f32>(resolution) - vec2<f32>(0.5);
        let x0 = u32(clamp(i32(floor(texel_min.x)), 0, i32(resolution.x) - 1));
        let y0 = u32(clamp(i32(floor(texel_min.y)), 0, i32(resolution.y) - 1));
        let x1 = u32(clamp(i32(ceil(texel_max.x)), 0, i32(resolution.x) - 1));
        let y1 = u32(clamp(i32(ceil(texel_max.y)), 0, i32(resolution.y) - 1));
        let extent = max(x1 - x0 + 1u, y1 - y0 + 1u);
        let mip_layout_index = layout_base + mip * APERTURE_LAYOUT_LEVELS;
        let mip_layout = bitcast<vec4<u32>>(aperture_bounds[mip_layout_index].min);
        if mip_layout.w == 0u { return invalid; }
        let level = min(ceil_log2((extent + 1u) / 2u), mip_layout.w - 1u);
        let bounds_layout = bitcast<vec4<u32>>(aperture_bounds[mip_layout_index + level].min);
        let scale = bounds_layout.z;
        let min_x = x0 / scale;
        let min_y = y0 / scale;
        let max_x = x1 / scale;
        let max_y = y1 / scale;
        var y = min_y;
        while y <= max_y {
            var x = min_x;
            while x <= max_x {
                let index = bounds_layout.x + y * bounds_layout.y + x;
                if index >= layout_base { return invalid; }
                let bounds = aperture_bounds[index];
                range_min = min(range_min, bounds.min);
                range_max = max(range_max, bounds.max);
                has_values = true;
                x += 1u;
            }
            y += 1u;
        }
        if mip == last_mip { break; }
        mip += 1u;
    }
    return ApertureRange(range_min, range_max, has_values);
}

fn ratio_interval(numerator: Interval, denominator: Interval) -> Interval {
    let a = numerator.lo / denominator.lo;
    let b = numerator.lo / denominator.hi;
    let c = numerator.hi / denominator.lo;
    let d = numerator.hi / denominator.hi;
    return Interval(min(min(a, b), min(c, d)), max(max(a, b), max(c, d)));
}

fn node_weight_scale(coc_min: f32, coc_max: f32, channel_scale: f32) -> f32 {
    if channel_scale == 0.0 { return 0.0; }
    let channel_min = coc_min * abs(channel_scale);
    if channel_min >= 0.5 { return 1.0 / (channel_scale * channel_scale); }
    if coc_min == coc_max {
        let channel_radius = coc_min * abs(channel_scale);
        return coc_min * coc_min / max(channel_radius * channel_radius, 0.25);
    }
    return 0.0;
}

struct LensBounds {
    distance: vec4<f32>,
    opacity: u32,
};

// Interval arithmetic bounds a region; WGSL built-ins evaluate the leaf math.
fn widen_box(value: vec4<f32>) -> vec4<f32> {
    let margin = max(max(abs(value.xy), abs(value.zw)), vec2<f32>(1.0)) * (16.0 * 1.1920929e-7);
    return vec4<f32>(value.xy - margin, value.zw + margin);
}
fn scale_box(value: vec4<f32>, factor: vec2<f32>) -> vec4<f32> {
    return widen_box(vec4<f32>(min(value.xy * factor, value.zw * factor), max(value.xy * factor, value.zw * factor)));
}
fn multiply_box(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    let p00 = a.xy * b.xy;
    let p01 = a.xy * b.zw;
    let p10 = a.zw * b.xy;
    let p11 = a.zw * b.zw;
    return widen_box(vec4<f32>(min(min(p00, p01), min(p10, p11)), max(max(p00, p01), max(p10, p11))));
}
fn add_box(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    return widen_box(a + b);
}
fn power_box(value: vec4<f32>, exponent: vec2<f32>) -> vec4<f32> {
    let lo = select(sign(value.xy) * pow(abs(value.xy), exponent), value.xy, exponent == vec2<f32>(1.0));
    let hi = select(sign(value.zw) * pow(abs(value.zw), exponent), value.zw, exponent == vec2<f32>(1.0));
    return widen_box(vec4<f32>(lo, hi));
}
fn squared_box_range(value: vec4<f32>) -> vec2<f32> {
    let closest = max(max(value.xy, vec2<f32>(0.0)), -value.zw);
    let farthest = max(abs(value.xy), abs(value.zw));
    return vec2<f32>(dot(closest, closest), dot(farthest, farthest));
}
fn combine_opacity(a: u32, b: u32) -> u32 {
    if a == 0u || b == 0u { return 0u; }
    if a == 1u && b == 1u { return 1u; }
    return 2u;
}
fn lens_bounds(bounds: vec4<u32>, destination: vec2<u32>, radius_min: f32, radius_max: f32, sign_value: f32) -> LensBounds {
    let raw = widen_box(vec4<f32>(vec2<f32>(bounds.xy) - vec2<f32>(destination), vec2<f32>(bounds.zw - vec2<u32>(1u)) - vec2<f32>(destination)));
    var result = LensBounds(raw, 1u);
    let flags = lens.non_uniform_flags;
    let aspect = vec2<f32>(select(1.0, lens.pixel_aspect, lens.pixel_aspect > 1.0), select(1.0, lens.pixel_aspect_normalizer, lens.pixel_aspect < 1.0));
    let corrected = scale_box(raw, aspect);
    let normalizer = vec2<f32>(select(lens.center.x, f32(lens.resolution.x) * 0.5, lens.center.x == 0.0), select(lens.center.y, f32(lens.resolution.y) * 0.5, lens.center.y == 0.0));
    if any(normalizer == vec2<f32>(0.0)) || radius_min <= 0.0 {
        return LensBounds(vec4<f32>(-3.402823e38, -3.402823e38, 3.402823e38, 3.402823e38), 2u);
    }
    let real_position = vec2<f32>(lens.full_region.xy) + vec2<f32>(destination);
    let center_delta = real_position - lens.center;
    let inverse_y = select(1.0, -1.0, (lens.flags & GLOBAL_INVERSE_Y) != 0u);
    let screen = scale_box(add_box(corrected, vec4<f32>(center_delta, center_delta)), vec2<f32>(inverse_y) / normalizer);
    let inverse_radius = vec4<f32>(vec2<f32>(1.0 / radius_max), vec2<f32>(1.0 / radius_min));
    let kernel = scale_box(multiply_box(corrected, inverse_radius), vec2<f32>(2.0 * sign_value));
    let size_factor = vec2<f32>(clamp(radius_min * 0.2, 0.0, 1.0), clamp(radius_max * 0.2, 0.0, 1.0));
    if (flags & 0x01u) != 0u {
        if lens.catseye_gamma <= 0.0 || lens.catseye_softness < 0.0 {
            result.opacity = 2u;
        } else {
            let normalized_y = f32(lens.resolution.y) * lens.pixel_aspect_normalizer;
            let screen_relative = (flags & 0x08u) != 0u;
            let division = vec2<f32>(select(1.0, normalized_y / f32(lens.resolution.x), screen_relative && f32(lens.resolution.x) > normalized_y), select(1.0, f32(lens.resolution.x) / normalized_y, screen_relative && f32(lens.resolution.x) < normalized_y));
            var inverse = select(1.0, -1.0, sign_value > 0.0 && (flags & 0x02u) != 0u);
            inverse *= select(1.0, -1.0, sign_value > 0.0 && (flags & 0x04u) != 0u);
            let powered = power_box(multiply_box(screen, vec4<f32>(vec2<f32>(size_factor.x), vec2<f32>(size_factor.y))), division / lens.catseye_gamma);
            let mask_center = add_box(kernel, scale_box(powered, vec2<f32>(inverse * lens.catseye_amount)));
            let squared = squared_box_range(mask_center);
            let margin = max(max(squared.x, squared.y), 1.0) * (16.0 * 1.1920929e-7);
            var opacity = 2u;
            if squared.x >= 4.0 + margin { opacity = 0u; }
            else if squared.y < 4.0 - 2.0 * lens.catseye_softness - margin { opacity = 1u; }
            result.opacity = combine_opacity(result.opacity, opacity);
        }
    }
    if (flags & 0x20u) != 0u {
        if lens.barndoors_gamma <= 0.0 { result.opacity = combine_opacity(result.opacity, 2u); }
        else {
            let powered_screen = power_box(screen, vec2<f32>(1.0 / lens.barndoors_gamma));
            var inverse = select(-1.0, 1.0, (flags & 0x40u) != 0u);
            if sign_value > 0.0 && (flags & 0x80u) != 0u { inverse = -inverse; }
            for (var axis = 0u; axis < 2u; axis++) {
                var opacity = 2u;
                if screen.xy[axis] >= 0.0 || screen.zw[axis] < 0.0 {
                    let negative = screen.zw[axis] < 0.0;
                    let side = select(select(lens.barndoors_left, lens.barndoors_right, negative), select(lens.barndoors_bottom, lens.barndoors_top, negative), axis == 1u);
                    let screen_sign = select(1.0, -1.0, negative);
                    let amount = size_factor * (lens.barndoors_amount * side * 0.02);
                    let offset = (amount - vec2<f32>(0.5)) * screen_sign;
                    let off = vec2<f32>(min(offset.x, offset.y), max(offset.x, offset.y));
                    let k = vec2<f32>(kernel.xy[axis], kernel.zw[axis]) * inverse;
                    let kr = vec2<f32>(min(k.x, k.y) + off.x, max(k.x, k.y) + off.y);
                    let sr = vec2<f32>(powered_screen.xy[axis], powered_screen.zw[axis]);
                    let products = vec4<f32>(kr.x * sr.x, kr.x * sr.y, kr.y * sr.x, kr.y * sr.y);
                    let edge = vec2<f32>(1.0 - max(max(products.x, products.y), max(products.z, products.w)), 1.0 - min(min(products.x, products.y), min(products.z, products.w)));
                    let margin = max(max(abs(edge.x), abs(edge.y)), 1.0) * (32.0 * 1.1920929e-7);
                    if edge.x > 1.0e-5 + margin { opacity = 1u; }
                    else if edge.y <= 1.0e-5 - margin { opacity = 0u; }
                }
                result.opacity = combine_opacity(result.opacity, opacity);
            }
        }
    }
    if (flags & 0x10u) != 0u {
        if lens.astigmatism_gamma <= 0.0 {
            result.distance = vec4<f32>(-3.402823e38, -3.402823e38, 3.402823e38, 3.402823e38);
        } else {
            let delta = lens.center - real_position;
            var rotation = vec4<f32>(0.0, 1.0, 0.0, 1.0);
            if dot(delta, delta) > 0.0 {
                let inv_length = inverseSqrt(dot(delta, delta));
                if real_position.y < lens.center.y { rotation.x = -delta.x * inv_length; rotation.y = abs(delta.y) * inv_length; }
                else { rotation.x = delta.x * inv_length; rotation.y = -delta.y * inv_length; }
                if real_position.x < lens.center.x { rotation.z = delta.y * inv_length; rotation.w = delta.x * inv_length; }
                else { rotation.z = -delta.y * inv_length; rotation.w = -delta.x * inv_length; }
            }
            let first = scale_box(vec4<f32>(corrected.xx, corrected.zz), vec2<f32>(rotation.y, -rotation.z));
            let second = scale_box(vec4<f32>(corrected.yy, corrected.ww), vec2<f32>(rotation.x, rotation.w));
            let rotated = scale_box(add_box(first, second), vec2<f32>(0.8 * lens.astigmatism_amount));
            let closest = max(max(screen.xy, vec2<f32>(0.0)), -screen.zw);
            let farthest = max(abs(screen.xy), abs(screen.zw));
            let absolute_screen = vec4<f32>(closest.yx, farthest.yx);
            let powered = power_box(absolute_screen, vec2<f32>(1.0 / lens.astigmatism_gamma));
            let offset = multiply_box(rotated, powered);
            // The leaf may suppress the offset at a zero screen delta.
            result.distance = add_box(raw, vec4<f32>(min(offset.xy, vec2<f32>(0.0)), max(offset.zw, vec2<f32>(0.0))));
        }
    }
    return result;
}

fn aperture_node_class(
    bounds: vec4<u32>, destination: vec2<u32>, coc_min: f32, coc_max: f32,
    sign_value: f32, search_radius: f32,
) -> NodeClass {
    var result = NodeClass(0u, 0u, vec4<f32>(0.0));
    var source_x = Interval(f32(bounds.x) - f32(destination.x), f32(bounds.z - 1u) - f32(destination.x));
    var source_y = Interval(f32(bounds.y) - f32(destination.y), f32(bounds.w - 1u) - f32(destination.y));
    var opacity = 1u;
    if !PLAIN_CIRCLE && (lens.flags & GLOBAL_USE_NON_UNIFORM) != 0u {
        let bounded = lens_bounds(bounds, destination, coc_min, coc_max, sign_value);
        source_x = Interval(bounded.distance.x, bounded.distance.z);
        source_y = Interval(bounded.distance.y, bounded.distance.w);
        opacity = bounded.opacity;
        if opacity == 0u { return NodeClass(CHANNEL_MASK, 0u, vec4<f32>(0.0)); }
    }
    if PLAIN_CIRCLE || (lens.flags & (GLOBAL_SIMPLE_DISC | GLOBAL_AXIAL_ABERRATION)) == GLOBAL_SIMPLE_DISC {
        let radius = vec2<f32>(max(coc_min, 0.5), max(coc_max, 0.5));
        var shift = vec2<f32>(0.0);
        if sign_value > 0.0 {
            shift = settings.radius.zw;
            if (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u &&
               lens.filter_resolution.x > 0u && lens.filter_resolution.y > 0u {
                shift = 2.0 / vec2<f32>(lens.filter_resolution);
            }
        }
        // Bound source distance plus the radius-dependent aperture phase in
        // pixel space; squared distances avoid division and square roots.
        let phase_min = min(shift * radius.x, shift * radius.y);
        let phase_max = max(shift * radius.x, shift * radius.y);
        let lo = vec2<f32>(source_x.lo, source_y.lo) + phase_min;
        let hi = vec2<f32>(source_x.hi, source_y.hi) + phase_max;
        let padding = max(max(abs(lo), abs(hi)), vec2<f32>(1.0)) * (16.0 * 1.1920929e-7);
        let lower = lo - padding;
        let upper = hi + padding;
        let closest = max(max(lower, vec2<f32>(0.0)), -upper);
        let farthest = max(abs(lower), abs(upper));
        let outer = radius.y + 0.5;
        let inner = max(radius.x - 0.5, 0.0);
        if dot(closest, closest) >= outer * outer {
            result.empty_mask = CHANNEL_MASK;
        } else if dot(farthest, farthest) <= inner * inner {
            let weight_scale = node_weight_scale(coc_min, coc_max, 1.0);
            if weight_scale > 0.0 && opacity == 1u {
                result.accept_mask = CHANNEL_MASK;
                result.coefficient = vec4<f32>(search_radius * search_radius * weight_scale);
            }
        }
        return result;
    }
    if !PLAIN_CIRCLE && (lens.flags & (GLOBAL_USE_NON_UNIFORM | GLOBAL_AXIAL_ABERRATION)) == 0u {
        let radius = Interval(max(coc_min, 0.5), max(coc_max, 0.5));
        let diameter = Interval(max(radius.lo * 2.0, 1.0), max(radius.hi * 2.0, 1.0));
        let ux = ratio_interval(source_x, diameter);
        let uy = ratio_interval(source_y, diameter);
        var uv_min = vec2<f32>(0.5 - ux.hi, 0.5 - uy.hi);
        var uv_max = vec2<f32>(0.5 - ux.lo, 0.5 - uy.lo);
        if (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u && sign_value > 0.0 {
            let shift = 1.0 / vec2<f32>(lens.filter_resolution);
            uv_min -= shift;
            uv_max -= shift;
        }
        let uv_padding = max(max(abs(uv_min), abs(uv_max)), vec2<f32>(1.0)) * (16.0 * 1.1920929e-7);
        uv_min -= uv_padding;
        uv_max += uv_padding;
        let closest = max(
            max(uv_min - vec2<f32>(0.5), vec2<f32>(0.0)),
            vec2<f32>(0.5) - uv_max,
        );
        if dot(closest, closest) >= 0.25 + 32.0 * 1.1920929e-7 {
            result.empty_mask = CHANNEL_MASK;
            return result;
        }
        let min_resolution = f32(min(lens.filter_resolution.x, lens.filter_resolution.y));
        if min_resolution <= 0.0 { return result; }
        let lod_min = max(log2(min_resolution / diameter.hi), 0.0);
        let lod_max = max(log2(min_resolution / diameter.lo), 0.0);
        let range = filter_range(uv_min, uv_max, lod_min, lod_max);
        if !range.valid { return result; }
        let farthest = max(
            max(length(uv_min - vec2<f32>(0.5)), length(uv_max - vec2<f32>(0.5))),
            max(length(vec2<f32>(uv_min.x, uv_max.y) - vec2<f32>(0.5)), length(vec2<f32>(uv_max.x, uv_min.y) - vec2<f32>(0.5))),
        );
        for (var channel = 0u; channel < 4u; channel++) {
            let bit = 1u << channel;
            let minimum = component(range.min, channel);
            let maximum = component(range.max, channel);
            if minimum == 0.0 && maximum == 0.0 {
                result.empty_mask |= bit;
            } else if minimum == maximum && farthest < 0.5 {
                let weight_scale = node_weight_scale(coc_min, coc_max, 1.0);
                if weight_scale > 0.0 && opacity == 1u {
                    result.accept_mask |= bit;
                    result.coefficient = with_component(result.coefficient, channel, minimum * search_radius * search_radius * weight_scale);
                }
            }
        }
        return result;
    }
    for (var channel = 0u; channel < 4u; channel++) {
        let bit = 1u << channel;
        let scale = select(1.0, axial_scale(sign_value, channel), (lens.flags & GLOBAL_AXIAL_ABERRATION) != 0u);
        if scale != scale || abs(scale) > 3.402823e38 { continue; }
        let radius = Interval(max(coc_min * abs(scale), 0.5), max(coc_max * abs(scale), 0.5));
        if radius.lo <= 0.0 { continue; }

        if PLAIN_CIRCLE || (lens.flags & GLOBAL_SIMPLE_DISC) != 0u {
            var shift = vec2<f32>(0.0);
            if sign_value * scale > 0.0 {
                if (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u && lens.filter_resolution.x > 0u && lens.filter_resolution.y > 0u {
                    shift = 2.0 / vec2<f32>(lens.filter_resolution);
                } else if PLAIN_CIRCLE || (lens.flags & GLOBAL_AXIAL_ABERRATION) == 0u {
                    shift = settings.radius.zw;
                }
            }
            var ux = ratio_interval(source_x, radius);
            var uy = ratio_interval(source_y, radius);
            ux.lo += shift.x; ux.hi += shift.x;
            uy.lo += shift.y; uy.hi += shift.y;
            let padding = max(max(max(abs(ux.lo), abs(ux.hi)), max(abs(uy.lo), abs(uy.hi))), 1.0) * (16.0 * 1.1920929e-7);
            ux.lo -= padding; ux.hi += padding;
            uy.lo -= padding; uy.hi += padding;
            let min_x = max(max(ux.lo, 0.0), -ux.hi);
            let min_y = max(max(uy.lo, 0.0), -uy.hi);
            let max_x = max(abs(ux.lo), abs(ux.hi));
            let max_y = max(abs(uy.lo), abs(uy.hi));
            let minimum_distance = length(vec2<f32>(min_x, min_y));
            let maximum_distance = length(vec2<f32>(max_x, max_y));
            let empty_radius = 1.0 + 0.5 / radius.lo;
            let full_radius = 1.0 - 0.5 / radius.lo;
            if minimum_distance >= empty_radius {
                result.empty_mask |= bit;
            } else if maximum_distance <= full_radius {
                let weight_scale = node_weight_scale(coc_min, coc_max, scale);
                if weight_scale > 0.0 && opacity == 1u {
                    result.accept_mask |= bit;
                    result.coefficient = with_component(result.coefficient, channel, search_radius * search_radius * weight_scale);
                }
            }
            continue;
        }

        let diameter = Interval(max(radius.lo * 2.0, 1.0), max(radius.hi * 2.0, 1.0));
        var ux = ratio_interval(source_x, diameter);
        var uy = ratio_interval(source_y, diameter);
        var uv_min = vec2<f32>(0.5 - ux.hi, 0.5 - uy.hi);
        var uv_max = vec2<f32>(0.5 - ux.lo, 0.5 - uy.lo);
        if (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u && sign_value * scale > 0.0 {
            if lens.filter_resolution.x == 0u || lens.filter_resolution.y == 0u { continue; }
            let shift = 1.0 / vec2<f32>(lens.filter_resolution);
            uv_min -= shift;
            uv_max -= shift;
        }
        let uv_padding = max(max(abs(uv_min), abs(uv_max)), vec2<f32>(1.0)) * (16.0 * 1.1920929e-7);
        uv_min -= uv_padding;
        uv_max += uv_padding;
        let min_resolution = f32(min(lens.filter_resolution.x, lens.filter_resolution.y));
        if min_resolution <= 0.0 { continue; }
        let lod_min = max(log2(min_resolution / diameter.hi), 0.0);
        let lod_max = max(log2(min_resolution / diameter.lo), 0.0);
        let range = filter_range(uv_min, uv_max, lod_min, lod_max);
        if !range.valid { continue; }
        let minimum = component(range.min, channel);
        let maximum = component(range.max, channel);
        if minimum == 0.0 && maximum == 0.0 {
            result.empty_mask |= bit;
        } else if minimum == maximum {
            let farthest = max(
                max(length(uv_min - vec2<f32>(0.5)), length(uv_max - vec2<f32>(0.5))),
                max(length(vec2<f32>(uv_min.x, uv_max.y) - vec2<f32>(0.5)), length(vec2<f32>(uv_max.x, uv_min.y) - vec2<f32>(0.5))),
            );
            if farthest < 0.5 {
                let weight_scale = node_weight_scale(coc_min, coc_max, scale);
                if weight_scale > 0.0 && opacity == 1u {
                    result.accept_mask |= bit;
                    result.coefficient = with_component(result.coefficient, channel, minimum * search_radius * search_radius * weight_scale);
                }
            }
        }
    }
    return result;
}

fn load_node(index: u32, near_tree: bool) -> IntegralNode {
    if near_tree { return near_nodes[index]; }
    return far_nodes[index];
}

fn tree_length(near_tree: bool) -> u32 {
    if near_tree { return arrayLength(&near_nodes); }
    return arrayLength(&far_nodes);
}

fn load_integral_pixel(position: vec2<u32>, near_front: bool) -> IntegralPixel {
    var metadata: vec4<f32>;
    var color: vec4<f32>;
    if near_front {
        metadata = textureLoad(near_metadata, vec2<i32>(position), 0);
        color = textureLoad(near_color, vec2<i32>(position), 0);
    } else {
        metadata = textureLoad(far_metadata, vec2<i32>(position), 0);
        color = textureLoad(far_color, vec2<i32>(position), 0);
    }
    let valid = clamp(metadata.y, 0.0, 1.0);
    if valid <= 0.0 { return IntegralPixel(vec4<f32>(0.0), 0.0, 0.0, 0.0); }
    let coc = metadata.x / valid;
    let front = select(1.0, smoothstep(1.0, 2.0, max(coc, 0.0)), near_front);
    let occupied = valid * front;
    return IntegralPixel(
        color / valid,
        select(abs(coc), max(coc, 0.0), near_front),
        occupied,
        occupied,
    );
}

fn circle_tree(destination: vec2<u32>, sign_value: f32, near_tree: bool, potential_radius: f32) -> LensIntegral {
    var result = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    let potential = potential_radius > 0.5;
    if settings.frame.x == 1u && settings.frame.y == 1u {
        let pixel = load_integral_pixel(vec2<u32>(0u), near_tree);
        if potential {
            let weight = aperture_weight(destination, vec2<f32>(0.0), sign_value * potential_radius, 1.0).x;
            result.premul = vec4<f32>(pixel.valid * weight);
            result.valid = vec4<f32>(weight);
        } else if pixel.valid > 0.0 && pixel.coc > 0.5 {
            let weight = aperture_weight(destination, vec2<f32>(0.0), sign_value * pixel.coc, 1.0).x;
            result.premul = pixel.premul * (pixel.valid * weight);
            result.valid = vec4<f32>(pixel.valid * weight);
            if weight > 0.0 { result.presence = vec4<f32>(pixel.coc); }
        }
        return result;
    }
    let count = tree_length(near_tree);
    var reference = 0u;
    while reference < count {
        let node = load_node(reference, near_tree);
        if !potential && node.weighted_valid <= 0.0 {
            reference = subtree_ends[reference];
            continue;
        }
        let minimum = select(node.coc_min, potential_radius, potential);
        let maximum = select(node.coc_max, potential_radius, potential);
        let classification = aperture_node_class(node.bounds, destination, minimum, maximum, sign_value, 1.0);
        if classification.empty_mask == CHANNEL_MASK {
            reference = subtree_ends[reference];
        } else if classification.accept_mask == CHANNEL_MASK {
            let coefficient = classification.coefficient.x;
            if potential {
                let weight = coefficient / max(potential_radius * potential_radius, 0.25);
                let area = f32((node.bounds.z - node.bounds.x) * (node.bounds.w - node.bounds.y));
                result.premul += vec4<f32>(node.occupied_area * weight);
                result.valid += vec4<f32>(area * weight);
            } else {
                result.premul += node.weighted_premul * coefficient;
                result.valid += vec4<f32>(node.weighted_valid * coefficient);
                result.presence = max(result.presence, vec4<f32>(node.coc_max));
            }
            reference = subtree_ends[reference];
        } else {
            // Nodes are preorder-contiguous. Evaluate tagged pixel children
            // here; advancing visits the internal children exactly once.
            for (var child = 0u; child < 4u; child++) {
                let leaf = node.children[child];
                if leaf != EMPTY_REF && (leaf & LEAF_REF_BIT) != 0u {
                    let index = leaf & ~LEAF_REF_BIT;
                    let position = vec2<u32>(index % settings.frame.x, index / settings.frame.x);
                    let distance = vec2<f32>(position) - vec2<f32>(destination);
                    if potential {
                        let weight = aperture_weight(destination, distance, sign_value * potential_radius, 1.0).x;
                        let metadata = textureLoad(near_metadata, vec2<i32>(position), 0);
                        let valid = clamp(metadata.y, 0.0, 1.0);
                        let coc = metadata.x / max(valid, 1.0e-6);
                        let occupied = valid * smoothstep(1.0, 2.0, max(coc, 0.0));
                        result.premul += vec4<f32>(occupied * weight);
                        result.valid += vec4<f32>(weight);
                    } else {
                        let pixel = load_integral_pixel(position, near_tree);
                        if pixel.valid > 0.0 && pixel.coc > 0.5 {
                        let weight = aperture_weight(destination, distance, sign_value * pixel.coc, 1.0).x;
                            result.premul += pixel.premul * (pixel.valid * weight);
                            result.valid += vec4<f32>(pixel.valid * weight);
                            if weight > 0.0 { result.presence = max(result.presence, vec4<f32>(pixel.coc)); }
                        }
                    }
                }
            }
            reference += 1u;
        }
    }
    return result;
}

struct FusedCircleIntegral {
    front: LensIntegral,
    potential: vec2<f32>,
};

// The ordinary near-front and potential gathers walk the same preorder tree.
// Walk it once while keeping independent subtree skip points for each term.
// The potential term uses the cap radius; the caller consumes it only when
// the exact front presence is that cap, matching the unfused path.
fn integrate_fused_capped_circle(destination: vec2<u32>) -> FusedCircleIntegral {
    var front = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    var potential = vec2<f32>(0.0);
    let cap = settings.radius.x;
    if settings.frame.x == 1u && settings.frame.y == 1u {
        let pixel = load_integral_pixel(vec2<u32>(0u), true);
        let weight = aperture_weight(destination, vec2<f32>(0.0), cap, 1.0).x;
        potential = vec2<f32>(pixel.valid * weight, weight);
        if pixel.valid > 0.0 && pixel.coc > 0.5 {
            let front_weight = aperture_weight(destination, vec2<f32>(0.0), pixel.coc, 1.0).x;
            front.premul = pixel.premul * (pixel.valid * front_weight);
            front.valid = vec4<f32>(pixel.valid * front_weight);
            if front_weight > 0.0 { front.presence = vec4<f32>(pixel.coc); }
        }
        return FusedCircleIntegral(front, potential);
    }

    let count = tree_length(true);
    var reference = 0u;
    var front_skip_end = 0u;
    var potential_skip_end = 0u;
    while reference < count {
        let node = load_node(reference, true);
        let subtree_end = subtree_ends[reference];
        var front_active = reference >= front_skip_end;
        var potential_active = reference >= potential_skip_end;
        var front_class = NodeClass(0u, 0u, vec4<f32>(0.0));
        var front_class_valid = false;

        if front_active && node.weighted_valid <= 0.0 {
            front_skip_end = subtree_end;
            front_active = false;
        }
        if front_active {
            front_class = aperture_node_class(
                node.bounds, destination, node.coc_min, node.coc_max, 1.0, 1.0,
            );
            front_class_valid = true;
            if front_class.empty_mask == CHANNEL_MASK {
                front_skip_end = subtree_end;
                front_active = false;
            } else if front_class.accept_mask == CHANNEL_MASK {
                let coefficient = front_class.coefficient.x;
                front.premul += node.weighted_premul * coefficient;
                front.valid += vec4<f32>(node.weighted_valid * coefficient);
                front.presence = max(front.presence, vec4<f32>(node.coc_max));
                front_skip_end = subtree_end;
                front_active = false;
            }
        }

        if potential_active {
            var potential_class = NodeClass(0u, 0u, vec4<f32>(0.0));
            if front_class_valid && node.coc_min == cap && node.coc_max == cap {
                potential_class = front_class;
            } else {
                potential_class = aperture_node_class(
                    node.bounds, destination, cap, cap, 1.0, 1.0,
                );
            }
            if potential_class.empty_mask == CHANNEL_MASK {
                potential_skip_end = subtree_end;
                potential_active = false;
            } else if potential_class.accept_mask == CHANNEL_MASK {
                let weight = potential_class.coefficient.x / max(cap * cap, 0.25);
                let area = f32((node.bounds.z - node.bounds.x) * (node.bounds.w - node.bounds.y));
                potential.x += node.occupied_area * weight;
                potential.y += area * weight;
                potential_skip_end = subtree_end;
                potential_active = false;
            }
        }

        if !front_active && !potential_active {
            reference = min(front_skip_end, potential_skip_end);
            continue;
        }

        for (var child = 0u; child < 4u; child++) {
            let leaf = node.children[child];
            if leaf != EMPTY_REF && (leaf & LEAF_REF_BIT) != 0u {
                let index = leaf & ~LEAF_REF_BIT;
                let position = vec2<u32>(index % settings.frame.x, index / settings.frame.x);
                let distance = vec2<f32>(position) - vec2<f32>(destination);
                let pixel = load_integral_pixel(position, true);
                var front_weight = 0.0;
                var have_front_weight = false;
                if front_active && pixel.valid > 0.0 && pixel.coc > 0.5 {
                    front_weight = aperture_weight(destination, distance, pixel.coc, 1.0).x;
                    have_front_weight = true;
                    front.premul += pixel.premul * (pixel.valid * front_weight);
                    front.valid += vec4<f32>(pixel.valid * front_weight);
                    if front_weight > 0.0 {
                        front.presence = max(front.presence, vec4<f32>(pixel.coc));
                    }
                }
                if potential_active {
                    var weight = 0.0;
                    if have_front_weight && pixel.coc == cap {
                        weight = front_weight;
                    } else {
                        weight = aperture_weight(destination, distance, cap, 1.0).x;
                    }
                    potential.x += pixel.occupied * weight;
                    potential.y += weight;
                }
            }
        }
        reference += 1u;
    }
    return FusedCircleIntegral(front, potential);
}

fn integrate_disk(destination: vec2<u32>, sign_value: f32, search_radius: f32, near_tree: bool) -> LensIntegral {
    var result = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    let frame = settings.frame.xy;
    if frame.x == 0u || frame.y == 0u { return result; }
    if PLAIN_CIRCLE || ((lens.flags & (GLOBAL_SIMPLE_DISC | GLOBAL_USE_NON_UNIFORM | GLOBAL_AXIAL_ABERRATION)) == GLOBAL_SIMPLE_DISC && (frame.x > 1u || frame.y > 1u)) {
        return circle_tree(destination, sign_value, near_tree, 0.0);
    }
    var refs: array<u32, 64>;
    var masks: array<u32, 64>;
    refs[0] = select(0u, LEAF_REF_BIT, frame.x == 1u && frame.y == 1u);
    masks[0] = CHANNEL_MASK;
    var stack_len = 1u;
    let node_count = tree_length(near_tree);
    while stack_len > 0u {
        stack_len -= 1u;
        let reference = refs[stack_len];
        let active_mask = masks[stack_len];
        if reference != EMPTY_REF && active_mask != 0u {
            if (reference & LEAF_REF_BIT) != 0u {
                let index = reference & ~LEAF_REF_BIT;
                let position = vec2<u32>(index % frame.x, index / frame.x);
                if position.y < frame.y {
                    let pixel = load_integral_pixel(position, near_tree);
                    if pixel.valid > 0.0 && pixel.coc > 0.5 {
                        let weights = aperture_weight(destination, vec2<f32>(position) - vec2<f32>(destination), sign_value * pixel.coc, search_radius);
                        let active_weights = weights * channel_mask_vec(active_mask);
                        result.premul += pixel.premul * pixel.valid * active_weights;
                        result.valid += vec4<f32>(pixel.valid) * active_weights;
                        result.presence = max(result.presence, select(vec4<f32>(0.0), vec4<f32>(pixel.coc), active_weights > vec4<f32>(0.0)));
                    }
                }
            } else if reference < node_count {
                let node = load_node(reference, near_tree);
                if node.weighted_valid > 0.0 {
                    let classification = aperture_node_class(node.bounds, destination, node.coc_min, node.coc_max, sign_value, search_radius);
                    let active_after_empty = active_mask & ~classification.empty_mask;
                    var descend = active_after_empty & ~classification.accept_mask;
                    let accepted = active_after_empty & classification.accept_mask;
                    let accepted_weights = classification.coefficient * channel_mask_vec(accepted);
                    result.premul += node.weighted_premul * accepted_weights;
                    result.valid += vec4<f32>(node.weighted_valid) * accepted_weights;
                    result.presence = max(result.presence, select(vec4<f32>(0.0), vec4<f32>(node.coc_max), accepted_weights > vec4<f32>(0.0)));
                    for (var child_index = 4u; child_index > 0u; child_index--) {
                        let child = node.children[child_index - 1u];
                        if child != EMPTY_REF && descend != 0u && stack_len < 64u {
                            refs[stack_len] = child;
                            masks[stack_len] = descend;
                            stack_len += 1u;
                        }
                    }
                }
            }
        }
    }
    return result;
}

fn integrate_potential(destination: vec2<u32>, radii: vec4<f32>, sign_value: f32) -> LensIntegral {
    var result = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    let frame = settings.frame.xy;
    if frame.x == 0u || frame.y == 0u { return result; }
    let same_radius = all(radii == vec4<f32>(radii.x));
    if PLAIN_CIRCLE || (same_radius && (lens.flags & (GLOBAL_SIMPLE_DISC | GLOBAL_USE_NON_UNIFORM | GLOBAL_AXIAL_ABERRATION)) == GLOBAL_SIMPLE_DISC && (frame.x > 1u || frame.y > 1u)) {
        return circle_tree(destination, sign_value, true, radii.x);
    }
    var refs: array<u32, 64>;
    var masks: array<u32, 64>;
    refs[0] = select(0u, LEAF_REF_BIT, frame.x == 1u && frame.y == 1u);
    masks[0] = 0u;
    for (var channel = 0u; channel < 4u; channel++) {
        if component(radii, channel) > 0.5 { masks[0] |= 1u << channel; }
    }
    var stack_len = 1u;
    let node_count = arrayLength(&near_nodes);
    while stack_len > 0u {
        stack_len -= 1u;
        let reference = refs[stack_len];
        let active_mask = masks[stack_len];
        if reference != EMPTY_REF && active_mask != 0u {
            if (reference & LEAF_REF_BIT) != 0u {
                let index = reference & ~LEAF_REF_BIT;
                let position = vec2<u32>(index % frame.x, index / frame.x);
                if position.y < frame.y {
                    let pixel = load_integral_pixel(position, true);
                    let occupancy = pixel.valid;
                    if same_radius {
                        let weights = aperture_weight(destination, vec2<f32>(position) - vec2<f32>(destination), sign_value * radii.x, 1.0) * channel_mask_vec(active_mask);
                        result.premul += weights * occupancy;
                        result.valid += weights;
                    } else {
                        for (var channel = 0u; channel < 4u; channel++) {
                            let bit = 1u << channel;
                            if (active_mask & bit) != 0u {
                                let weight = component(aperture_weight(destination, vec2<f32>(position) - vec2<f32>(destination), sign_value * component(radii, channel), 1.0), channel);
                                result.premul = with_added_component(result.premul, channel, occupancy * weight);
                                result.valid = with_added_component(result.valid, channel, weight);
                            }
                        }
                    }
                }
            } else if reference < node_count {
                let node = near_nodes[reference];
                var descend = active_mask;
                if same_radius {
                    let classification = aperture_node_class(node.bounds, destination, radii.x, radii.x, sign_value, 1.0);
                    descend &= ~classification.empty_mask;
                    let accepted = descend & classification.accept_mask;
                    let inverse_area = 1.0 / max(radii.x * radii.x, 0.25);
                    let coefficients = classification.coefficient * inverse_area * channel_mask_vec(accepted);
                    let area = f32((node.bounds.z - node.bounds.x) * (node.bounds.w - node.bounds.y));
                    result.premul += vec4<f32>(node.occupied_area) * coefficients;
                    result.valid += vec4<f32>(area) * coefficients;
                    result.presence = max(result.presence, select(vec4<f32>(0.0), radii, coefficients > vec4<f32>(0.0)));
                    descend &= ~classification.accept_mask;
                } else {
                    for (var channel = 0u; channel < 4u; channel++) {
                        let bit = 1u << channel;
                        if (active_mask & bit) != 0u {
                            let classification = aperture_node_class(node.bounds, destination, component(radii, channel), component(radii, channel), sign_value, 1.0);
                            if (classification.empty_mask & bit) != 0u { descend &= ~bit; }
                            else if (classification.accept_mask & bit) != 0u {
                                let radius = component(radii, channel);
                                let coefficient = component(classification.coefficient, channel) / max(radius * radius, 0.25);
                                let area = f32((node.bounds.z - node.bounds.x) * (node.bounds.w - node.bounds.y));
                                result.premul = with_added_component(result.premul, channel, node.occupied_area * coefficient);
                                result.valid = with_added_component(result.valid, channel, area * coefficient);
                                result.presence = with_component(result.presence, channel, component(radii, channel));
                                descend &= ~bit;
                            }
                        }
                    }
                }
                for (var child_index = 4u; child_index > 0u; child_index--) {
                    let child = node.children[child_index - 1u];
                    if child != EMPTY_REF && descend != 0u && stack_len < 64u {
                        refs[stack_len] = child;
                        masks[stack_len] = descend;
                        stack_len += 1u;
                    }
                }
            }
        }
    }
    return result;
}

fn integrate_local(destination: vec2<u32>) -> LocalIntegral {
    var result = LocalIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    let frame = settings.frame.xy;
    if frame.x == 0u || frame.y == 0u { return result; }
    var refs: array<u32, 64>;
    refs[0] = select(0u, LEAF_REF_BIT, frame.x == 1u && frame.y == 1u);
    var stack_len = 1u;
    let node_count = arrayLength(&near_nodes);
    let full_traversal = (lens.flags & (GLOBAL_USE_NON_UNIFORM | GLOBAL_AXIAL_ABERRATION)) != 0u;
    var local_support_radius = 2.5;
    if PLAIN_CIRCLE || (lens.flags & GLOBAL_SIMPLE_DISC) != 0u {
        if (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u &&
           lens.filter_resolution.x > 0u && lens.filter_resolution.y > 0u {
            local_support_radius += length(4.0 / vec2<f32>(lens.filter_resolution));
        } else {
            local_support_radius += length(settings.radius.zw * 2.0);
        }
    } else if (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u &&
              lens.filter_resolution.x > 0u && lens.filter_resolution.y > 0u {
        local_support_radius += length(4.0 / vec2<f32>(lens.filter_resolution));
    }
    while stack_len > 0u {
        stack_len -= 1u;
        let reference = refs[stack_len];
        if reference != EMPTY_REF {
            if (reference & LEAF_REF_BIT) != 0u {
                let index = reference & ~LEAF_REF_BIT;
                let position = vec2<u32>(index % frame.x, index / frame.x);
                if position.y < frame.y {
                    let uv = texture_uv(vec2<f32>(position) + vec2<f32>(0.5));
                    let metadata = textureLoad(near_metadata, vec2<i32>(position), 0);
                    let valid = clamp(metadata.y, 0.0, 1.0);
                    let distance = vec2<f32>(position) - vec2<f32>(destination);
                    let support = aperture_weight(destination, distance, 2.0, 1.0);
                    result.support_weight += support;
                    result.occupied_support += support * valid;
                    if valid > 0.0 {
                        let coc = metadata.x / valid;
                        if coc > 0.5 {
                            let t = clamp((coc - 1.0), 0.0, 1.0);
                            let local_fraction = 1.0 - t * t * (3.0 - 2.0 * t);
                            if local_fraction > 0.0 {
                                let color = textureLoad(near_color, vec2<i32>(position), 0) / valid;
                                let weight = aperture_weight(destination, distance, coc, 1.0) * (valid * local_fraction);
                                result.premul += color * weight;
                                result.local_weight += weight;
                            }
                        }
                    }
                }
            } else if reference < node_count {
                let node = near_nodes[reference];
                var include = full_traversal;
                if full_traversal {
                    let support = aperture_node_class(node.bounds, destination, 0.5, 2.0, 1.0, 1.0);
                    include = support.empty_mask != CHANNEL_MASK;
                } else {
                    let min_x = max(max(f32(node.bounds.x) - f32(destination.x), 0.0), f32(destination.x) - f32(node.bounds.z - 1u));
                    let min_y = max(max(f32(node.bounds.y) - f32(destination.y), 0.0), f32(destination.y) - f32(node.bounds.w - 1u));
                    include = dot(vec2<f32>(min_x, min_y), vec2<f32>(min_x, min_y)) <= local_support_radius * local_support_radius;
                }
                if include {
                    for (var child_index = 4u; child_index > 0u; child_index--) {
                        let child = node.children[child_index - 1u];
                        if child != EMPTY_REF && stack_len < 64u {
                            refs[stack_len] = child;
                            stack_len += 1u;
                        }
                    }
                }
            }
        }
    }
    return result;
}

fn tile_radius(x: u32, y: u32, offset: u32) -> f32 {
    let tile_position = (vec2<f32>(f32(x), f32(y)) + vec2<f32>(settings.tile_origin)) / 16.0 - vec2<f32>(0.46875);
    let base = floor(tile_position);
    let fraction = tile_position - base;
    let tile_max = vec2<f32>(settings.frame.zw) - vec2<f32>(1.0);
    let p00 = vec2<u32>(clamp(base, vec2<f32>(0.0), tile_max));
    let p10 = vec2<u32>(clamp(base + vec2<f32>(1.0, 0.0), vec2<f32>(0.0), tile_max));
    let p01 = vec2<u32>(clamp(base + vec2<f32>(0.0, 1.0), vec2<f32>(0.0), tile_max));
    let p11 = vec2<u32>(clamp(base + vec2<f32>(1.0), vec2<f32>(0.0), tile_max));
    let tiles_x = settings.frame.z;
    let r00 = near_search_radii[p00.y * tiles_x + p00.x + offset];
    let r10 = near_search_radii[p10.y * tiles_x + p10.x + offset];
    let r01 = near_search_radii[p01.y * tiles_x + p01.x + offset];
    let r11 = near_search_radii[p11.y * tiles_x + p11.x + offset];
    let top = mix(r00, r10, fraction.x);
    let bottom = mix(r01, r11, fraction.x);
    return mix(top, bottom, fraction.y);
}

// Gathers skip source pixels with |CoC| <= GATHER_MIN_COC, so such a centre is
// present only through its source value; a far centre hands its source to the
// far gather only above that radius, by FAR_CENTER_BLURRED_COC.
const GATHER_MIN_COC: f32 = 0.5;
const FAR_CENTER_BLURRED_COC: f32 = 1.0;
// A near centre is part of the opaque surface until it becomes a front pixel
// (the smoothstep(1, 2) front occupancy). Below that its own source, not the
// far continuation it occludes, lies under the local layer; the sampled local
// estimate is not exact enough to hide that far colour, which otherwise shows
// as a line where a receding surface crosses the focal plane.
const FRONT_MIN_COC: f32 = 1.0;
const FRONT_FULL_COC: f32 = 2.0;

// The front layer is normalised by the potential disc area at its silhouette,
// which gives its coverage, and by its own gathered validity inside the
// surface, where that coverage estimate is biased by front pixels whose CoC is
// below the potential radius. The hand-over ramps over the potential occupancy
// from FRONT_INTERIOR_FROM to one, which is the outer quarter of the potential
// radius, so it is as wide as the blur itself. A narrow ramp switches between
// two denominators that differ by several percent and draws a ring inside the
// blurred foreground.
const FRONT_INTERIOR_FROM: f32 = 0.9;

// Weight of the front layer's own validity in its normaliser at potential
// `occupancy`; see FRONT_INTERIOR_FROM.
fn front_interior_blend(occupancy: vec4<f32>) -> vec4<f32> {
    return smoothstep(vec4<f32>(FRONT_INTERIOR_FROM), vec4<f32>(1.0), occupancy);
}

fn center_blur_weight(center_near: LayerSample, center_far: LayerSample) -> f32 {
    let near_blur = smoothstep(FRONT_MIN_COC, FRONT_FULL_COC, abs(center_near.coc));
    let far_blur = smoothstep(GATHER_MIN_COC, FAR_CENTER_BLURRED_COC, abs(center_far.coc));
    return far_blur + (near_blur - far_blur) * clamp(center_near.valid, 0.0, 1.0);
}

// The far layer is one normalized gather, so a strongly blurred background
// would otherwise bleed through a less blurred far surface in front of it. A
// ray that reaches the destination from a far source of CoC r crosses the
// depth of CoC t at offset (t / r) * (source - destination). The model probes
// the far layer on a disc of radius FAR_PROBE_SCALE times the destination's own
// far CoC: pixels up to FAR_BLOCKED_SCALE times that radius block, and their
// blocking fades out at the radius (`far_blocking_weight`). Sources above the
// radius are partly, and above FAR_BEHIND_SCALE times it wholly, behind the
// surface (`far_behind_weight`); the gap between the two ramps keeps the
// farther part of a receding surface off the blocked side. Sources behind the
// surface remain visible only where their ray crosses the probe
// disc outside the blocking surface, which the probe tests at the depth of the
// probe CoC: the farther part of a receding plane or of a curved rim lies on
// the open side of that disc and keeps its reach. Every weight is continuous
// in the source CoC and the destination's own CoC, so the occlusion never
// switches on at a contour inside a far surface. A one-sided open area (a
// straight silhouette) is fitted by a half-plane, which hides exactly the
// sources whose ray crosses the blocked side. Open area spread around the destination (holes, interleaved detail)
// gives no direction; it lets the open fraction of every behind source
// through. The two are blended by how one-sided the open area is. A separate
// pass integrates the hidden behind sources and the exact gather subtracts
// them from its ordinary far gather.
const FAR_PROBE_SCALE: f32 = 2.0;
const FAR_BEHIND_SCALE: f32 = 1.25;
const FAR_BLOCKED_SCALE: f32 = 0.8;
// The fitted occluder edge is softened by up to one pixel on its covered side,
// scaled down as the open fraction falls to FAR_SOFT_OPEN so that visibility
// vanishes continuously with the open area.
const FAR_SOFT_OPEN: f32 = 0.05;
// Visible fraction of the gathered far validity below which the remainder of
// the far gather is cancellation residue and the far surface's own colour is
// used; above FAR_HIDDEN_TRUSTED the remainder is used as gathered.
const FAR_HIDDEN_RESIDUE: f32 = 0.01;
const FAR_HIDDEN_TRUSTED: f32 = 0.05;
// Bisection steps that invert the circular-segment area to 2^-20 of the radius.
const CAP_HEIGHT_STEPS: u32 = 20u;
const PI: f32 = 3.14159265358979;
// Relative tolerance when a node's validity sums prove it holds no in-focus
// pixel; such pixels count as occupancy but are absent from the CoC range.
const FAR_COVER_TOLERANCE: f32 = 1.0e-4;
const FAR_UNOCCLUDED: u32 = 0u;
const FAR_HIDDEN: u32 = 1u;
const FAR_PARTIAL: u32 = 2u;

fn far_unoccluded() -> FarOcclusion {
    return FarOcclusion(FAR_UNOCCLUDED, 0.0, 0.0, 0.0, vec2<f32>(0.0), 0.0, 0.0, 0.0);
}

fn far_probe_radius(center_far: LayerSample) -> f32 {
    return FAR_PROBE_SCALE * max(abs(center_far.coc), GATHER_MIN_COC);
}

// Whether a far source behind the probed surface can reach a pixel whose far
// sources reach at most `reach` pixels: only sources above the probe CoC have
// a behind weight.
fn far_behind_reachable(reach: f32, probe: f32) -> bool {
    return reach > probe;
}

// Part of a far pixel of CoC `coc` that lies behind the surface probed with
// `probe`: zero up to the probe CoC, one from FAR_BEHIND_SCALE times it.
fn far_behind_weight(coc: f32, probe: f32) -> f32 {
    return smoothstep(probe, probe * FAR_BEHIND_SCALE, coc);
}

// Part of a valid far pixel of CoC `coc` that blocks the probe disc of radius
// `probe`: one up to FAR_BLOCKED_SCALE times the radius, zero from the radius.
fn far_blocking_weight(coc: f32, probe: f32) -> f32 {
    return 1.0 - smoothstep(probe * FAR_BLOCKED_SCALE, probe, coc);
}

// Height, as a fraction of the radius, of the chord that cuts `fraction` of a
// disc's area off on its far side.
fn cap_height(fraction: f32) -> f32 {
    var low = -1.0;
    var high = 1.0;
    for (var step = 0u; step < CAP_HEIGHT_STEPS; step++) {
        let middle = 0.5 * (low + high);
        let area = (acos(middle) - middle * sqrt(1.0 - middle * middle)) / PI;
        if area > fraction { low = middle; } else { high = middle; }
    }
    return 0.5 * (low + high);
}

// Occluder fitted to the `cover` of the probe disc of radius `probe`: its
// blocked area, total area and the first moment of its open area about the
// destination. A disc without blocked area hides nothing, the limit of a
// partial occluder whose open fraction reaches one.
fn far_occlusion_from_cover(probe: f32, cover: vec4<f32>) -> FarOcclusion {
    if cover.y <= 0.0 || cover.x <= 0.0 { return far_unoccluded(); }
    let behind = probe * FAR_BEHIND_SCALE;
    let open = 1.0 - clamp(cover.x / cover.y, 0.0, 1.0);
    if open <= 0.0 {
        return FarOcclusion(FAR_HIDDEN, probe, behind, 0.0, vec2<f32>(0.0), 0.0, 0.0, 0.0);
    }
    let edge = cap_height(open);
    let moment = cover.zw / (cover.y * probe);
    let cap_moment = 2.0 / (3.0 * PI) * pow(1.0 - edge * edge, 1.5);
    let moment_length = length(moment);
    var one_sided = 0.0;
    var normal = vec2<f32>(0.0);
    if moment_length > 0.0 && cap_moment > 0.0 {
        one_sided = clamp(moment_length / cap_moment, 0.0, 1.0);
        normal = moment / moment_length;
    }
    let softness = min(open / FAR_SOFT_OPEN, 1.0);
    return FarOcclusion(FAR_PARTIAL, probe, behind, softness, normal, edge, one_sided, open);
}

// Hidden fraction of a wholly behind source whose half-plane part is
// `edge_hidden` (one when the half-plane blocks its ray).
fn far_hidden_fraction(edge_hidden: f32, occlusion: FarOcclusion) -> f32 {
    let spread_hidden = 1.0 - occlusion.open;
    return mix(spread_hidden, edge_hidden, occlusion.one_sided);
}

// Half-plane part of the hidden fraction for a ray crossing the probe disc
// `along_normal` probe radii along the fitted normal.
fn far_edge_hidden(along_normal: f32, occlusion: FarOcclusion) -> f32 {
    return 1.0 - clamp((along_normal - occlusion.edge) * occlusion.probe + occlusion.softness, 0.0, 1.0);
}

// Hidden fraction of one far source of CoC `coc` at `offset` from the destination.
fn far_pixel_hidden(coc: f32, offset: vec2<f32>, occlusion: FarOcclusion) -> f32 {
    if occlusion.state == FAR_UNOCCLUDED || coc <= occlusion.probe { return 0.0; }
    let behind = far_behind_weight(coc, occlusion.probe);
    if occlusion.state == FAR_HIDDEN { return behind; }
    return behind * far_hidden_fraction(far_edge_hidden(dot(occlusion.normal, offset / coc), occlusion), occlusion);
}

fn scale_interval(value: Interval, factor: f32) -> Interval {
    return Interval(min(value.lo * factor, value.hi * factor), max(value.lo * factor, value.hi * factor));
}

// Hidden fraction shared by every source of a far node whose bounds are in
// full-frame coordinates, or -1 when the node must be descended. Only nodes
// wholly behind share a behind weight (one).
fn far_node_hidden(node: IntegralNode, bounds: vec4<u32>, destination: vec2<u32>, occlusion: FarOcclusion) -> f32 {
    if occlusion.state == FAR_UNOCCLUDED || node.coc_max <= occlusion.probe { return 0.0; }
    if node.coc_min < occlusion.behind { return -1.0; }
    if occlusion.state == FAR_HIDDEN { return 1.0; }
    let radius = Interval(node.coc_min, node.coc_max);
    let crossing_x = ratio_interval(Interval(f32(bounds.x) - f32(destination.x), f32(bounds.z - 1u) - f32(destination.x)), radius);
    let crossing_y = ratio_interval(Interval(f32(bounds.y) - f32(destination.y), f32(bounds.w - 1u) - f32(destination.y)), radius);
    let along_x = scale_interval(crossing_x, occlusion.normal.x);
    let along_y = scale_interval(crossing_y, occlusion.normal.y);
    let most_hidden = far_edge_hidden(along_x.lo + along_y.lo, occlusion);
    let least_hidden = far_edge_hidden(along_x.hi + along_y.hi, occlusion);
    if most_hidden == least_hidden || occlusion.one_sided == 0.0 {
        return far_hidden_fraction(most_hidden, occlusion);
    }
    return -1.0;
}

// Blocked area of a fully contained far node in the probe disc of radius
// `probe`, or -1 when its pixels must be visited. In-focus pixels block but
// are missing from the CoC range; an open node whose validity exceeds
// what its blurred pixels can hold contains them and is visited.
fn far_node_blocked(node: IntegralNode, probe: f32) -> f32 {
    if node.coc_max <= probe * FAR_BLOCKED_SCALE { return node.occupied_area; }
    let blurred_valid_bound = node.weighted_valid * node.coc_max * node.coc_max;
    if node.coc_min >= probe && node.occupied_area <= blurred_valid_bound * (1.0 + FAR_COVER_TOLERANCE) {
        return 0.0;
    }
    return -1.0;
}

// Distances from `destination` to the nearest and farthest pixel centre of a
// node whose bounds are in full-frame coordinates.
fn node_distance_range(bounds: vec4<u32>, destination: vec2<u32>) -> vec2<f32> {
    let lower = vec2<f32>(bounds.xy) - vec2<f32>(destination);
    let upper = vec2<f32>(bounds.zw - vec2<u32>(1u)) - vec2<f32>(destination);
    let closest = max(max(lower, vec2<f32>(0.0)), -upper);
    let farthest = max(abs(lower), abs(upper));
    return vec2<f32>(length(closest), length(farthest));
}

// Leaf pixels of the far tree are visited at their parent; a one-pixel frame
// has no node and its single pixel is the leaf at index 0.
fn far_leaf_position(leaf: u32, extent: vec2<u32>) -> vec2<u32> {
    let index = leaf & ~LEAF_REF_BIT;
    return vec2<u32>(index % extent.x, index / extent.x);
}

fn far_tree_is_single_pixel(extent: vec2<u32>) -> bool {
    return all(extent == vec2<u32>(1u));
}

// Sums of a walk of the far tree: the probe `cover` (blocked area, total area
// and the first moment of the open area about the destination) or the hidden
// far sources (`premul` and per-channel `valid`), depending on its mode.
struct FarWalk {
    cover: vec4<f32>,
    premul: vec4<f32>,
    valid: vec4<f32>,
};

// What a walk of the far tree does at one node: add `sums` and skip its
// subtree, or visit its leaf children and descend.
struct FarStep {
    descend: bool,
    sums: FarWalk,
};

// Modes of `far_walk`: the probe cover, or the hidden far sources.
const FAR_WALK_COVER: u32 = 0u;
const FAR_WALK_HIDDEN: u32 = 1u;

fn far_walk_empty() -> FarWalk {
    return FarWalk(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
}

fn far_walk_add(a: FarWalk, b: FarWalk) -> FarWalk {
    return FarWalk(a.cover + b.cover, a.premul + b.premul, a.valid + b.valid);
}

// Walks the far tree around `destination` in `mode`, with the probe radius of
// `occlusion` for the cover and its occluder for the hidden sources. Walks of
// the far tree are stackless preorder walks: a private stack would cost every
// pixel of the gather occupancy. `origin` and `extent` place the bound far
// tree and textures in full-frame coordinates.
fn far_walk(mode: u32, destination: vec2<u32>, occlusion: FarOcclusion, origin: vec2<u32>, extent: vec2<u32>) -> FarWalk {
    var sums = far_walk_empty();
    if any(extent == vec2<u32>(0u)) { return sums; }
    if far_tree_is_single_pixel(extent) { return far_walk_leaf(mode, vec2<u32>(0u), destination, occlusion, origin); }
    let count = tree_length(false);
    var reference = 0u;
    while reference < count {
        let node = far_nodes[reference];
        let bounds = node.bounds + vec4<u32>(origin, origin);
        var step: FarStep;
        if mode == FAR_WALK_COVER {
            step = far_cover_step(node, bounds, destination, occlusion.probe);
        } else {
            step = far_hidden_step(node, bounds, destination, occlusion);
        }
        if !step.descend {
            sums = far_walk_add(sums, step.sums);
            reference = subtree_ends[reference];
            continue;
        }
        for (var child = 0u; child < 4u; child++) {
            let leaf = node.children[child];
            if leaf != EMPTY_REF && (leaf & LEAF_REF_BIT) != 0u {
                sums = far_walk_add(sums, far_walk_leaf(mode, far_leaf_position(leaf, extent), destination, occlusion, origin));
            }
        }
        reference += 1u;
    }
    return sums;
}

// Sums of the far leaf at `local` in `mode`; see `far_walk`.
fn far_walk_leaf(mode: u32, local: vec2<u32>, destination: vec2<u32>, occlusion: FarOcclusion, origin: vec2<u32>) -> FarWalk {
    if mode == FAR_WALK_COVER {
        return FarWalk(far_leaf_cover(local, destination, occlusion.probe, origin), vec4<f32>(0.0), vec4<f32>(0.0));
    }
    return far_leaf_hidden(local, destination, occlusion, origin);
}

// Cover step of a far node for the probe disc of `radius`: nodes outside the
// disc are skipped, fully contained nodes with a known blocked area add it
// with their open part placed at their centre, others descend.
fn far_cover_step(node: IntegralNode, bounds: vec4<u32>, destination: vec2<u32>, radius: f32) -> FarStep {
    var step = FarStep(false, far_walk_empty());
    let distances = node_distance_range(bounds, destination);
    if distances.x >= radius + 0.5 { return step; }
    let blocked = far_node_blocked(node, radius);
    if distances.y <= radius - 0.5 && blocked >= 0.0 {
        let area = f32((node.bounds.z - node.bounds.x) * (node.bounds.w - node.bounds.y));
        let center = 0.5 * (vec2<f32>(bounds.xy) + vec2<f32>(bounds.zw - vec2<u32>(1u))) - vec2<f32>(destination);
        step.sums.cover = vec4<f32>(blocked, area, center * (area - blocked));
        return step;
    }
    step.descend = true;
    return step;
}

// Hidden step of a far node: nodes that hide nothing or lie outside the
// gather are skipped, nodes that every channel accepts with one hidden
// fraction add their weighted sums, others descend to exact leaves.
fn far_hidden_step(node: IntegralNode, bounds: vec4<u32>, destination: vec2<u32>, occlusion: FarOcclusion) -> FarStep {
    var step = FarStep(false, far_walk_empty());
    let hidden = far_node_hidden(node, bounds, destination, occlusion);
    if node.weighted_valid <= 0.0 || hidden == 0.0 { return step; }
    let classification = far_hidden_node_class(bounds, destination, node);
    if classification.empty_mask == CHANNEL_MASK { return step; }
    if classification.accept_mask == CHANNEL_MASK && hidden > 0.0 {
        let weights = classification.coefficient * hidden;
        step.sums.premul = node.weighted_premul * weights;
        step.sums.valid = vec4<f32>(node.weighted_valid) * weights;
        return step;
    }
    step.descend = true;
    return step;
}

// Cover of the plain probe disc of `radius` around `destination`: blocked
// area, total area and the first moment of the open area. Valid far pixels
// block with `far_blocking_weight`; invalid pixels are open.
fn far_cover(destination: vec2<u32>, radius: f32, origin: vec2<u32>, extent: vec2<u32>) -> vec4<f32> {
    var probe = far_unoccluded();
    probe.probe = radius;
    return far_walk(FAR_WALK_COVER, destination, probe, origin, extent).cover;
}

fn far_leaf_cover(local: vec2<u32>, destination: vec2<u32>, radius: f32, origin: vec2<u32>) -> vec4<f32> {
    let offset = vec2<f32>(local + origin) - vec2<f32>(destination);
    let weight = clamp(radius - length(offset) + 0.5, 0.0, 1.0);
    if weight <= 0.0 { return vec4<f32>(0.0); }
    let pixel = load_integral_pixel(local, false);
    let blocked = select(0.0, weight * pixel.valid * far_blocking_weight(pixel.coc, radius), pixel.valid > 0.0);
    return vec4<f32>(blocked, weight, offset * (weight - blocked));
}

// Occluder of the far surface at `destination`, probed with `probe`.
fn far_occlusion(destination: vec2<u32>, probe: f32, origin: vec2<u32>, extent: vec2<u32>) -> FarOcclusion {
    return far_occlusion_from_cover(probe, far_cover(destination, probe, origin, extent));
}

// Weighted far sources behind the probed surface that the occluder hides, in
// the units of the ordinary far gather and with each channel's own aperture
// weights, so that subtracting it from that gather leaves the visible sources.
// A node is accepted only when every channel is; mixed channels descend to
// exact leaves.
fn far_hidden_integral(destination: vec2<u32>, occlusion: FarOcclusion, origin: vec2<u32>, extent: vec2<u32>) -> LensIntegral {
    if occlusion.state == FAR_UNOCCLUDED { return LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0)); }
    let sums = far_walk(FAR_WALK_HIDDEN, destination, occlusion, origin, extent);
    return LensIntegral(sums.premul, sums.valid, vec4<f32>(0.0));
}

// Far node classification for the hidden integral. A plain circle needs only
// the disc test (far sources carry no aperture phase shift), which keeps the
// general aperture classifier out of the plain-circle pipeline.
fn far_hidden_node_class(bounds: vec4<u32>, destination: vec2<u32>, node: IntegralNode) -> NodeClass {
    if !PLAIN_CIRCLE {
        return aperture_node_class(bounds, destination, node.coc_min, node.coc_max, -1.0, 1.0);
    }
    var result = NodeClass(0u, 0u, vec4<f32>(0.0));
    let distances = node_distance_range(bounds, destination);
    if distances.x >= max(node.coc_max, GATHER_MIN_COC) + 0.5 {
        result.empty_mask = CHANNEL_MASK;
    } else if distances.y <= max(node.coc_min - 0.5, 0.0) && node.coc_min >= GATHER_MIN_COC {
        result.accept_mask = CHANNEL_MASK;
        result.coefficient = vec4<f32>(1.0);
    }
    return result;
}

// Hidden sums of the far leaf at `local`.
fn far_leaf_hidden(local: vec2<u32>, destination: vec2<u32>, occlusion: FarOcclusion, origin: vec2<u32>) -> FarWalk {
    var sums = far_walk_empty();
    let pixel = load_integral_pixel(local, false);
    if pixel.valid <= 0.0 || pixel.coc <= GATHER_MIN_COC { return sums; }
    let offset = vec2<f32>(local + origin) - vec2<f32>(destination);
    let hidden = far_pixel_hidden(pixel.coc, offset, occlusion);
    if hidden <= 0.0 { return sums; }
    let weights = aperture_weight(destination, offset, -pixel.coc, 1.0) * (pixel.valid * hidden);
    sums.premul = pixel.premul * weights;
    sums.valid = weights;
    return sums;
}

// Colour of the far gather whose hidden sources were already removed, leaving
// `premul` and `valid`; `hidden` is the removed validity of the alpha channel,
// whose visible part decides for every channel. When nearly everything was
// hidden the remainder is the difference of nearly equal sums, so the colour
// blends to the far surface's own colour `surface` rather than to noise or to
// the hidden background. The exact and streamed gathers share this rule;
// streaming applies it to the sums over all source chunks.
fn far_visible_color(premul: vec4<f32>, valid: vec4<f32>, hidden: f32, surface: vec4<f32>) -> vec4<f32> {
    let remainder = premul / max(valid, vec4<f32>(1.0e-6));
    if hidden <= 0.0 { return remainder; }
    let visible = valid.w / max(valid.w + hidden, 1.0e-6);
    let trust = smoothstep(FAR_HIDDEN_RESIDUE, FAR_HIDDEN_TRUSTED, visible);
    if trust <= 0.0 { return surface; }
    return mix(surface, remainder, trust);
}

// Conservative far reach of the tile holding the pixel, without the bilinear
// tile blend: sources behind the far surface can only reach the pixel when it
// exceeds the behind CoC.
fn far_tile_reach(x: u32, y: u32) -> f32 {
    let tile_count = settings.frame.z * settings.frame.w;
    if arrayLength(&near_search_radii) < tile_count * 2u { return settings.radius.x; }
    let tile = (vec2<u32>(x, y) + settings.tile_origin) / 16u;
    let clamped = min(tile, settings.frame.zw - vec2<u32>(1u));
    return near_search_radii[tile_count + clamped.y * settings.frame.z + clamped.x];
}

fn exact_pixel(position: vec2<u32>) -> vec4<f32> {
    let center = vec2<f32>(position) + vec2<f32>(0.5);
    let uv = texture_uv(center);
    let source_value = textureSampleLevel(source, linear_sampler, uv, 0.0);
    if settings.radius.x <= 0.0 { return source_value; }
    let center_near = sample_layer(center, 1.0, settings.samples.x, true);
    let center_far = sample_layer(center, 1.0, 1u, false);
    let center_blur = center_blur_weight(center_near, center_far);
    let near_radius = clamp(tile_radius(position.x, position.y, 0u), 0.0, settings.radius.x);
    let tile_count = settings.frame.z * settings.frame.w;
    var far_radius = abs(center_far.coc);
    if arrayLength(&near_search_radii) >= tile_count * 2u {
        far_radius = max(tile_radius(position.x, position.y, tile_count), far_radius);
    }
    far_radius = clamp(far_radius, 0.0, settings.radius.x);

    var front = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    var fused_potential = vec2<f32>(0.0);
    if FUSED_CAPPED_CIRCLE {
        let fused = integrate_fused_capped_circle(position);
        front = fused.front;
        fused_potential = fused.potential;
    } else {
        front = integrate_disk(position, 1.0, 1.0, true);
    }
    var potential = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    if max(max(front.valid.x, front.valid.y), max(front.valid.z, front.valid.w)) > 0.0 &&
       max(max(front.presence.x, front.presence.y), max(front.presence.z, front.presence.w)) > 0.5 {
        if FUSED_CAPPED_CIRCLE && front.presence.x == settings.radius.x {
            potential = LensIntegral(
                vec4<f32>(fused_potential.x), vec4<f32>(fused_potential.y), vec4<f32>(0.0),
            );
        } else {
            potential = integrate_potential(position, front.presence, 1.0);
        }
    }
    let occupancy = potential.premul / max(potential.valid, vec4<f32>(1.0e-6));
    let blend = front_interior_blend(occupancy);
    let denominator = potential.valid * (vec4<f32>(1.0) - blend) + front.valid * blend;
    var front_result = front.premul / max(denominator, vec4<f32>(1.0e-6));
    if front_result.w > 1.0 { front_result /= front_result.w; }
    front_result.w = clamp(front_result.w, 0.0, 1.0);

    let local = integrate_local(position);
    let local_color = local.premul / max(local.local_weight, vec4<f32>(1.0e-6));
    let local_coverage = clamp(local.local_weight / max(local.support_weight, vec4<f32>(1.0e-6)), vec4<f32>(0.0), vec4<f32>(1.0));
    let local_occupancy = clamp(local.occupied_support / max(local.support_weight, vec4<f32>(1.0e-6)), vec4<f32>(0.0), vec4<f32>(1.0));
    let local_fill = vec4<f32>(
        smoothstep(0.99, 1.0, local_occupancy.x), smoothstep(0.99, 1.0, local_occupancy.y),
        smoothstep(0.99, 1.0, local_occupancy.z), smoothstep(0.99, 1.0, local_occupancy.w),
    );
    let local_denominator = local_coverage * (vec4<f32>(1.0) - local_fill) + local_fill;
    var local_result = local_color * local_denominator;
    if local_result.w > 1.0 { local_result /= local_result.w; }
    local_result.w = clamp(local_result.w, 0.0, 1.0);

    var far = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    var hidden = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    if center_blur > 0.0 {
        far = integrate_disk(position, -1.0, 1.0, false);
        hidden = read_far_hidden(position);
    }
    let far_color_value = far_visible_color(far.premul - hidden.premul, far.valid - hidden.valid, hidden.valid.w, center_far.color);
    var far_background = vec4<f32>(0.0);
    for (var channel = 0u; channel < 4u; channel++) {
        if component(far.presence, channel) > 0.0 {
            set_component(&far_background, channel, component(far_color_value, channel));
        } else if center_near.valid > 0.0 && center_far.valid > 0.0 {
            set_component(&far_background, channel, component(center_far.color, channel));
        }
    }
    let background = source_value * (1.0 - center_blur) + far_background * center_blur;
    let under = local_result + background * (1.0 - clamp(local_result.w, 0.0, 1.0));
    return preserve_source_alpha_coverage(
        front_result + under * (1.0 - front_result.w), source_value,
    );
}

// Keep the complete source hierarchy but gather only the requested output.
// An empty region preserves the full-frame low-level diagnostic convention.
fn output_requested(position: vec2<u32>) -> bool {
    if any(lens.process_region.zw <= lens.process_region.xy) { return true; }
    let absolute = lens.full_region.xy + vec2<i32>(position);
    return all(absolute >= lens.process_region.xy) && all(absolute < lens.process_region.zw);
}

fn preserve_source_alpha_coverage(result: vec4<f32>, source_value: vec4<f32>) -> vec4<f32> {
    // Convolution cannot make a fully opaque source partially transparent.
    // Non-uniform aperture masks can otherwise create coverage holes, including
    // at a one-pixel focal crossing. Preserve the input surface in that case.
    if settings.source_opaque != 0u && source_value.w >= 1.0 - 1.0e-6 {
        if result.w > 1.0e-6 {
            return vec4<f32>(result.xyz / result.w, 1.0);
        }
        return source_value;
    }
    if result.w <= 1.0e-6 && source_value.w > 0.0 {
        return source_value;
    }
    return result;
}

@compute @workgroup_size(8, 8, 1)
fn adaptive_convolution_high(@builtin(global_invocation_id) gid: vec3<u32>) {
    let position = gid.xy;
    if position.x >= settings.frame.x || position.y >= settings.frame.y || !output_requested(position) { return; }
    let color = exact_pixel(position);
    let index = exact_output_index(position);
    output[index] = color.x;
    output[index + 1u] = color.y;
    output[index + 2u] = color.z;
    output[index + 3u] = color.w;
    output[index + 4u] = color.w;
}

fn exact_output_index(position: vec2<u32>) -> u32 {
    return (position.y * settings.frame.x + position.x) * 5u;
}

// The far occlusion pass runs before the exact gather and leaves each pixel's
// hidden far sources in that pixel's five output slots: the premultiplied sum
// and the alpha-channel validity. Apertures whose channels weigh differently
// (custom images, axial aberration) get a buffer with three more floats per
// pixel after the five-float layout for the red, green and blue validity; a
// plain disc weighs every channel alike and uses the alpha validity. The gather
// reads them before it overwrites the slots with its colour. Keeping this work
// out of the gather keeps that pipeline's register budget.
const FAR_HIDDEN_CHANNEL_FLOATS: u32 = 3u;
// Channel index of a buffer without the per-channel hidden validity.
const FAR_HIDDEN_CHANNEL_NONE: u32 = 0xffffffffu;

// Index of the red, green and blue hidden validity of `position` after the
// five-float layout, or FAR_HIDDEN_CHANNEL_NONE when the buffer has no room
// for them.
fn far_hidden_channel_index(position: vec2<u32>) -> u32 {
    // A plain disc weighs every channel alike; keeping the check out of its
    // pipelines measured about 4% of the exact gather.
    if PLAIN_CIRCLE { return FAR_HIDDEN_CHANNEL_NONE; }
    let pixels = settings.frame.x * settings.frame.y;
    if arrayLength(&output) < pixels * (5u + FAR_HIDDEN_CHANNEL_FLOATS) { return FAR_HIDDEN_CHANNEL_NONE; }
    return pixels * 5u + (position.y * settings.frame.x + position.x) * FAR_HIDDEN_CHANNEL_FLOATS;
}

fn read_far_hidden(position: vec2<u32>) -> LensIntegral {
    let index = exact_output_index(position);
    let premul = vec4<f32>(output[index], output[index + 1u], output[index + 2u], output[index + 3u]);
    var valid = vec4<f32>(output[index + 4u]);
    let channels = far_hidden_channel_index(position);
    if channels != FAR_HIDDEN_CHANNEL_NONE {
        valid = vec4<f32>(output[channels], output[channels + 1u], output[channels + 2u], valid.w);
    }
    return LensIntegral(premul, valid, vec4<f32>(0.0));
}

// Far sources hidden at `position` by the far surface in front of them, or
// zero when the far gather there crosses no depth gap.
fn far_hidden_at(position: vec2<u32>) -> LensIntegral {
    let none = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    if settings.radius.x <= 0.0 { return none; }
    let center = vec2<f32>(position) + vec2<f32>(0.5);
    let center_far = sample_layer(center, 1.0, 1u, false);
    let probe = far_probe_radius(center_far);
    if !far_behind_reachable(far_tile_reach(position.x, position.y), probe) { return none; }
    let center_near = sample_layer(center, 1.0, settings.samples.x, true);
    if center_blur_weight(center_near, center_far) <= 0.0 { return none; }
    let occlusion = far_occlusion(position, probe, vec2<u32>(0u), settings.frame.xy);
    return far_hidden_integral(position, occlusion, vec2<u32>(0u), settings.frame.xy);
}

@compute @workgroup_size(8, 8, 1)
fn adaptive_far_occlusion(@builtin(global_invocation_id) gid: vec3<u32>) {
    let position = gid.xy;
    if position.x >= settings.frame.x || position.y >= settings.frame.y || !output_requested(position) { return; }
    let hidden = far_hidden_at(position);
    let index = exact_output_index(position);
    output[index] = hidden.premul.x;
    output[index + 1u] = hidden.premul.y;
    output[index + 2u] = hidden.premul.z;
    output[index + 3u] = hidden.premul.w;
    output[index + 4u] = hidden.valid.w;
    let channels = far_hidden_channel_index(position);
    if channels != FAR_HIDDEN_CHANNEL_NONE {
        output[channels] = hidden.valid.x;
        output[channels + 1u] = hidden.valid.y;
        output[channels + 2u] = hidden.valid.z;
    }
}


fn detail_weight(position: vec2<u32>) -> f32 {
    let coordinate = (vec2<f32>(position) + vec2<f32>(settings.tile_origin)) / 16.0 - vec2<f32>(0.46875);
    let base = floor(coordinate);
    let fraction = fract(coordinate);
    let maximum = vec2<f32>(settings.frame.zw) - vec2<f32>(1.0);
    var weights: vec4<f32>;
    for (var i = 0u; i < 4u; i++) {
        let p = vec2<u32>(clamp(base + vec2<f32>(f32(i % 2u), f32(i / 2u)), vec2<f32>(0.0), maximum));
        let tile_class = tile_classes[p.y * settings.frame.z + p.x];
        weights[i] = select(0.0, 1.0, (tile_class & 0x80000000u) != 0u || ((tile_class & 0x7fffffffu) >= 2u));
    }
    return mix(mix(weights.x, weights.y, fraction.x), mix(weights.z, weights.w, fraction.x), fraction.y);
}
fn sampled_weight(destination: vec2<u32>, distance: vec2<f32>, coc: f32, search_radius: f32, footprint: f32) -> vec4<f32> {
    let lens_active = (lens.flags & (GLOBAL_USE_NON_UNIFORM | GLOBAL_AXIAL_ABERRATION)) != 0u;
    if !lens_active && (lens.flags & GLOBAL_SIMPLE_DISC) != 0u {
        let radius = abs(coc);
        let shifted = distance + select(vec2<f32>(0.0), settings.radius.zw * radius, coc > 0.0);
        let coverage = clamp((radius - length(shifted)) / footprint + 0.5, 0.0, 1.0);
        return vec4<f32>(search_radius * search_radius / max(radius * radius, 0.25) * coverage);
    }
    return aperture_weight_with_footprint(destination, distance, coc, search_radius, footprint);
}
// Sampled counterpart of `far_cover` over the smooth pattern of the budget:
// each in-frame sample of the far layer inside the probe disc counts once.
fn sampled_far_cover(center: vec2<f32>, radius: f32) -> vec4<f32> {
    var cover = vec4<f32>(0.0);
    for (var i = 0u; i < settings.samples.x; i++) {
        let offset_index = settings.samples.z + i * 2u;
        let offset = vec2<f32>(sample_offsets[offset_index], sample_offsets[offset_index + 1u]) * radius;
        let position = center + offset;
        if position.x >= 0.5 && position.y >= 0.5 && position.x <= f32(settings.frame.x) - 0.5 && position.y <= f32(settings.frame.y) - 0.5 {
            let sample = sample_layer(position, radius, settings.samples.x, false);
            let blocked = max(sample.valid, 0.0) * far_blocking_weight(abs(sample.coc), radius);
            cover += vec4<f32>(blocked, 1.0, offset * (1.0 - blocked));
        }
    }
    return cover;
}
// Weighted colour and weight sums of sampled far sources.
struct FarSum {
    sum: vec4<f32>,
    weight: vec4<f32>,
};

// Far sources sampled with the area-distributed pattern of `count` offsets at
// `offset_base`, scaled to the disc of `radius` around `destination`, without
// the offsets inside `inner`, which a denser pass covers. Hidden sources are
// weighted out with `occlusion`. Each sample stands for the disc area over
// `count`, so passes with different counts add up.
fn sampled_far_disc(destination: vec2<u32>, count: u32, offset_base: u32, radius: f32, inner: f32, occlusion: FarOcclusion) -> FarSum {
    let center = vec2<f32>(destination) + vec2<f32>(0.5);
    let axis_count = select(1.0, select(8.0, 16.0, count >= 256u), count >= 64u);
    let sample_area = 1.0 / f32(max(count, 1u));
    var result = FarSum(vec4<f32>(0.0), vec4<f32>(0.0));
    for (var i = 0u; i < count; i++) {
        let offset_index = offset_base + i * 2u;
        let far_offset = vec2<f32>(sample_offsets[offset_index], sample_offsets[offset_index + 1u]) * radius;
        if length(far_offset) < inner { continue; }
        let sample = sample_layer(center + far_offset, radius, count, false);
        if sample.valid > 0.0 && abs(sample.coc) > GATHER_MIN_COC {
            let weight = sampled_weight(destination, far_offset, sample.coc, radius, max(radius / axis_count, 1.0))
                * (sample_area * sample.valid * (1.0 - far_pixel_hidden(abs(sample.coc), far_offset, occlusion)));
            result.sum += sample.color * weight;
            result.weight += weight;
        }
    }
    return result;
}

// The far gather weights a source by the inverse square of its CoC, so the
// least blurred sources, which only reach the destination from close by,
// dominate near the destination. An area-distributed pattern over a wide far
// disc gives that close region a few filtered samples: a thin far band blurred
// by a few pixels lost its colour to the strongly blurred surfaces around it,
// and a less blurred silhouette drew ribs over the wide blur beside it. The
// disc of the probe radius, at most FAR_INNER_FRACTION of the far radius, is
// therefore sampled separately with the smooth pattern and excluded from the
// wide pass. Both passes estimate the same area integral, so the split adds
// no seam.
const FAR_INNER_FRACTION: f32 = 0.25;

fn sampled_far_gather(destination: vec2<u32>, count: u32, offset_base: u32, far_radius: f32, probe: f32, occlusion: FarOcclusion) -> FarSum {
    let inner_radius = min(probe, FAR_INNER_FRACTION * far_radius);
    let outer = sampled_far_disc(destination, count, offset_base, far_radius, inner_radius, occlusion);
    let inner = sampled_far_disc(destination, settings.samples.x, settings.samples.z, inner_radius, 0.0, occlusion);
    return FarSum(outer.sum + inner.sum, outer.weight + inner.weight);
}

// A sampled pixel and the transmittance of its front layer, the blurred
// foreground: the part of everything under it that stays visible.
struct SampledColor {
    color: vec4<f32>,
    front_transmittance: f32,
};

fn sampled_gather(destination: vec2<u32>, count: u32, offset_base: u32, near_radius: f32, far_radius: f32, source_value: vec4<f32>, center_near: LayerSample, center_far: LayerSample) -> SampledColor {
    let center = vec2<f32>(destination) + vec2<f32>(0.5);
    let lens_active = (lens.flags & (GLOBAL_USE_NON_UNIFORM | GLOBAL_AXIAL_ABERRATION)) != 0u || (lens.flags & GLOBAL_SIMPLE_DISC) == 0u;
    var local_sum = vec4<f32>(0.0);
    var local_weight = vec4<f32>(0.0);
    var support_weight = vec4<f32>(0.0);
    var occupied_support = vec4<f32>(0.0);
    var front_sum = vec4<f32>(0.0);
    var front_weight = vec4<f32>(0.0);
    var occupied_area = vec4<f32>(0.0);
    var available_area = vec4<f32>(0.0);
    let axis_count = select(1.0, select(8.0, 16.0, count >= 256u), count >= 64u);
    let center_blur = center_blur_weight(center_near, center_far);
    var occlusion = far_unoccluded();
    let probe = far_probe_radius(center_far);
    if center_blur > 0.0 && far_behind_reachable(far_radius, probe) {
        occlusion = far_occlusion_from_cover(probe, sampled_far_cover(center, probe));
    }
    for (var i = 0u; i < count; i++) {
        let offset_index = offset_base + i * 2u;
        let offset = vec2<f32>(sample_offsets[offset_index], sample_offsets[offset_index + 1u]);
        if near_radius > 0.5 {
            let pattern_radius = length(offset);
            let area_weight = 2.0 * pattern_radius * pattern_radius;
            let near_offset = offset * (near_radius * pattern_radius);
            let position = center + near_offset;
            let sample_radius = near_radius * pattern_radius * 1.41421356237;
            let footprint = sample_radius / axis_count;
            let sample = sample_layer(position, sample_radius, count, true);
            let optical_radius = abs(sample.coc);
            let front_fraction = smoothstep(1.0, 2.0, optical_radius);
            if position.x >= 0.5 && position.y >= 0.5 && position.x <= f32(settings.frame.x) - 0.5 && position.y <= f32(settings.frame.y) - 0.5 {
                let potential = sampled_weight(destination, near_offset, near_radius, near_radius, footprint) * area_weight;
                available_area += potential;
                occupied_area += potential * (sample.valid * front_fraction);
                let support = sampled_weight(destination, near_offset, 2.0, near_radius, 1.0) * area_weight;
                support_weight += support;
                occupied_support += support * sample.valid;
            }
            if sample.valid > 0.0 && optical_radius > 0.5 {
                let weight = sampled_weight(destination, near_offset, sample.coc, near_radius, footprint) * (sample.valid * area_weight);
                let local = weight * (1.0 - front_fraction);
                let front = weight * front_fraction;
                local_sum += sample.color * local;
                local_weight += local;
                front_sum += sample.color * front;
                front_weight += front;
            }
        }
    }
    let far = sampled_far_gather(destination, count, offset_base, far_radius, probe, occlusion);
    let far_sum = far.sum;
    var far_weight = far.weight;
    if !lens_active {
        local_weight = vec4<f32>(local_weight.w);
        support_weight = vec4<f32>(support_weight.w);
        occupied_support = vec4<f32>(occupied_support.w);
        front_weight = vec4<f32>(front_weight.w);
        occupied_area = vec4<f32>(occupied_area.w);
        available_area = vec4<f32>(available_area.w);
        far_weight = vec4<f32>(far_weight.w);
    }
    var local_result = vec4<f32>(0.0);
    if local_weight.w > 0.0 && support_weight.w > 0.0 {
        let coverage = clamp(local_weight / max(support_weight, vec4<f32>(1.0e-6)), vec4<f32>(0.0), vec4<f32>(1.0));
        let occupancy = clamp(occupied_support / max(support_weight, vec4<f32>(1.0e-6)), vec4<f32>(0.0), vec4<f32>(1.0));
        var fill: vec4<f32>;
        for (var channel = 0u; channel < 4u; channel++) { fill[channel] = smoothstep(0.99, 1.0, occupancy[channel]); }
        local_result = local_sum / max(local_weight, vec4<f32>(1.0e-6)) * (coverage * (vec4<f32>(1.0) - fill) + fill);
        if local_result.w > 1.0 { local_result /= local_result.w; }
        local_result.w = clamp(local_result.w, 0.0, 1.0);
    }
    var far_background = vec4<f32>(0.0);
    if far_weight.w > 0.0 { far_background = far_sum / max(far_weight, vec4<f32>(1.0e-6)); }
    else if center_near.valid > 0.0 && center_far.valid > 0.0 { far_background = center_far.color; }
    let background = source_value * (1.0 - center_blur) + far_background * center_blur;
    let under = local_result + background * (1.0 - local_result.w);
    var front_result = vec4<f32>(0.0);
    if front_weight.w > 0.0 && available_area.w > 0.0 {
        let occupancy = occupied_area / max(available_area, vec4<f32>(1.0e-6));
        let fill = front_interior_blend(occupancy);
        let denominator = available_area * (vec4<f32>(1.0) - fill) + front_weight * fill;
        front_result = front_sum / max(denominator, vec4<f32>(1.0e-6));
        if front_result.w > 1.0 { front_result /= front_result.w; }
        front_result.w = clamp(front_result.w, 0.0, 1.0);
    }
    let color = preserve_source_alpha_coverage(
        front_result + under * (1.0 - front_result.w), source_value,
    );
    return SampledColor(color, 1.0 - front_result.w);
}
fn sampled_pixel(position: vec2<u32>) -> SampledColor {
    let center = vec2<f32>(position) + vec2<f32>(0.5);
    let source_value = textureLoad(source, vec2<i32>(position), 0);
    let class_position = (position + settings.tile_origin) / 16u;
    let tile_class = tile_classes[class_position.y * settings.frame.z + class_position.x];
    if settings.radius.x <= 0.0 || ((lens.flags & (GLOBAL_SIMPLE_DISC | GLOBAL_USE_NON_UNIFORM | GLOBAL_AXIAL_ABERRATION)) == GLOBAL_SIMPLE_DISC && tile_class == 0u) { return SampledColor(source_value, 1.0); }
    let center_near = sample_layer(center, 1.0, settings.samples.x, true);
    let center_far = sample_layer(center, 1.0, 1u, false);
    var near_radius = clamp(tile_radius(position.x, position.y, 0u), 0.0, settings.radius.x);
    var far_radius = abs(center_far.coc);
    let tile_count = settings.frame.z * settings.frame.w;
    if arrayLength(&near_search_radii) >= tile_count * 2u { far_radius = max(tile_radius(position.x, position.y, tile_count), far_radius); }
    far_radius = clamp(far_radius, 0.0, settings.radius.x);
    if (lens.flags & GLOBAL_USE_NON_UNIFORM) != 0u && (lens.non_uniform_flags & 0x10u) != 0u {
        near_radius = settings.radius.x;
        far_radius = max(settings.radius.x, 1.0);
    }
    let aperture_active = (lens.flags & GLOBAL_SIMPLE_DISC) == 0u;
    let detail = detail_weight(position);
    if aperture_active || detail >= 1.0 {
        return sampled_gather(position, settings.samples.y, settings.samples.w, near_radius, far_radius, source_value, center_near, center_far);
    }
    let smooth_result = sampled_gather(position, settings.samples.x, settings.samples.z, near_radius, far_radius, source_value, center_near, center_far);
    if detail <= 0.0 { return smooth_result; }
    let detailed = sampled_gather(position, settings.samples.y, settings.samples.w, near_radius, far_radius, source_value, center_near, center_far);
    return SampledColor(mix(smooth_result.color, detailed.color, detail), mix(smooth_result.front_transmittance, detailed.front_transmittance, detail));
}
@compute @workgroup_size(8, 8, 1)
fn adaptive_convolution_sampled(@builtin(global_invocation_id) gid: vec3<u32>) {
    let position = gid.xy;
    if position.x >= settings.frame.x || position.y >= settings.frame.y || !output_requested(position) { return; }
    let sampled = sampled_pixel(position);
    let index = (position.y * settings.frame.x + position.x) * 5u;
    output[index] = sampled.color.x;
    output[index + 1u] = sampled.color.y;
    output[index + 2u] = sampled.color.z;
    output[index + 3u] = sampled.color.w;
    // The fifth slot carries the front transmittance, which bounds how much
    // source detail the half-resolution Fast path adds back.
    output[index + 4u] = sampled.front_transmittance;
}

// Streaming uses source chunks in full-frame-local coordinates. Tree and
// texture indices remain chunk-local; origins are applied only for geometry.
struct AdaptiveStreamingSettings {
    source_origin: vec2<u32>,
    destination_origin: vec2<u32>,
    source_extent: vec2<u32>,
    output_extent: vec2<u32>,
    phase: u32,
    _padding0: u32,
    _padding1: u32,
    _padding2: u32,
};

@group(0) @binding(17) var<uniform> stream: AdaptiveStreamingSettings;

const STREAM_STRIDE: u32 = 72u;

fn stream_bounds(bounds: vec4<u32>) -> vec4<u32> {
    return bounds + vec4<u32>(stream.source_origin, stream.source_origin);
}

fn stream_global_leaf(position: vec2<u32>) -> vec2<u32> {
    return position + stream.source_origin;
}

fn stream_load_node(reference: u32, near_tree: bool) -> IntegralNode {
    var node: IntegralNode;
    if near_tree { node = near_nodes[reference]; }
    else { node = far_nodes[reference]; }
    node.bounds = stream_bounds(node.bounds);
    return node;
}

fn stream_output_index(position: vec2<u32>) -> u32 {
    return (position.y * stream.output_extent.x + position.x) * STREAM_STRIDE;
}

fn stream_add_vec4(base: u32, offset: u32, value: vec4<f32>) {
    output[base + offset] += value.x;
    output[base + offset + 1u] += value.y;
    output[base + offset + 2u] += value.z;
    output[base + offset + 3u] += value.w;
}

fn stream_read_vec4(base: u32, offset: u32) -> vec4<f32> {
    return vec4<f32>(output[base + offset], output[base + offset + 1u], output[base + offset + 2u], output[base + offset + 3u]);
}

fn stream_write_vec4(base: u32, offset: u32, value: vec4<f32>) {
    output[base + offset] = value.x;
    output[base + offset + 1u] = value.y;
    output[base + offset + 2u] = value.z;
    output[base + offset + 3u] = value.w;
}

fn stream_integrate_disk(destination: vec2<u32>, sign_value: f32, search_radius: f32, near_tree: bool) -> LensIntegral {
    var result = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    if any(stream.source_extent == vec2<u32>(0u)) { return result; }
    if all(stream.source_extent == vec2<u32>(1u)) {
        let pixel = load_integral_pixel(vec2<u32>(0u), near_tree);
        if pixel.valid > 0.0 && pixel.coc > 0.5 {
            let global_position = stream_global_leaf(vec2<u32>(0u));
            let weight = aperture_weight(destination, vec2<f32>(global_position) - vec2<f32>(destination), sign_value * pixel.coc, search_radius);
            result.premul = pixel.premul * pixel.valid * weight;
            result.valid = vec4<f32>(pixel.valid) * weight;
            result.presence = select(vec4<f32>(0.0), vec4<f32>(pixel.coc), weight > vec4<f32>(0.0));
        }
        return result;
    }
    let node_count = tree_length(near_tree);
    var refs: array<u32, 64>;
    var masks: array<u32, 64>;
    refs[0] = 0u;
    masks[0] = CHANNEL_MASK;
    var stack_len = 1u;
    while stack_len > 0u {
        stack_len -= 1u;
        let reference = refs[stack_len];
        let active_mask = masks[stack_len];
        if reference != EMPTY_REF && active_mask != 0u {
            if (reference & LEAF_REF_BIT) != 0u {
                let index = reference & ~LEAF_REF_BIT;
                let local = vec2<u32>(index % stream.source_extent.x, index / stream.source_extent.x);
                if local.y < stream.source_extent.y {
                    let pixel = load_integral_pixel(local, near_tree);
                    if pixel.valid > 0.0 && pixel.coc > 0.5 {
                        let position = stream_global_leaf(local);
                        let weights = aperture_weight(destination, vec2<f32>(position) - vec2<f32>(destination), sign_value * pixel.coc, search_radius);
                        let active_weights = weights * channel_mask_vec(active_mask);
                        result.premul += pixel.premul * pixel.valid * active_weights;
                        result.valid += vec4<f32>(pixel.valid) * active_weights;
                        result.presence = max(result.presence, select(vec4<f32>(0.0), vec4<f32>(pixel.coc), active_weights > vec4<f32>(0.0)));
                    }
                }
            } else if reference < node_count {
                let node = stream_load_node(reference, near_tree);
                if node.weighted_valid > 0.0 {
                    let classification = aperture_node_class(node.bounds, destination, node.coc_min, node.coc_max, sign_value, search_radius);
                    let active_after_empty = active_mask & ~classification.empty_mask;
                    let accepted = active_after_empty & classification.accept_mask;
                    let accepted_weights = classification.coefficient * channel_mask_vec(accepted);
                    result.premul += node.weighted_premul * accepted_weights;
                    result.valid += vec4<f32>(node.weighted_valid) * accepted_weights;
                    result.presence = max(result.presence, select(vec4<f32>(0.0), vec4<f32>(node.coc_max), accepted_weights > vec4<f32>(0.0)));
                    let descend = active_after_empty & ~classification.accept_mask;
                    for (var child_index = 4u; child_index > 0u; child_index--) {
                        let child = node.children[child_index - 1u];
                        if child != EMPTY_REF && descend != 0u && stack_len < 64u {
                            refs[stack_len] = child;
                            masks[stack_len] = descend;
                            stack_len += 1u;
                        }
                    }
                }
            }
        }
    }
    return result;
}

fn stream_integrate_potential(destination: vec2<u32>, radii: vec4<f32>, sign_value: f32) -> LensIntegral {
    var result = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    if any(stream.source_extent == vec2<u32>(0u)) { return result; }
    let same_radius = all(radii == vec4<f32>(radii.x));
    if all(stream.source_extent == vec2<u32>(1u)) {
        let pixel = load_integral_pixel(vec2<u32>(0u), true);
        let position = stream_global_leaf(vec2<u32>(0u));
        let distance = vec2<f32>(position) - vec2<f32>(destination);
        let occupied = pixel.valid;
        if same_radius {
            let weights = aperture_weight(destination, distance, sign_value * radii.x, 1.0);
            result.premul = vec4<f32>(occupied) * weights;
            result.valid = weights;
            result.presence = select(vec4<f32>(0.0), radii, weights > vec4<f32>(0.0));
        } else {
            for (var channel = 0u; channel < 4u; channel++) {
                let radius = component(radii, channel);
                if radius > 0.5 {
                    let weight = component(aperture_weight(destination, distance, sign_value * radius, 1.0), channel);
                    result.premul = with_added_component(result.premul, channel, occupied * weight);
                    result.valid = with_added_component(result.valid, channel, weight);
                    if weight > 0.0 { result.presence = with_component(result.presence, channel, radius); }
                }
            }
        }
        return result;
    }
    let node_count = arrayLength(&near_nodes);
    var refs: array<u32, 64>;
    var masks: array<u32, 64>;
    refs[0] = 0u;
    masks[0] = 0u;
    for (var channel = 0u; channel < 4u; channel++) {
        if component(radii, channel) > 0.5 { masks[0] |= 1u << channel; }
    }
    var stack_len = 1u;
    while stack_len > 0u {
        stack_len -= 1u;
        let reference = refs[stack_len];
        let active_mask = masks[stack_len];
        if reference != EMPTY_REF && active_mask != 0u {
            if (reference & LEAF_REF_BIT) != 0u {
                let index = reference & ~LEAF_REF_BIT;
                let local = vec2<u32>(index % stream.source_extent.x, index / stream.source_extent.x);
                if local.y < stream.source_extent.y {
                    let pixel = load_integral_pixel(local, true);
                    let position = stream_global_leaf(local);
                    if same_radius {
                        let weights = aperture_weight(destination, vec2<f32>(position) - vec2<f32>(destination), sign_value * radii.x, 1.0) * channel_mask_vec(active_mask);
                        result.premul += weights * pixel.valid;
                        result.valid += weights;
                        result.presence = max(result.presence, select(vec4<f32>(0.0), radii, weights > vec4<f32>(0.0)));
                    } else {
                        for (var channel = 0u; channel < 4u; channel++) {
                            let bit = 1u << channel;
                            if (active_mask & bit) != 0u {
                                let weight = component(aperture_weight(destination, vec2<f32>(position) - vec2<f32>(destination), sign_value * component(radii, channel), 1.0), channel);
                                result.premul = with_added_component(result.premul, channel, pixel.valid * weight);
                                result.valid = with_added_component(result.valid, channel, weight);
                                if weight > 0.0 { result.presence = with_component(result.presence, channel, component(radii, channel)); }
                            }
                        }
                    }
                }
            } else if reference < node_count {
                let node = stream_load_node(reference, true);
                var descend = active_mask;
                if same_radius {
                    let classification = aperture_node_class(node.bounds, destination, radii.x, radii.x, sign_value, 1.0);
                    descend &= ~classification.empty_mask;
                    let accepted = descend & classification.accept_mask;
                    let coefficients = classification.coefficient * channel_mask_vec(accepted) / max(radii.x * radii.x, 0.25);
                    let area = f32((node.bounds.z - node.bounds.x) * (node.bounds.w - node.bounds.y));
                    result.premul += vec4<f32>(node.occupied_area) * coefficients;
                    result.valid += vec4<f32>(area) * coefficients;
                    result.presence = max(result.presence, select(vec4<f32>(0.0), radii, coefficients > vec4<f32>(0.0)));
                    descend &= ~classification.accept_mask;
                } else {
                    for (var channel = 0u; channel < 4u; channel++) {
                        let bit = 1u << channel;
                        if (active_mask & bit) != 0u {
                            let radius = component(radii, channel);
                            let classification = aperture_node_class(node.bounds, destination, radius, radius, sign_value, 1.0);
                            if (classification.empty_mask & bit) != 0u { descend &= ~bit; }
                            else if (classification.accept_mask & bit) != 0u {
                                let coefficient = component(classification.coefficient, channel) / max(radius * radius, 0.25);
                                let area = f32((node.bounds.z - node.bounds.x) * (node.bounds.w - node.bounds.y));
                                result.premul = with_added_component(result.premul, channel, node.occupied_area * coefficient);
                                result.valid = with_added_component(result.valid, channel, area * coefficient);
                                result.presence = with_component(result.presence, channel, radius);
                                descend &= ~bit;
                            }
                        }
                    }
                }
                for (var child_index = 4u; child_index > 0u; child_index--) {
                    let child = node.children[child_index - 1u];
                    if child != EMPTY_REF && descend != 0u && stack_len < 64u {
                        refs[stack_len] = child;
                        masks[stack_len] = descend;
                        stack_len += 1u;
                    }
                }
            }
        }
    }
    return result;
}

fn stream_integrate_local(destination: vec2<u32>) -> LocalIntegral {
    var result = LocalIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
    if all(stream.source_extent == vec2<u32>(1u)) {
        let local = vec2<u32>(0u);
        let position = stream_global_leaf(local);
        let metadata = textureLoad(near_metadata, vec2<i32>(local), 0);
        let valid = clamp(metadata.y, 0.0, 1.0);
        let distance = vec2<f32>(position) - vec2<f32>(destination);
        let support = aperture_weight(destination, distance, 2.0, 1.0);
        result.support_weight = support;
        result.occupied_support = support * valid;
        if valid > 0.0 {
            let coc = metadata.x / valid;
            if coc > 0.5 {
                let t = clamp(coc - 1.0, 0.0, 1.0);
                let local_fraction = 1.0 - t * t * (3.0 - 2.0 * t);
                if local_fraction > 0.0 {
                    let color = textureLoad(near_color, vec2<i32>(local), 0) / valid;
                    let weight = aperture_weight(destination, distance, coc, 1.0) * (valid * local_fraction);
                    result.premul = color * weight;
                    result.local_weight = weight;
                }
            }
        }
        return result;
    }
    let node_count = arrayLength(&near_nodes);
    var refs: array<u32, 64>;
    refs[0] = 0u;
    var stack_len = 1u;
    let full_traversal = (lens.flags & (GLOBAL_USE_NON_UNIFORM | GLOBAL_AXIAL_ABERRATION)) != 0u;
    var local_support_radius = 2.5;
    if PLAIN_CIRCLE || (lens.flags & GLOBAL_SIMPLE_DISC) != 0u {
        if (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u && lens.filter_resolution.x > 0u && lens.filter_resolution.y > 0u {
            local_support_radius += length(4.0 / vec2<f32>(lens.filter_resolution));
        } else { local_support_radius += length(settings.radius.zw * 2.0); }
    } else if (lens.flags & GLOBAL_INVERSE_FOREGROUND) != 0u && lens.filter_resolution.x > 0u && lens.filter_resolution.y > 0u {
        local_support_radius += length(4.0 / vec2<f32>(lens.filter_resolution));
    }
    while stack_len > 0u {
        stack_len -= 1u;
        let reference = refs[stack_len];
        if reference != EMPTY_REF {
            if (reference & LEAF_REF_BIT) != 0u {
                let index = reference & ~LEAF_REF_BIT;
                let local = vec2<u32>(index % stream.source_extent.x, index / stream.source_extent.x);
                if local.y < stream.source_extent.y {
                    let position = stream_global_leaf(local);
                    let metadata = textureLoad(near_metadata, vec2<i32>(local), 0);
                    let valid = clamp(metadata.y, 0.0, 1.0);
                    let distance = vec2<f32>(position) - vec2<f32>(destination);
                    let support = aperture_weight(destination, distance, 2.0, 1.0);
                    result.support_weight += support;
                    result.occupied_support += support * valid;
                    if valid > 0.0 {
                        let coc = metadata.x / valid;
                        if coc > 0.5 {
                            let t = clamp(coc - 1.0, 0.0, 1.0);
                            let local_fraction = 1.0 - t * t * (3.0 - 2.0 * t);
                            if local_fraction > 0.0 {
                                let color = textureLoad(near_color, vec2<i32>(local), 0) / valid;
                                let weight = aperture_weight(destination, distance, coc, 1.0) * (valid * local_fraction);
                                result.premul += color * weight;
                                result.local_weight += weight;
                            }
                        }
                    }
                }
            } else if reference < node_count {
                let node = stream_load_node(reference, true);
                let bounds = node.bounds;
                var include = full_traversal;
                if full_traversal {
                    include = aperture_node_class(bounds, destination, 0.5, 2.0, 1.0, 1.0).empty_mask != CHANNEL_MASK;
                } else {
                    let min_x = max(max(f32(bounds.x) - f32(destination.x), 0.0), f32(destination.x) - f32(bounds.z - 1u));
                    let min_y = max(max(f32(bounds.y) - f32(destination.y), 0.0), f32(destination.y) - f32(bounds.w - 1u));
                    include = min_x * min_x + min_y * min_y <= local_support_radius * local_support_radius;
                }
                if include {
                    for (var child_index = 4u; child_index > 0u; child_index--) {
                        let child = node.children[child_index - 1u];
                        if child != EMPTY_REF && stack_len < 64u {
                            refs[stack_len] = child;
                            stack_len += 1u;
                        }
                    }
                }
            }
        }
    }
    return result;
}

fn stream_center_phase(position: vec2<u32>, base: u32) {
    let source_local = stream.destination_origin - stream.source_origin + position;
    let center = vec2<f32>(source_local) + vec2<f32>(0.5);
    let source_value = textureSampleLevel(source, linear_sampler, texture_uv(center), 0.0);
    let center_near = sample_layer(center, 1.0, settings.samples.x, true);
    let center_far = sample_layer(center, 1.0, 1u, false);
    let global_position = stream.destination_origin + position;
    let tile_count = settings.frame.z * settings.frame.w;
    let far_radius = max(abs(center_far.coc), tile_radius(global_position.x, global_position.y, tile_count));
    let far_reach = far_tile_reach(global_position.x, global_position.y);
    stream_write_vec4(base, 0u, source_value);
    stream_write_vec4(base, 4u, center_near.color);
    output[base + 8u] = center_near.coc;
    output[base + 9u] = center_near.valid;
    stream_write_vec4(base, 10u, center_far.color);
    output[base + 14u] = center_far.coc;
    output[base + 15u] = center_far.valid;
    stream_write_vec4(base, 16u, vec4<f32>(0.0, far_radius, far_reach, 0.0));
}

// Probe radius of the far occluder at the accumulator `base`, or -1 when no
// source behind the far surface can reach the pixel. Slot 16 sums the hidden
// far validity over the source chunks for the final `far_visible_color` and
// slot 18 holds the far tile reach; the cover areas (blocked, total, open moment) accumulate in the result slots 68
// to 71, which only the final phase overwrites, so the 72-float layout holds.
fn stream_far_probe_radius(base: u32) -> f32 {
    let center_near = LayerSample(stream_read_vec4(base, 4u), output[base + 8u], output[base + 9u]);
    let center_far = LayerSample(stream_read_vec4(base, 10u), output[base + 14u], output[base + 15u]);
    if center_blur_weight(center_near, center_far) <= 0.0 { return -1.0; }
    let probe = far_probe_radius(center_far);
    if !far_behind_reachable(output[base + 18u], probe) { return -1.0; }
    return probe;
}

@compute @workgroup_size(8, 8, 1)
fn adaptive_convolution_stream_high(@builtin(global_invocation_id) gid: vec3<u32>) {
    let local = gid.xy;
    if local.x >= stream.output_extent.x || local.y >= stream.output_extent.y { return; }
    let base = stream_output_index(local);
    let destination = stream.destination_origin + local;
    if stream.phase == 0u {
        let front = stream_integrate_disk(destination, 1.0, 1.0, true);
        let probe = stream_far_probe_radius(base);
        if probe > 0.0 {
            stream_add_vec4(base, 68u, far_cover(destination, probe, stream.source_origin, stream.source_extent));
        }
        stream_add_vec4(base, 20u, front.premul);
        stream_add_vec4(base, 24u, front.valid);
        stream_write_vec4(base, 28u, max(stream_read_vec4(base, 28u), front.presence));
    } else if stream.phase == 1u {
        let front_presence = stream_read_vec4(base, 28u);
        let potential = stream_integrate_potential(destination, front_presence, 1.0);
        let local_integral = stream_integrate_local(destination);
        let center_near = LayerSample(stream_read_vec4(base, 4u), output[base + 8u], output[base + 9u]);
        let center_far = LayerSample(stream_read_vec4(base, 10u), output[base + 14u], output[base + 15u]);
        let center_blur = center_blur_weight(center_near, center_far);
        var far = LensIntegral(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
        if center_blur > 0.0 {
            far = stream_integrate_disk(destination, -1.0, 1.0, false);
            let probe = stream_far_probe_radius(base);
            if probe > 0.0 {
                let occlusion = far_occlusion_from_cover(probe, stream_read_vec4(base, 68u));
                let hidden = far_hidden_integral(destination, occlusion, stream.source_origin, stream.source_extent);
                far.premul -= hidden.premul;
                far.valid -= hidden.valid;
                output[base + 16u] += hidden.valid.w;
            }
        }
        stream_add_vec4(base, 32u, potential.premul);
        stream_add_vec4(base, 36u, potential.valid);
        stream_add_vec4(base, 40u, local_integral.premul);
        stream_add_vec4(base, 44u, local_integral.local_weight);
        stream_add_vec4(base, 48u, local_integral.support_weight);
        stream_add_vec4(base, 52u, local_integral.occupied_support);
        stream_add_vec4(base, 56u, far.premul);
        stream_add_vec4(base, 60u, far.valid);
        stream_write_vec4(base, 64u, max(stream_read_vec4(base, 64u), far.presence));
    } else if stream.phase == 2u {
        stream_center_phase(local, base);
    } else {
        let source_value = stream_read_vec4(base, 0u);
        let center_near = LayerSample(stream_read_vec4(base, 4u), output[base + 8u], output[base + 9u]);
        let center_far = LayerSample(stream_read_vec4(base, 10u), output[base + 14u], output[base + 15u]);
        let front_premul = stream_read_vec4(base, 20u);
        let front_valid = stream_read_vec4(base, 24u);
        let potential_premul = stream_read_vec4(base, 32u);
        let potential_valid = stream_read_vec4(base, 36u);
        let occupancy = potential_premul / max(potential_valid, vec4<f32>(1.0e-6));
        let blend = front_interior_blend(occupancy);
        let denominator = potential_valid * (vec4<f32>(1.0) - blend) + front_valid * blend;
        var front_result = front_premul / max(denominator, vec4<f32>(1.0e-6));
        if front_result.w > 1.0 { front_result /= front_result.w; }
        front_result.w = clamp(front_result.w, 0.0, 1.0);

        let local_premul = stream_read_vec4(base, 40u);
        let local_weight = stream_read_vec4(base, 44u);
        let support_weight = stream_read_vec4(base, 48u);
        let occupied_support = stream_read_vec4(base, 52u);
        let local_color = local_premul / max(local_weight, vec4<f32>(1.0e-6));
        let local_coverage = clamp(local_weight / max(support_weight, vec4<f32>(1.0e-6)), vec4<f32>(0.0), vec4<f32>(1.0));
        let local_occupancy = clamp(occupied_support / max(support_weight, vec4<f32>(1.0e-6)), vec4<f32>(0.0), vec4<f32>(1.0));
        let local_fill = vec4<f32>(smoothstep(0.99, 1.0, local_occupancy.x), smoothstep(0.99, 1.0, local_occupancy.y), smoothstep(0.99, 1.0, local_occupancy.z), smoothstep(0.99, 1.0, local_occupancy.w));
        let local_denominator = local_coverage * (vec4<f32>(1.0) - local_fill) + local_fill;
        var local_result = local_color * local_denominator;
        if local_result.w > 1.0 { local_result /= local_result.w; }
        local_result.w = clamp(local_result.w, 0.0, 1.0);

        let far_premul = stream_read_vec4(base, 56u);
        let far_valid = stream_read_vec4(base, 60u);
        let far_presence = stream_read_vec4(base, 64u);
        let far_color_value = far_visible_color(far_premul, far_valid, output[base + 16u], center_far.color);
        var far_background = vec4<f32>(0.0);
        for (var channel = 0u; channel < 4u; channel++) {
            if component(far_presence, channel) > 0.0 { set_component(&far_background, channel, component(far_color_value, channel)); }
            else if center_near.valid > 0.0 && center_far.valid > 0.0 { set_component(&far_background, channel, component(center_far.color, channel)); }
        }
        let center_blur = center_blur_weight(center_near, center_far);
        let background = source_value * (1.0 - center_blur) + far_background * center_blur;
        let under = local_result + background * (1.0 - clamp(local_result.w, 0.0, 1.0));
        let result = preserve_source_alpha_coverage(
            vec4<f32>(
                front_result.x + under.x * (1.0 - front_result.w),
                front_result.y + under.y * (1.0 - front_result.w),
                front_result.z + under.z * (1.0 - front_result.w),
                front_result.w + under.w * (1.0 - front_result.w),
            ),
            source_value,
        );
        stream_write_vec4(base, 68u, result);
    }
}

// This independent entry point compacts final pixels without changing the gather layout.
@group(0) @binding(18) var<storage, read_write> compact: array<f32>;

@compute @workgroup_size(64, 1, 1)
fn adaptive_stream_compact(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel = gid.x;
    if pixel >= arrayLength(&compact) / 4u { return; }
    let source = pixel * STREAM_STRIDE + 68u;
    let destination = pixel * 4u;
    compact[destination] = output[source];
    compact[destination + 1u] = output[source + 1u];
    compact[destination + 2u] = output[source + 2u];
    compact[destination + 3u] = output[source + 3u];
}
