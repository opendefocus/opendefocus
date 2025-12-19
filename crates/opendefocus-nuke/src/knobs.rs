use cxx::UniquePtr;

use crate::{
    consts::KnobDefinition,
    ffi::{
        Channel, ChannelSet, Knob_Callback, KnobType, create_bool_knob, create_divider_knob, create_enumeration_knob, create_float_knob, create_inputchannelset_knob, create_inputonlychannel_knob, create_int_knob, create_newline_knob, create_tab_knob, create_text_knob, create_xy_knob
    },
};

pub fn create_knob_with_value<T>(
    callback: &Knob_Callback,
    definition: KnobDefinition,
    knob_value: *mut T,
) {
    match definition.knob_type() {
        KnobType::Bool => {
            let casted_value = knob_value as *mut bool;
            unsafe { create_bool_knob(callback, casted_value, definition.parameters()) }
        }
        KnobType::Float => {
            let casted_value = knob_value as *mut f32;
            unsafe { create_float_knob(callback, casted_value, definition.parameters()) }
        }
        KnobType::Int => {
            let casted_value = knob_value as *mut i32;
            unsafe { create_int_knob(callback, casted_value, definition.parameters()) }
        }
        KnobType::Enumeration => {
            let casted_value = knob_value as *mut i32;
            unsafe { create_enumeration_knob(callback, casted_value, definition.parameters()) }
        }
        KnobType::InputChannelSet => {
            let casted_value = knob_value as *mut UniquePtr<ChannelSet>;
            unsafe { create_inputchannelset_knob(callback, casted_value, definition.parameters()) }
        }
        KnobType::InputOnlyChannel => {
            let casted_value = knob_value as *mut Channel;
            unsafe { create_inputonlychannel_knob(callback, casted_value, definition.parameters()) }
        }
        KnobType::XY => {
            let slice: &mut [f32] =
                unsafe { std::slice::from_raw_parts_mut(knob_value as *mut f32, 2) };
            unsafe { create_xy_knob(callback, slice, definition.parameters()) }
        }
        _ => (),
    }
}

pub fn create_knob(callback: &Knob_Callback, definition: KnobDefinition) {
    match definition.knob_type() {
        KnobType::Text => create_text_knob(callback, definition.parameters()),
        KnobType::NamedText => create_text_knob(callback, definition.parameters()),
        KnobType::Newline => create_newline_knob(callback, definition.parameters()),
        KnobType::Tab => create_tab_knob(callback, definition.parameters()),
        KnobType::Divider => create_divider_knob(callback),
        _ => (),
    }
}
