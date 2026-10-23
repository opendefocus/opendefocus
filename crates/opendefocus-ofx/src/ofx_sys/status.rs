//! OFX status codes returned by host and plugin actions, from `ofxCore.h` and `ofxImageEffect.h`.

use super::core_types::OfxStatus;

/// kOfxStatOK (ofxCore.h:996)
pub const OFX_STAT_OK: OfxStatus = 0;
/// kOfxStatFailed (ofxCore.h:999)
pub const OFX_STAT_FAILED: OfxStatus = 1;
/// kOfxStatErrFatal (ofxCore.h:1005)
pub const OFX_STAT_ERR_FATAL: OfxStatus = 2;
/// kOfxStatErrUnknown (ofxCore.h:1008)
pub const OFX_STAT_ERR_UNKNOWN: OfxStatus = 3;
/// kOfxStatErrMissingHostFeature (ofxCore.h:1014)
pub const OFX_STAT_ERR_MISSING_HOST_FEATURE: OfxStatus = 4;
/// kOfxStatErrUnsupported (ofxCore.h:1017)
pub const OFX_STAT_ERR_UNSUPPORTED: OfxStatus = 5;
/// kOfxStatErrExists (ofxCore.h:1020)
pub const OFX_STAT_ERR_EXISTS: OfxStatus = 6;
/// kOfxStatErrFormat (ofxCore.h:1023)
pub const OFX_STAT_ERR_FORMAT: OfxStatus = 7;
/// kOfxStatErrMemory (ofxCore.h:1026)
pub const OFX_STAT_ERR_MEMORY: OfxStatus = 8;
/// kOfxStatErrBadHandle (ofxCore.h:1029)
pub const OFX_STAT_ERR_BAD_HANDLE: OfxStatus = 9;
/// kOfxStatErrBadIndex (ofxCore.h:1032)
pub const OFX_STAT_ERR_BAD_INDEX: OfxStatus = 10;
/// kOfxStatErrValue (ofxCore.h:1035)
pub const OFX_STAT_ERR_VALUE: OfxStatus = 11;
/// kOfxStatReplyYes (ofxCore.h:1038)
pub const OFX_STAT_REPLY_YES: OfxStatus = 12;
/// kOfxStatReplyNo (ofxCore.h:1041)
pub const OFX_STAT_REPLY_NO: OfxStatus = 13;
/// kOfxStatReplyDefault (ofxCore.h:1044)
pub const OFX_STAT_REPLY_DEFAULT: OfxStatus = 14;
/// kOfxStatUnlicensed (ofxCore.h:1047)
pub const OFX_STAT_UNLICENSED: OfxStatus = 15;
/// kOfxStatErrImageFormat (ofxImageEffect.h:2056)
pub const OFX_STAT_ERR_IMAGE_FORMAT: OfxStatus = 1000;
