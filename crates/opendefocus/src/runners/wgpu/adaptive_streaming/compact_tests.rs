//! Tests for streamed opacity and the output compaction pass.
use super::*;
use crate::datamodel::IVector4;
use wgpu::util::DeviceExt;

#[test]
/// Verifies streamed opacity requires every source core to be opaque.
fn streamed_opacity_requires_every_source_core_to_be_opaque() {
    use crate::runners::{adaptive::PreparedLayers, adaptive_streaming::AdaptiveSourceRegion};

    let chunk = |alpha| AdaptiveSourceChunk {
        region: AdaptiveSourceRegion {
            core: IVector4 {
                x: 0,
                y: 0,
                z: 1,
                w: 1,
            },
            context: IVector4 {
                x: 0,
                y: 0,
                z: 1,
                w: 1,
            },
        },
        source: Array3::from_shape_vec((1, 1, 4), vec![0.0, 0.0, 0.0, alpha]).unwrap(),
        prepared: PreparedLayers {
            width: 1,
            height: 1,
            pixels: Vec::new(),
            near_edge_distance: Vec::new(),
        },
        near_integral_nodes: Vec::new(),
        far_integral_nodes: Vec::new(),
        subtree_ends: Vec::new(),
    };
    let chunks = [chunk(1.0), chunk(0.0)];
    assert!(stream_sources_are_opaque(&chunks[..1]));
    assert!(!stream_sources_are_opaque(&chunks));
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that compaction preserves the odd tail pixels.
async fn hardware_stream_compaction_preserves_odd_tail_pixels() {
    let runner = WgpuRunner::new().await.expect("hardware GPU required");
    let pixels = 65usize;
    let source: Vec<f32> = (0..pixels * 72).map(|index| index as f32 + 0.25).collect();
    let accumulator = runner
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Synthetic streamed accumulators"),
            contents: bytemuck::cast_slice(&source),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let output_bytes = (pixels * 4 * std::mem::size_of::<f32>()) as u64;
    let compact = runner.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Synthetic compact output"),
        size: output_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let staging = runner.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Synthetic compact readback"),
        size: output_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = runner
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Synthetic compact test encoder"),
        });
    runner
        .encode_stream_compaction(&mut encoder, &accumulator, &compact, pixels as u64)
        .unwrap();
    encoder.copy_buffer_to_buffer(&compact, 0, &staging, 0, output_bytes);
    let (sender, receiver) = futures_channel::oneshot::channel();
    encoder.map_buffer_on_submit(&staging, wgpu::MapMode::Read, .., |result| {
        crate::runners::wgpu::complete_readback(sender, result);
    });
    runner.queue.submit([encoder.finish()]);
    runner.wait_for_gpu().unwrap();
    receiver.await.unwrap().unwrap();
    let view = staging.slice(..).get_mapped_range().unwrap();
    let compacted: &[f32] = bytemuck::cast_slice(&view);
    for pixel in 0..pixels {
        for channel in 0..4 {
            assert_eq!(
                compacted[pixel * 4 + channel],
                source[pixel * 72 + 68 + channel]
            );
        }
    }
    drop(view);
    staging.unmap();
}
