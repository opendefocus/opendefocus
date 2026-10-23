// Near/far mip reduction and padded texture packing share the buffer contract.
struct PyramidSettings { frame: vec4<u32>, }
@group(0) @binding(0) var<uniform> settings: PyramidSettings;
@group(0) @binding(1) var<storage, read> source: array<f32>;
@group(0) @binding(2) var<storage, read_write> target_pixels: array<f32>;
@group(0) @binding(3) var<storage, read_write> far_color: array<f32>;
@group(0) @binding(4) var<storage, read_write> near_metadata: array<f32>;
@group(0) @binding(5) var<storage, read_write> far_metadata: array<f32>;
@group(0) @binding(6) var<storage, read> edge_distances: array<f32>;

@compute @workgroup_size(8, 8)
fn reduce_layers(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= settings.frame.z || gid.y >= settings.frame.w { return; }
    let start = gid.xy * 2u;
    // The last cell includes the odd row/column, preserving all source energy.
    let end = vec2<u32>(
        select(min(start.x + 2u, settings.frame.x), settings.frame.x, gid.x + 1u == settings.frame.z),
        select(min(start.y + 2u, settings.frame.y), settings.frame.y, gid.y + 1u == settings.frame.w),
    );
    var color: array<f32, 8>;
    var near_coc = 0.0;
    var far_coc = 0.0;
    var near_valid = 0.0;
    var far_valid = 0.0;
    var children = 0.0;
    for (var y = start.y; y < end.y; y++) {
        for (var x = start.x; x < end.x; x++) {
            let base = (y * settings.frame.x + x) * 12u;
            let nv = clamp(source[base + 10u], 0.0, 1.0);
            let fv = clamp(source[base + 11u], 0.0, 1.0);
            for (var channel = 0u; channel < 8u; channel++) {
                color[channel] += source[base + channel];
            }
            near_coc += source[base + 8u] * nv;
            far_coc += source[base + 9u] * fv;
            near_valid += nv;
            far_valid += fv;
            children += 1.0;
        }
    }
    let base = (gid.y * settings.frame.z + gid.x) * 12u;
    let divisor = max(children, 1.0);
    for (var channel = 0u; channel < 8u; channel++) {
        target_pixels[base + channel] = color[channel] / divisor;
    }
    target_pixels[base + 8u] = 0.0;
    target_pixels[base + 9u] = 0.0;
    if near_valid > 0.0 { target_pixels[base + 8u] = near_coc / near_valid; }
    if far_valid > 0.0 { target_pixels[base + 9u] = far_coc / far_valid; }
    target_pixels[base + 10u] = near_valid / divisor;
    target_pixels[base + 11u] = far_valid / divisor;
}

@compute @workgroup_size(8, 8)
fn pack_layers(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= settings.frame.x || gid.y >= settings.frame.y { return; }
    let index = gid.y * settings.frame.x + gid.x;
    let base = index * 12u;
    let packed = (gid.y * settings.frame.z + gid.x) * 4u;
    for (var channel = 0u; channel < 4u; channel++) {
        target_pixels[packed + channel] = source[base + channel];
        far_color[packed + channel] = source[base + 4u + channel];
    }
    let nv = source[base + 10u];
    let fv = source[base + 11u];
    let edge = edge_distances[index];
    near_metadata[packed] = source[base + 8u] * nv;
    near_metadata[packed + 1u] = nv;
    near_metadata[packed + 2u] = max(edge, 0.0);
    near_metadata[packed + 3u] = select(0.0, 1.0, edge >= 0.0);
    far_metadata[packed] = source[base + 9u] * fv;
    far_metadata[packed + 1u] = fv;
    far_metadata[packed + 2u] = 0.0;
    far_metadata[packed + 3u] = 0.0;
}
