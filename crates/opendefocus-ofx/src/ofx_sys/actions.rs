//! Plugin API identifier and the OFX action names the host sends to the plugin.

use core::ffi::CStr;
use core::ffi::c_int;

/// kOfxImageEffectPluginApi (ofxImageEffect.h:26)
pub const OFX_IMAGE_EFFECT_PLUGIN_API: &CStr = c"OfxImageEffectPluginAPI";
/// kOfxImageEffectPluginApiVersion (ofxImageEffect.h:30)
pub const OFX_IMAGE_EFFECT_PLUGIN_API_VERSION: c_int = 1;

/// kOfxActionLoad (ofxCore.h:193)
pub const OFX_ACTION_LOAD: &CStr = c"OfxActionLoad";
/// kOfxActionUnload (ofxCore.h:275)
pub const OFX_ACTION_UNLOAD: &CStr = c"OfxActionUnload";
/// kOfxActionDescribe (ofxCore.h:247)
pub const OFX_ACTION_DESCRIBE: &CStr = c"OfxActionDescribe";
/// kOfxActionCreateInstance (ofxCore.h:398)
pub const OFX_ACTION_CREATE_INSTANCE: &CStr = c"OfxActionCreateInstance";
/// kOfxActionDestroyInstance (ofxCore.h:438)
pub const OFX_ACTION_DESTROY_INSTANCE: &CStr = c"OfxActionDestroyInstance";
/// kOfxImageEffectActionDescribeInContext (ofxImageEffect.h:698)
pub const OFX_IMAGE_EFFECT_ACTION_DESCRIBE_IN_CONTEXT: &CStr =
    c"OfxImageEffectActionDescribeInContext";
/// kOfxImageEffectActionRender (ofxImageEffect.h:521)
pub const OFX_IMAGE_EFFECT_ACTION_RENDER: &CStr = c"OfxImageEffectActionRender";
/// kOfxImageEffectActionGetRegionsOfInterest (ofxImageEffect.h:204)
pub const OFX_IMAGE_EFFECT_ACTION_GET_REGIONS_OF_INTEREST: &CStr =
    c"OfxImageEffectActionGetRegionsOfInterest";
/// kOfxImageEffectActionIsIdentity (ofxImageEffect.h:448)
pub const OFX_IMAGE_EFFECT_ACTION_IS_IDENTITY: &CStr = c"OfxImageEffectActionIsIdentity";
/// kOfxImageEffectActionGetClipPreferences (ofxImageEffect.h:387)
pub const OFX_IMAGE_EFFECT_ACTION_GET_CLIP_PREFERENCES: &CStr =
    c"OfxImageEffectActionGetClipPreferences";
/// kOfxActionInstanceChanged (ofxCore.h:512)
pub const OFX_ACTION_INSTANCE_CHANGED: &CStr = c"OfxActionInstanceChanged";
