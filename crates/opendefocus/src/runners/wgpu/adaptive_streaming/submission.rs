//! Records and submits the compute phases of a streamed gather for one source chunk.
use crate::{
    error::{Error, Result},
    runners::{adaptive_streaming::AdaptiveSourceChunk, wgpu::WgpuRunner},
};

use super::{StreamDestination, StreamFrameCache};

impl WgpuRunner {
    /// Records the compute phase of one source chunk into the destination accumulator, then submits it. Cached resources are reused when the cache retains them.
    pub(super) fn submit_stream_chunk_phase(
        &self,
        cache: &mut StreamFrameCache,
        chunk_index: usize,
        chunk: &AdaptiveSourceChunk,
        destination: &StreamDestination<'_>,
        phase: u32,
    ) -> Result<()> {
        if cache.selected.get(chunk_index).copied().unwrap_or(false) {
            if cache.resources[chunk_index].is_none() {
                cache.resources[chunk_index] = Some(self.make_stream_source_resources(chunk)?);
                self.wait_for_gpu()?;
            }
            let source = cache.resources[chunk_index]
                .as_ref()
                .ok_or(Error::InvalidDimensions)?;
            let bindings = self.make_stream_chunk_bindings(
                source,
                chunk,
                destination,
                phase,
                cache.source_opaque,
            )?;
            self.submit_stream_phase(
                destination.pipeline,
                &bindings.bind_group,
                destination.dispatch,
            )?;
        } else {
            // This chunk did not fit the frame cache. Build, use, then release
            // its GPU resources before the next chunk, retaining bounded peak residency.
            let source = self.make_stream_source_resources(chunk)?;
            let bindings = self.make_stream_chunk_bindings(
                &source,
                chunk,
                destination,
                phase,
                cache.source_opaque,
            )?;
            self.submit_stream_phase(
                destination.pipeline,
                &bindings.bind_group,
                destination.dispatch,
            )?;
        }
        Ok(())
    }

    /// Records one compute dispatch with the given bind group, submits it immediately and waits for it.
    ///
    /// # Errors
    /// Returns an error when waiting for the GPU fails.
    fn submit_stream_phase(
        &self,
        pipeline: &wgpu::ComputePipeline,
        bind_group: &wgpu::BindGroup,
        dispatch: [u32; 2],
    ) -> Result<()> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Adaptive streamed Accurate source contribution"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Adaptive streamed Accurate source contribution"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(dispatch[0], dispatch[1], 1);
        }
        self.queue.submit([encoder.finish()]);
        self.wait_for_gpu()
    }
}
