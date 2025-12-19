#pragma once

#include "DDImage/Knobs.h"
#include "DDImage/Op.h"
#include "opendefocus-nuke/src/lib.rs.h"
#include "rust/cxx.h"

using namespace DD::Image;

void create_float_knob(Knob_Callback callback, float *value,
                       KnobParameters parameters);
void create_int_knob(Knob_Callback callback, int *value,
                     KnobParameters parameters);
void create_bool_knob(Knob_Callback callback, bool *value,
                      KnobParameters parameters);
void create_xy_knob(Knob_Callback callback, rust::Slice<float> value,
                    KnobParameters parameters);
void create_enumeration_knob(Knob_Callback callback, int *value,
                             KnobParameters parameters);
void create_inputchannelset_knob(Knob_Callback callback,
                                 std::unique_ptr<ChannelSet> *channel,
                                 KnobParameters parameters);
void create_inputonlychannel_knob(Knob_Callback callback, DD::Image::Channel *channel,
                                  KnobParameters parameters);
void create_tab_knob(Knob_Callback callback, KnobParameters parameters);
void create_newline_knob(Knob_Callback callback, KnobParameters parameters);
void create_text_knob(Knob_Callback callback, KnobParameters parameters);
void create_divider_knob(Knob_Callback callback);

void set_knobchanged(DD::Image::Op const &node, rust::string name,
                     KnobChanged knob_changed);

bool input_connected(DD::Image::Op const &node, uint32_t input);

float sample_channel(DD::Image::Op const &node, DD::Image::Channel channel,
                     std::array<float, 2> coordinates);

bool aborted(DD::Image::Op const &node);

std::shared_ptr<DD::Image::ChannelSet> get_channelset(DD::Image::ChannelSetInit channels);
std::shared_ptr<DD::Image::Format> create_format();