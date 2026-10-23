const TILE_SIZE: i32 = 16;
const COC_SHARP_LIMIT: f32 = 0.5;
// A tile-wide CoC range also grows on a perfectly smooth focus ramp. Only a
// discontinuity between adjacent source pixels is a depth-edge reason.
const COC_EDGE_STEP_LIMIT: f32 = 1.0;
const ALPHA_VARIATION_LIMIT: f32 = 0.05;
// Fine texture should be integrated by a broad blur. Reserve detail for the
// moderate-radius interval where it can still alias a bounded gather.
const TEXTURE_CONTRAST_LIMIT: f32 = 0.5;
const TEXTURE_DETAIL_MIN_COC: f32 = 0.5;
const TEXTURE_DETAIL_MAX_COC: f32 = 4.0;
const HDR_EMITTER_LIMIT: f32 = 4.0;

struct Params {
    width: u32,
    height: u32,
    tiles_x: u32,
    tiles_y: u32,
    origin_x: i32,
    origin_y: i32,
    max_search_tiles: u32,
    padding: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> color: array<f32>;
@group(0) @binding(2) var<storage, read> coc: array<f32>;
@group(0) @binding(3) var<storage, read_write> features: array<f32>;

fn luma_at(x: i32, y: i32) -> f32 {
    let cx = u32(clamp(x, 0, i32(params.width) - 1));
    let cy = u32(clamp(y, 0, i32(params.height) - 1));
    let i = (cy * params.width + cx) * 4u;
    return dot(vec3<f32>(color[i], color[i + 1u], color[i + 2u]), vec3<f32>(0.2126, 0.7152, 0.0722));
}

@compute @workgroup_size(1, 1, 1)
fn classify(@builtin(workgroup_id) tile: vec3<u32>) {
    if (tile.x >= params.tiles_x || tile.y >= params.tiles_y) { return; }
    let base_x = (params.origin_x / TILE_SIZE) * TILE_SIZE + i32(tile.x) * TILE_SIZE;
    let base_y = (params.origin_y / TILE_SIZE) * TILE_SIZE + i32(tile.y) * TILE_SIZE;
    var min_coc = 3.402823e+38;
    var max_coc = -3.402823e+38;
    var min_alpha = 3.402823e+38;
    var max_alpha = -3.402823e+38;
    var min_luma = 3.402823e+38;
    var max_luma = -3.402823e+38;
    var max_texture = 0.0;
    var max_coc_step = 0.0;
    var peak_luma = 0.0;
    var count = 0u;

    for (var y = 0; y < TILE_SIZE; y += 1) {
        for (var x = 0; x < TILE_SIZE; x += 1) {
            let gx = base_x + x;
            let gy = base_y + y;
            let lx = gx - params.origin_x;
            let ly = gy - params.origin_y;
            if (lx < 0 || ly < 0 || lx >= i32(params.width) || ly >= i32(params.height)) { continue; }
            let pixel = (u32(ly) * params.width + u32(lx)) * 4u;
            let d = coc[u32(ly) * params.width + u32(lx)];
            let lum = dot(vec3<f32>(color[pixel], color[pixel + 1u], color[pixel + 2u]), vec3<f32>(0.2126, 0.7152, 0.0722));
            let alpha = color[pixel + 3u];
            min_coc = min(min_coc, d);
            max_coc = max(max_coc, d);
            min_alpha = min(min_alpha, alpha);
            max_alpha = max(max_alpha, alpha);
            min_luma = min(min_luma, lum);
            max_luma = max(max_luma, lum);
            peak_luma = max(peak_luma, lum);
            max_texture = max(max_texture, abs(luma_at(lx - 1, ly) - lum));
            max_texture = max(max_texture, abs(luma_at(lx + 1, ly) - lum));
            max_texture = max(max_texture, abs(luma_at(lx, ly - 1) - lum));
            max_texture = max(max_texture, abs(luma_at(lx, ly + 1) - lum));
            // Record each horizontal/vertical edge once. The neighboring-tile
            // propagation pass carries a boundary reason across tile edges.
            max_coc_step = max(max_coc_step, abs(coc[u32(ly) * params.width + u32(clamp(lx + 1, 0, i32(params.width) - 1))] - d));
            max_coc_step = max(max_coc_step, abs(coc[u32(clamp(ly + 1, 0, i32(params.height) - 1)) * params.width + u32(lx)] - d));
            count += 1u;
        }
    }

    if (count == 0u) { return; }
    var reasons = 0u;
    if (max_coc_step > COC_EDGE_STEP_LIMIT) { reasons |= 1u; }
    if (max_alpha - min_alpha > ALPHA_VARIATION_LIMIT) { reasons |= 2u; }
    if (peak_luma > HDR_EMITTER_LIMIT && peak_luma - min_luma > HDR_EMITTER_LIMIT) { reasons |= 4u; }
    let max_radius = max(abs(min_coc), abs(max_coc));
    if (max_texture > TEXTURE_CONTRAST_LIMIT && max_radius > TEXTURE_DETAIL_MIN_COC && max_radius <= TEXTURE_DETAIL_MAX_COC) { reasons |= 8u; }

    // Candidate classes are finalized by the blur-reach pass before they are used for routing.
    var tile_class = 0.0;
    if (max(abs(min_coc), abs(max_coc)) > COC_SHARP_LIMIT) {
        tile_class = 1.0;
        if ((reasons & 14u) != 0u) { tile_class = 2.0; }
    }
    let out_index = (tile.y * params.tiles_x + tile.x) * 11u;
    features[out_index] = min_coc;
    features[out_index + 1u] = max_coc;
    features[out_index + 2u] = min_alpha;
    features[out_index + 3u] = max_alpha;
    features[out_index + 4u] = min_luma;
    features[out_index + 5u] = max_luma;
    features[out_index + 6u] = max_texture;
    features[out_index + 7u] = peak_luma;
    features[out_index + 8u] = f32(reasons);
    features[out_index + 9u] = tile_class;
    features[out_index + 10u] = max_coc_step;
}

@group(1) @binding(0) var<uniform> propagation_params: Params;
@group(1) @binding(1) var<storage, read> raw_features: array<f32>;
@group(1) @binding(2) var<storage, read_write> final_features: array<f32>;

@compute @workgroup_size(1, 1, 1)
fn propagate(@builtin(workgroup_id) tile: vec3<u32>) {
    if (tile.x >= propagation_params.tiles_x || tile.y >= propagation_params.tiles_y) { return; }
    let index = (tile.y * propagation_params.tiles_x + tile.x) * 11u;
    let min_coc = raw_features[index];
    let max_coc = raw_features[index + 1u];
    let reasons = u32(raw_features[index + 8u]);
    let own_reach = max(abs(min_coc), abs(max_coc));
    var tile_class = 0.0;
    if (own_reach > COC_SHARP_LIMIT) {
        tile_class = 1.0;
        if (reasons != 0u) { tile_class = 2.0; }
    }
    let search = i32(propagation_params.max_search_tiles);
    for (var oy = -search; oy <= search; oy += 1) {
        for (var ox = -search; ox <= search; ox += 1) {
            let sx = i32(tile.x) + ox;
            let sy = i32(tile.y) + oy;
            if ((ox == 0 && oy == 0) || sx < 0 || sy < 0 || sx >= i32(propagation_params.tiles_x) || sy >= i32(propagation_params.tiles_y)) { continue; }
            let source_index = (u32(sy) * propagation_params.tiles_x + u32(sx)) * 11u;
            let source_reach = max(abs(raw_features[source_index]), abs(raw_features[source_index + 1u]));
            let gap_x = max(0.0, f32(abs(ox) - 1) * f32(TILE_SIZE));
            let gap_y = max(0.0, f32(abs(oy) - 1) * f32(TILE_SIZE));
            if (source_reach > COC_SHARP_LIMIT && gap_x <= source_reach && gap_y <= source_reach) {
                tile_class = max(tile_class, select(1.0, 2.0, u32(raw_features[source_index + 8u]) != 0u));
            }
        }
    }
    for (var i = 0u; i < 9u; i += 1u) { final_features[index + i] = raw_features[index + i]; }
    final_features[index + 9u] = tile_class;
    final_features[index + 10u] = raw_features[index + 10u];
}
