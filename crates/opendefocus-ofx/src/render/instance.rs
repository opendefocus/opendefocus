//! Per-effect render state: the instance object that owns the WGPU renderer, and the create and
//! destroy actions that allocate and free it through the instance-data property.

use super::{
    Failure, HostResult, HostSuites, OFX_PROP_INSTANCE_DATA, OfxImageEffectHandle,
    OpenDefocusRenderer, Outcome, Settings, c_void, effect_props, fail, stored_suites,
};

/// Per-instance state, reached through `kOfxPropInstanceData` on the effect property set.
#[derive(Default)]
pub struct Instance {
    /// Created on the first render. Its device follows the first render's settings.
    renderer: Option<OpenDefocusRenderer>,
}

impl Instance {
    /// Returns the renderer, creating it from `settings` on first use. Its device is chosen then.
    pub(super) fn renderer(&mut self, settings: &Settings) -> Outcome<&OpenDefocusRenderer> {
        if self.renderer.is_none() {
            let mut init = settings.clone();
            let renderer =
                pollster::block_on(OpenDefocusRenderer::new(&mut init)).map_err(|error| {
                    Failure::Message(format!(
                        "OpenDefocus renderer initialization failed: {error}"
                    ))
                })?;
            self.renderer = Some(renderer);
        }
        match self.renderer.as_ref() {
            Some(renderer) => Ok(renderer),
            None => fail("OpenDefocus renderer is not initialized"),
        }
    }
}
/// Returns the instance pointer stored on the effect, or fails if none was created.
pub(super) fn instance_mut<'a>(
    suites: &HostSuites,
    effect: OfxImageEffectHandle,
) -> Outcome<&'a mut Instance> {
    let props = effect_props(suites, effect)?;
    let data = props.get_pointer(OFX_PROP_INSTANCE_DATA, 0)?;
    if data.is_null() {
        return fail("effect has no instance data");
    }
    // SAFETY: the pointer was produced by `create_instance` as a `Box<Instance>` and is freed
    // only by `destroy_instance`. OFX runs at most one render per instance at a time
    // (kOfxImageEffectRenderInstanceSafe), so this is the only live reference.
    Ok(unsafe { &mut *data.cast::<Instance>() })
}

/// Allocates the instance and stores it on the effect (`CreateInstance`).
///
/// # Errors
///
/// Returns an error if Load did not store the suites or the host rejects the instance pointer;
/// the allocated instance is freed in that case.
pub fn create_instance(effect: OfxImageEffectHandle) -> HostResult<()> {
    let suites = stored_suites()?;
    let props = effect_props(&suites, effect)?;
    let instance = Box::into_raw(Box::new(Instance::default()));
    if let Err(error) = props.set_pointer(OFX_PROP_INSTANCE_DATA, 0, instance.cast::<c_void>()) {
        // SAFETY: the pointer was just created and the host never saw it; reclaim it.
        drop(unsafe { Box::from_raw(instance) });
        return Err(error);
    }
    Ok(())
}

/// Frees the instance stored on the effect (`DestroyInstance`). A null pointer is a no-op.
///
/// # Errors
///
/// Returns an error if Load did not store the suites or the host rejects the property read.
pub fn destroy_instance(effect: OfxImageEffectHandle) -> HostResult<()> {
    let suites = stored_suites()?;
    let props = effect_props(&suites, effect)?;
    let data = props.get_pointer(OFX_PROP_INSTANCE_DATA, 0)?;
    if data.is_null() {
        return Ok(());
    }
    props.set_pointer(OFX_PROP_INSTANCE_DATA, 0, core::ptr::null_mut())?;
    // SAFETY: the pointer came from `create_instance`, is cleared on the effect above so it
    // cannot be used again, and is freed exactly once.
    drop(unsafe { Box::from_raw(data.cast::<Instance>()) });
    Ok(())
}
