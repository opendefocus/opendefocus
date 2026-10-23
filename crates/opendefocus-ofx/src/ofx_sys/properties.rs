//! Property names, parameter type names and message identifiers the plugin reads and writes.

use core::ffi::CStr;

/// kOfxPropLabel (ofxCore.h:806)
pub const OFX_PROP_LABEL: &CStr = c"OfxPropLabel";
/// kOfxPropType (ofxCore.h:748)
pub const OFX_PROP_TYPE: &CStr = c"OfxPropType";
/// kOfxTypeParameter (ofxParam.h:18)
pub const OFX_TYPE_PARAMETER: &CStr = c"OfxTypeParameter";
/// kOfxPropPluginDescription (ofxCore.h:794)
pub const OFX_PROP_PLUGIN_DESCRIPTION: &CStr = c"OfxPropPluginDescription";
/// kOfxImageEffectPluginPropGrouping (ofxImageEffect.h:955)
pub const OFX_IMAGE_EFFECT_PLUGIN_PROP_GROUPING: &CStr = c"OfxImageEffectPluginPropGrouping";
/// kOfxImageEffectPluginRenderThreadSafety (ofxImageEffect.h:783)
pub const OFX_IMAGE_EFFECT_PLUGIN_RENDER_THREAD_SAFETY: &CStr =
    c"OfxImageEffectPluginRenderThreadSafety";
/// kOfxImageEffectRenderInstanceSafe (ofxImageEffect.h:788)
pub const OFX_IMAGE_EFFECT_RENDER_INSTANCE_SAFE: &CStr = c"OfxImageEffectRenderInstanceSafe";
/// kOfxImageEffectPropSupportedContexts (ofxImageEffect.h:733)
pub const OFX_IMAGE_EFFECT_PROP_SUPPORTED_CONTEXTS: &CStr = c"OfxImageEffectPropSupportedContexts";
/// kOfxImageEffectContextFilter (ofxImageEffect.h:60)
pub const OFX_IMAGE_EFFECT_CONTEXT_FILTER: &CStr = c"OfxImageEffectContextFilter";
/// kOfxImageEffectContextGeneral (ofxImageEffect.h:69)
pub const OFX_IMAGE_EFFECT_CONTEXT_GENERAL: &CStr = c"OfxImageEffectContextGeneral";
/// kOfxImageEffectPropContext (ofxImageEffect.h:1070)
pub const OFX_IMAGE_EFFECT_PROP_CONTEXT: &CStr = c"OfxImageEffectPropContext";
/// kOfxImageEffectPropSupportedPixelDepths (ofxImageEffect.h:1233)
pub const OFX_IMAGE_EFFECT_PROP_SUPPORTED_PIXEL_DEPTHS: &CStr =
    c"OfxImageEffectPropSupportedPixelDepths";
/// kOfxBitDepthFloat (ofxCore.h:976)
pub const OFX_BIT_DEPTH_FLOAT: &CStr = c"OfxBitDepthFloat";
/// kOfxImageEffectPropSupportedComponents (ofxImageEffect.h:1255)
pub const OFX_IMAGE_EFFECT_PROP_SUPPORTED_COMPONENTS: &CStr =
    c"OfxImageEffectPropSupportedComponents";
/// kOfxImageComponentRGBA (ofxImageEffect.h:47)
pub const OFX_IMAGE_COMPONENT_RGBA: &CStr = c"OfxImageComponentRGBA";
/// kOfxImageComponentAlpha (ofxImageEffect.h:53)
pub const OFX_IMAGE_COMPONENT_ALPHA: &CStr = c"OfxImageComponentAlpha";
/// kOfxImageClipPropOptional (ofxImageEffect.h:1263)
pub const OFX_IMAGE_CLIP_PROP_OPTIONAL: &CStr = c"OfxImageClipPropOptional";
/// kOfxImageEffectPropSupportsTiles (ofxImageEffect.h:1033)
pub const OFX_IMAGE_EFFECT_PROP_SUPPORTS_TILES: &CStr = c"OfxImageEffectPropSupportsTiles";
/// kOfxImageEffectPropTemporalClipAccess (ofxImageEffect.h:1048)
pub const OFX_IMAGE_EFFECT_PROP_TEMPORAL_CLIP_ACCESS: &CStr =
    c"OfxImageEffectPropTemporalClipAccess";
/// kOfxImageEffectPluginPropFieldRenderTwiceAlways (ofxImageEffect.h:1611)
pub const OFX_IMAGE_EFFECT_PLUGIN_PROP_FIELD_RENDER_TWICE_ALWAYS: &CStr =
    c"OfxImageEffectPluginPropFieldRenderTwiceAlways";
/// kOfxImageEffectPropSupportsMultiResolution (ofxImageEffect.h:1018)
pub const OFX_IMAGE_EFFECT_PROP_SUPPORTS_MULTI_RESOLUTION: &CStr =
    c"OfxImageEffectPropSupportsMultiResolution";
/// kOfxImageEffectPluginPropHostFrameThreading (ofxImageEffect.h:801)
pub const OFX_IMAGE_EFFECT_PLUGIN_PROP_HOST_FRAME_THREADING: &CStr =
    c"OfxImageEffectPluginPropHostFrameThreading";
/// kOfxImageEffectPropSupportsMultipleClipPARs (ofxImageEffect.h:830)
pub const OFX_IMAGE_EFFECT_PROP_SUPPORTS_MULTIPLE_CLIP_PARS: &CStr =
    c"OfxImageEffectPropSupportsMultipleClipPARs";
/// kOfxImagePropData (ofxImageEffect.h:1531)
pub const OFX_IMAGE_PROP_DATA: &CStr = c"OfxImagePropData";
/// kOfxImagePropRowBytes (ofxImageEffect.h:1581)
pub const OFX_IMAGE_PROP_ROW_BYTES: &CStr = c"OfxImagePropRowBytes";
/// kOfxImagePropBounds (ofxImageEffect.h:1548)
pub const OFX_IMAGE_PROP_BOUNDS: &CStr = c"OfxImagePropBounds";
/// kOfxImagePropPixelAspectRatio (ofxImageEffect.h:1283). Also a clip property.
pub const OFX_IMAGE_PROP_PIXEL_ASPECT_RATIO: &CStr = c"OfxImagePropPixelAspectRatio";
/// kOfxImagePropRegionOfDefinition (ofxImageEffect.h:1565)
pub const OFX_IMAGE_PROP_REGION_OF_DEFINITION: &CStr = c"OfxImagePropRegionOfDefinition";
/// kOfxImageEffectPropPixelDepth (ofxImageEffect.h:1092)
pub const OFX_IMAGE_EFFECT_PROP_PIXEL_DEPTH: &CStr = c"OfxImageEffectPropPixelDepth";
/// kOfxImageEffectPropComponents (ofxImageEffect.h:1112)
pub const OFX_IMAGE_EFFECT_PROP_COMPONENTS: &CStr = c"OfxImageEffectPropComponents";
/// kOfxImageClipPropConnected (ofxImageEffect.h:1360)
pub const OFX_IMAGE_CLIP_PROP_CONNECTED: &CStr = c"OfxImageClipPropConnected";
/// kOfxImageEffectPropPreMultiplication (ofxImageEffect.h:1197)
pub const OFX_IMAGE_EFFECT_PROP_PRE_MULTIPLICATION: &CStr = c"OfxImageEffectPropPreMultiplication";
/// kOfxImagePreMultiplied (ofxImageEffect.h:1207). The string value is
/// "`OfxImageAlphaPremultiplied`"; the macro name differs, and the string is fixed by ABI.
pub const OFX_IMAGE_PRE_MULTIPLIED: &CStr = c"OfxImageAlphaPremultiplied";
/// `OfxImageClipPropComponents_` with the clip name `Output` appended (ofxImageEffect.h:332).
pub const OFX_IMAGE_CLIP_PROP_COMPONENTS_OUTPUT: &CStr = c"OfxImageClipPropComponents_Output";
/// `OfxImageClipPropDepth_` with the clip name `Output` appended (ofxImageEffect.h:338).
pub const OFX_IMAGE_CLIP_PROP_DEPTH_OUTPUT: &CStr = c"OfxImageClipPropDepth_Output";
/// kOfxImageEffectPropRenderWindow (ofxImageEffect.h:1690). Four ints: x1, y1, x2, y2.
pub const OFX_IMAGE_EFFECT_PROP_RENDER_WINDOW: &CStr = c"OfxImageEffectPropRenderWindow";
/// kOfxImageEffectPropRegionOfInterest (ofxImageEffect.h:1678). Four doubles in canonical
/// coordinates: x1, y1, x2, y2. The `GetRegionsOfInterest` inArgs carry the requested output region.
pub const OFX_IMAGE_EFFECT_PROP_REGION_OF_INTEREST: &CStr = c"OfxImageEffectPropRegionOfInterest";
/// kOfxImageEffectPropRenderScale (ofxImageEffect.h:1381). Two doubles; excludes the pixel aspect ratio.
pub const OFX_IMAGE_EFFECT_PROP_RENDER_SCALE: &CStr = c"OfxImageEffectPropRenderScale";
/// Prefix of the per-clip `RoI` out-args property, followed by the clip name (ofxImageEffect.h:182).
pub const OFX_IMAGE_CLIP_PROP_ROI_PREFIX: &str = "OfxImageClipPropRoI_";
/// kOfxPropTime (ofxCore.h:696)
pub const OFX_PROP_TIME: &CStr = c"OfxPropTime";
/// kOfxPropInstanceData (ofxCore.h:732). A pointer on the effect property set.
pub const OFX_PROP_INSTANCE_DATA: &CStr = c"OfxPropInstanceData";

/// kOfxImageEffectPropSupportsMultipleClipDepths (ofxImageEffect.h:816).
/// The header's string value is "`OfxImageEffectPropMultipleClipDepths`", not the constant name.
pub const OFX_IMAGE_EFFECT_PROP_SUPPORTS_MULTIPLE_CLIP_DEPTHS: &CStr =
    c"OfxImageEffectPropMultipleClipDepths";

/// kOfxParamPropDefault (ofxParam.h:422)
pub const OFX_PARAM_PROP_DEFAULT: &CStr = c"OfxParamPropDefault";
/// kOfxParamPropMin (ofxParam.h:821)
pub const OFX_PARAM_PROP_MIN: &CStr = c"OfxParamPropMin";
/// kOfxParamPropMax (ofxParam.h:831)
pub const OFX_PARAM_PROP_MAX: &CStr = c"OfxParamPropMax";
/// kOfxParamPropDisplayMin (ofxParam.h:841)
pub const OFX_PARAM_PROP_DISPLAY_MIN: &CStr = c"OfxParamPropDisplayMin";
/// kOfxParamPropDisplayMax (ofxParam.h:851)
pub const OFX_PARAM_PROP_DISPLAY_MAX: &CStr = c"OfxParamPropDisplayMax";
/// kOfxParamPropHint (ofxParam.h:395)
pub const OFX_PARAM_PROP_HINT: &CStr = c"OfxParamPropHint";
/// kOfxParamPropChoiceOption (ofxParam.h:735)
pub const OFX_PARAM_PROP_CHOICE_OPTION: &CStr = c"OfxParamPropChoiceOption";
/// kOfxParamPropAnimates (ofxParam.h:259)
pub const OFX_PARAM_PROP_ANIMATES: &CStr = c"OfxParamPropAnimates";
/// kOfxParamPropParent (ofxParam.h:695)
pub const OFX_PARAM_PROP_PARENT: &CStr = c"OfxParamPropParent";
/// kOfxPluginPropParamPageOrder (ofxParam.h:669)
pub const OFX_PLUGIN_PROP_PARAM_PAGE_ORDER: &CStr = c"OfxPluginPropParamPageOrder";
/// kOfxParamPropPageChild (ofxParam.h:682)
pub const OFX_PARAM_PROP_PAGE_CHILD: &CStr = c"OfxParamPropPageChild";
/// kOfxParamPropSecret (ofxParam.h:345)
pub const OFX_PARAM_PROP_SECRET: &CStr = c"OfxParamPropSecret";
/// kOfxParamPropEnabled (ofxParam.h:714)
pub const OFX_PARAM_PROP_ENABLED: &CStr = c"OfxParamPropEnabled";

/// kOfxParamTypeInteger (ofxParam.h:42)
pub const OFX_PARAM_TYPE_INTEGER: &CStr = c"OfxParamTypeInteger";
/// kOfxParamTypeDouble (ofxParam.h:44)
pub const OFX_PARAM_TYPE_DOUBLE: &CStr = c"OfxParamTypeDouble";
/// kOfxParamTypeBoolean (ofxParam.h:46)
pub const OFX_PARAM_TYPE_BOOLEAN: &CStr = c"OfxParamTypeBoolean";
/// kOfxParamTypeChoice (ofxParam.h:48)
pub const OFX_PARAM_TYPE_CHOICE: &CStr = c"OfxParamTypeChoice";
/// kOfxParamTypeGroup (ofxParam.h:70)
pub const OFX_PARAM_TYPE_GROUP: &CStr = c"OfxParamTypeGroup";
/// kOfxParamTypePage (ofxParam.h:72)
pub const OFX_PARAM_TYPE_PAGE: &CStr = c"OfxParamTypePage";

/// kOfxMessageError (ofxMessage.h:28)
pub const OFX_MESSAGE_ERROR: &CStr = c"OfxMessageError";
