struct Params {
    width: u32,
    height: u32,
    jump_step: u32,
    foreground_positive: u32,
    max_radius: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> source: array<f32>;
@group(0) @binding(2) var<storage, read> coc: array<f32>;
@group(0) @binding(3) var<storage, read> seeds_in: array<i32>;
@group(0) @binding(4) var<storage, read_write> seeds_out: array<i32>;
@group(0) @binding(5) var<storage, read_write> prepared: array<f32>;

fn pixel_index(x: u32, y: u32) -> u32 { return y * params.width + x; }
fn foreground(value: f32) -> bool {
    // Treat the focal plane as an occluding near surface. Its zero CoC must
    // stay sharp at the output, while keeping it out of the blurred far layer
    // prevents that layer's mip chain from bleeding across focused silhouettes.
    if (params.foreground_positive != 0u) { return value >= 0.0; }
    return value <= 0.0;
}

@compute @workgroup_size(8, 8, 1)
fn initialize(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) { return; }
    let pixel = pixel_index(gid.x, gid.y);
    let i = pixel * 4u;
    var near_edge = false;
    var near_defocus_edge = false;
    for (var oy = -1; oy <= 1; oy += 1) {
        for (var ox = -1; ox <= 1; ox += 1) {
            if (ox == 0 && oy == 0) { continue; }
            let nx = i32(gid.x) + ox;
            let ny = i32(gid.y) + oy;
            if (nx < 0 || ny < 0 || nx >= i32(params.width) || ny >= i32(params.height)) { continue; }
            let neighbor_coc = coc[u32(ny) * params.width + u32(nx)];
            if (foreground(neighbor_coc)) { near_edge = true; }
            if (neighbor_coc > 0.5) { near_defocus_edge = true; }
        }
    }
    // Edge pixels can contain foreground matte color even when the depth
    // already classifies them as far. Seed the far plate from one pixel into
    // the background so this color is not spread into the defocused halo.
    let pixel_coc = coc[pixel];
    if (!foreground(pixel_coc) && !near_edge) {
        seeds_out[i] = i32(gid.x);
        seeds_out[i + 1u] = i32(gid.y);
    } else {
        seeds_out[i] = -1;
        seeds_out[i + 1u] = -1;
    }
    // A second nearest-seed field carries exact-focus color for local opaque
    // background continuation beneath defocused near layers. Keep its seeds
    // one pixel inside the focus region at positive-defocus boundaries.
    if (pixel_coc == 0.0 && !near_defocus_edge) {
        seeds_out[i + 2u] = i32(gid.x);
        seeds_out[i + 3u] = i32(gid.y);
    } else {
        seeds_out[i + 2u] = -1;
        seeds_out[i + 3u] = -1;
    }
}

@compute @workgroup_size(8, 8, 1)
fn jump_flood(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) { return; }
    let out_i = pixel_index(gid.x, gid.y) * 4u;
    var best_x = seeds_in[out_i];
    var best_y = seeds_in[out_i + 1u];
    var best_dist2 = 3.402823e+38;
    if (best_x >= 0) {
        let dx = f32(best_x - i32(gid.x));
        let dy = f32(best_y - i32(gid.y));
        best_dist2 = dx * dx + dy * dy;
    }
    var focus_best_x = seeds_in[out_i + 2u];
    var focus_best_y = seeds_in[out_i + 3u];
    var focus_best_dist2 = 3.402823e+38;
    if (focus_best_x >= 0) {
        let dx = f32(focus_best_x - i32(gid.x));
        let dy = f32(focus_best_y - i32(gid.y));
        focus_best_dist2 = dx * dx + dy * dy;
    }
    let step = i32(params.jump_step);
    for (var oy = -1; oy <= 1; oy += 1) {
        for (var ox = -1; ox <= 1; ox += 1) {
            if (ox == 0 && oy == 0) { continue; }
            let sx = i32(gid.x) + ox * step;
            let sy = i32(gid.y) + oy * step;
            if (sx < 0 || sy < 0 || sx >= i32(params.width) || sy >= i32(params.height)) { continue; }
            let source_i = (u32(sy) * params.width + u32(sx)) * 4u;
            let candidate_x = seeds_in[source_i];
            let candidate_y = seeds_in[source_i + 1u];
            if (candidate_x >= 0) {
                let dx = f32(candidate_x - i32(gid.x));
                let dy = f32(candidate_y - i32(gid.y));
                let dist2 = dx * dx + dy * dy;
                if (dist2 < best_dist2) {
                    best_dist2 = dist2;
                    best_x = candidate_x;
                    best_y = candidate_y;
                }
            }
            let focus_candidate_x = seeds_in[source_i + 2u];
            let focus_candidate_y = seeds_in[source_i + 3u];
            if (focus_candidate_x >= 0) {
                let dx = f32(focus_candidate_x - i32(gid.x));
                let dy = f32(focus_candidate_y - i32(gid.y));
                let dist2 = dx * dx + dy * dy;
                if (dist2 < focus_best_dist2) {
                    focus_best_dist2 = dist2;
                    focus_best_x = focus_candidate_x;
                    focus_best_y = focus_candidate_y;
                }
            }
        }
    }
    seeds_out[out_i] = best_x;
    seeds_out[out_i + 1u] = best_y;
    seeds_out[out_i + 2u] = focus_best_x;
    seeds_out[out_i + 3u] = focus_best_y;
}

@compute @workgroup_size(8, 8, 1)
fn compose_layers(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) { return; }
    let pixel = pixel_index(gid.x, gid.y);
    let source_i = pixel * 4u;
    let output_i = pixel * 13u;
    let is_foreground = foreground(coc[pixel]);
    prepared[output_i + 12u] = -1.0;
    let seed_i = pixel * 4u;
    let sx = seeds_in[seed_i];
    let sy = seeds_in[seed_i + 1u];
    let focus_sx = seeds_in[seed_i + 2u];
    let focus_sy = seeds_in[seed_i + 3u];
    var near_edge = false;
    if (is_foreground && sx >= 0 && sy >= 0) {
        let dx = f32(sx - i32(gid.x));
        let dy = f32(sy - i32(gid.y));
        // Far seeds are kept one pixel away from the depth boundary. Reuse
        // the jump-flood result to identify the first foreground edge pixels
        // without another per-pixel neighborhood scan.
        near_edge = dx * dx + dy * dy <= 8.0;
    }
    var near_coverage = 1.0;
    var near_color = vec4<f32>(
        source[source_i],
        source[source_i + 1u],
        source[source_i + 2u],
        source[source_i + 3u],
    );
    // A focus crossing is a continuous depth ramp, so its color differences
    // must not be interpreted as a partially covered object silhouette.
    var focus_crossing = false;
    if (near_edge && abs(coc[pixel]) <= 0.25) {
        if (gid.x > 0u && gid.x + 1u < params.width) {
            focus_crossing = coc[pixel - 1u] * coc[pixel + 1u] < 0.0;
        }
        if (gid.y > 0u && gid.y + 1u < params.height) {
            focus_crossing = focus_crossing || coc[pixel - params.width] * coc[pixel + params.width] < 0.0;
        }
    }
    if (near_edge && !focus_crossing) {
        // The nearest clean far seed points away from the silhouette.
        // Sample one pixel inward for a constant-cost foreground estimate.
        if (sx >= 0 && sy >= 0) {
            let far_pixel = u32(sy) * params.width + u32(sx);
            let far_source_i = far_pixel * 4u;
            let far_color = vec4<f32>(
                source[far_source_i],
                source[far_source_i + 1u],
                source[far_source_i + 2u],
                source[far_source_i + 3u],
            );
            let dx = sx - i32(gid.x);
            let dy = sy - i32(gid.y);
            // Move exactly one pixel toward the foreground. Stepping on
            // both axes for a diagonal seed moved sqrt(2) pixels and could
            // pull a visibly different color across a curved silhouette.
            let horizontal = abs(dx) >= abs(dy);
            let step_x = select(0, select(-1, 1, dx > 0), horizontal);
            let step_y = select(select(-1, 1, dy > 0), 0, horizontal);
            let inner_x = clamp(i32(gid.x) - step_x, 0, i32(params.width) - 1);
            let inner_y = clamp(i32(gid.y) - step_y, 0, i32(params.height) - 1);
            let inner_pixel = u32(inner_y) * params.width + u32(inner_x);
            let edge_coc = coc[pixel];
            let inner_coc = coc[inner_pixel];
            // RGB alone cannot distinguish antialiasing from a legitimate
            // foreground color gradient. Only estimate matte coverage when
            // the edge also has an independent, sharp CoC discontinuity from
            // the interior foreground sample.
            let coc_step = abs(edge_coc - inner_coc);
            let coc_step_threshold = max(1.0, 0.25 * abs(inner_coc));
            if (foreground(inner_coc) && coc_step > coc_step_threshold) {
                let inner_source_i = inner_pixel * 4u;
                let inner_color = vec4<f32>(
                    source[inner_source_i],
                    source[inner_source_i + 1u],
                    source[inner_source_i + 2u],
                    source[inner_source_i + 3u],
                );
                let foreground_delta = inner_color.xyz - far_color.xyz;
                let foreground_length_squared = dot(foreground_delta, foreground_delta);
                if (foreground_length_squared > 1e-6) {
                    let source_delta = vec3<f32>(
                        source[source_i],
                        source[source_i + 1u],
                        source[source_i + 2u],
                    ) - far_color.xyz;
                    let estimated_coverage = clamp(
                        dot(source_delta, foreground_delta) / foreground_length_squared,
                        0.0,
                        1.0,
                    );
                    let residual = source_delta - foreground_delta * estimated_coverage;
                    // Fractional coverage requires a measured color mixture.
                    // An uncertain estimate or a focus crossing stays opaque.
                    if (estimated_coverage < 0.999 && dot(residual, residual) <= foreground_length_squared * 0.02) {
                        near_coverage = estimated_coverage;
                        near_color = inner_color;
                    }
                }
            }
        }
    }
    for (var c = 0u; c < 4u; c += 1u) {
        prepared[output_i + c] = select(0.0, near_color[c] * near_coverage, is_foreground);
        prepared[output_i + 4u + c] = 0.0;
    }
    prepared[output_i + 8u] = select(0.0, coc[pixel], is_foreground);
    prepared[output_i + 9u] = 0.0;
    // Validity records geometric layer support; alpha remains opacity in the
    // premultiplied color and must not shrink the filter's normalization area.
    prepared[output_i + 10u] = select(0.0, near_coverage, is_foreground);
    prepared[output_i + 11u] = 0.0;

    if (!is_foreground) {
        var far_source_i = source_i;
        if (sx >= 0 && sy >= 0) {
            let dx = f32(sx - i32(gid.x));
            let dy = f32(sy - i32(gid.y));
            if (dx * dx + dy * dy <= 4.0) {
                far_source_i = (u32(sy) * params.width + u32(sx)) * 4u;
            }
        }
        for (var c = 0u; c < 4u; c += 1u) { prepared[output_i + 4u + c] = source[far_source_i + c]; }
        prepared[output_i + 9u] = coc[pixel];
        prepared[output_i + 11u] = 1.0;
        return;
    }
    // The defocused far plate continues behind focused occluders as before.
    // Exact-focus color is used only beneath a positive defocused near layer;
    // copying it into focused pixels would spread that silhouette through mips.
    var continuation_x = sx;
    var continuation_y = sy;
    var continuation_is_focused = false;
    // Focus seeds exclude a one-pixel edge neighborhood; allow that inset
    // and the half-pixel aperture tent when deciding whether to fill the plate.
    let focus_reach = params.max_radius + 2.5;
    if (coc[pixel] > 0.5 && focus_sx >= 0 && focus_sy >= 0) {
        let focus_dx = f32(focus_sx - i32(gid.x));
        let focus_dy = f32(focus_sy - i32(gid.y));
        if (focus_dx * focus_dx + focus_dy * focus_dy <= focus_reach * focus_reach) {
            continuation_x = focus_sx;
            continuation_y = focus_sy;
            continuation_is_focused = true;
        }
    }
    if (continuation_x < 0 || continuation_y < 0) { return; }
    let dx = f32(continuation_x - i32(gid.x));
    let dy = f32(continuation_y - i32(gid.y));
    let distance_to_silhouette = max(sqrt(dx * dx + dy * dy) - 1.0, 0.0);
    let continuation_reach = select(params.max_radius, focus_reach, continuation_is_focused);
    if (dx * dx + dy * dy > continuation_reach * continuation_reach) { return; }
    let far_pixel = u32(continuation_y) * params.width + u32(continuation_x);
    let far_i = far_pixel * 4u;
    for (var c = 0u; c < 4u; c += 1u) { prepared[output_i + 4u + c] = source[far_i + c]; }
    prepared[output_i + 9u] = select(coc[far_pixel], 0.0, continuation_is_focused);
    prepared[output_i + 12u] = distance_to_silhouette;
    prepared[output_i + 11u] = 1.0;
}
