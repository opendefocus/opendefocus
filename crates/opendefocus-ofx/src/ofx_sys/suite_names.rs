//! Names of the OFX suites the plugin fetches from the host.

use core::ffi::CStr;

/// kOfxPropertySuite (ofxProperty.h:18)
pub const OFX_PROPERTY_SUITE: &CStr = c"OfxPropertySuite";
/// kOfxImageEffectSuite (ofxImageEffect.h:1774)
pub const OFX_IMAGE_EFFECT_SUITE: &CStr = c"OfxImageEffectSuite";
/// kOfxParameterSuite (ofxParam.h:15)
pub const OFX_PARAMETER_SUITE: &CStr = c"OfxParameterSuite";
/// kOfxMemorySuite (ofxMemory.h:12)
pub const OFX_MEMORY_SUITE: &CStr = c"OfxMemorySuite";
/// kOfxMultiThreadSuite (ofxMultiThread.h:19)
pub const OFX_MULTI_THREAD_SUITE: &CStr = c"OfxMultiThreadSuite";
/// kOfxMessageSuite (ofxMessage.h:14)
pub const OFX_MESSAGE_SUITE: &CStr = c"OfxMessageSuite";
