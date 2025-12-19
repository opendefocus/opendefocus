use bitflags::bitflags;
use cxx::{ExternType, type_id};

#[derive(Debug, Clone)]
#[repr(u32)]
pub enum Channel {
    ChanRed = 1,
    ChanGreen = 2,
    ChanBlue = 3,
    ChanAlpha = 4,
    ChanZ = 5,
}

bitflags! {
    #[derive(Debug, Clone)]
    #[repr(C)]
    /// This is not a complete implementation, but just what we need to cover everything
    pub struct ChannelSetInit: u32 {
        const NONE                     = 0;
        const RED                      = 1 << Channel::ChanRed as u32 -  1;
        const GREEN                    = 1 << Channel::ChanGreen as u32 - 1;
        const BLUE                     = 1 << Channel::ChanBlue as u32 - 1;
        const ALPHA                    = 1 << Channel::ChanAlpha as u32 - 1;
        const Z                        = 1 << Channel::ChanZ as u32 - 1;
        const RGB                      = Self::RED.bits() | Self::GREEN.bits() | Self::BLUE.bits();
        const RGBA                     = Self::RGB.bits() | Self::ALPHA.bits();
    }
}

unsafe impl ExternType for Channel {
    type Id = type_id!("DD::Image::Channel");
    type Kind = cxx::kind::Trivial;
}
unsafe impl ExternType for ChannelSetInit {
    type Id = type_id!("DD::Image::ChannelSetInit");
    type Kind = cxx::kind::Trivial;
}
