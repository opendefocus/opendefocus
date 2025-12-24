// Wrapping of DDImage api into Rust native types

#include "opendefocus-nuke/include/bridge.hpp"
#ifdef __APPLE__
  #include "opendefocus-nuke/include/opendefocus.hpp"
#endif
#include "DDImage/Iop.h"
#include "DDImage/Knobs.h"
#include "DDImage/Op.h"
#include "DDImage/TextKnobI.h"
#include <memory>

void set_parameters(DD::Image::Knob_Callback callback,
                    KnobParameters &parameters)
{
  SetFlags(callback, parameters.with_flags);
  ClearFlags(callback, parameters.without_flags);
  if (parameters.tooltip.length())
  {
    Tooltip(callback, parameters.tooltip.c_str());
  }
  if (parameters.range[1] != 0.0 || parameters.range[1] != 0.0)
  {
    SetRange(callback, parameters.range[0], parameters.range[1]);
  }
}

void create_float_knob(Knob_Callback callback, float *value,
                       KnobParameters parameters)
{
  Float_knob(callback, value, parameters.name.c_str(),
             parameters.label.empty() ? nullptr : parameters.label.c_str());
  set_parameters(callback, parameters);
}

void create_int_knob(Knob_Callback callback, int *value,
                     KnobParameters parameters)
{
  Int_knob(callback, value, parameters.name.c_str(),
           parameters.label.empty() ? nullptr : parameters.label.c_str());
  set_parameters(callback, parameters);
}
void create_bool_knob(Knob_Callback callback, bool *value,
                      KnobParameters parameters)
{
  Bool_knob(callback, value, parameters.name.c_str(),
            parameters.label.empty() ? nullptr : parameters.label.c_str());
  set_parameters(callback, parameters);
}
void create_xy_knob(Knob_Callback callback, rust::Slice<float> value,
                    KnobParameters parameters)
{
  XY_knob(callback, value.data(), parameters.name.c_str(),
          parameters.label.empty() ? nullptr : parameters.label.c_str());
}
void create_enumeration_knob(Knob_Callback callback, int *value,
                             KnobParameters parameters)
{
    // Use std::vector to store enum labels
    std::vector<const char*> enum_labels(parameters.enum_labels.size() + 1);
    
    for (size_t i = 0; i < parameters.enum_labels.size(); ++i)
    {
        enum_labels[i] = parameters.enum_labels[i].c_str();
    }
    
    // Set the last element to nullptr
    enum_labels[parameters.enum_labels.size()] = nullptr;

    // Call Enumeration_knob with enum_labels
    Enumeration_knob(callback, value, enum_labels.data(), parameters.name.c_str(),
                      parameters.label.empty() ? nullptr : parameters.label.c_str());

    // Set additional parameters
    set_parameters(callback, parameters);
}

void create_inputchannelset_knob(Knob_Callback callback,
                                 std::unique_ptr<ChannelSet> *channel,
                                 KnobParameters parameters)
{
  Input_ChannelSet_knob(callback, channel->get(), parameters.input,
                        parameters.label.empty() ? nullptr
                                                 : parameters.label.c_str());
  set_parameters(callback, parameters);
}

void create_inputonlychannel_knob(Knob_Callback callback, DD::Image::Channel *channel,
                                  KnobParameters parameters)
{
  InputOnly_Channel_knob(
      callback, reinterpret_cast<DD::Image::Channel *>(channel),
      parameters.count, parameters.input,
      parameters.label.empty() ? nullptr : parameters.label.c_str());
  set_parameters(callback, parameters);
}

void create_tab_knob(Knob_Callback callback, KnobParameters parameters)
{
  Tab_knob(callback,
           parameters.label.empty() ? nullptr : parameters.label.c_str());
}

void create_newline_knob(Knob_Callback callback, KnobParameters parameters)
{
  Newline(callback, parameters.label.empty() ? " " : parameters.label.c_str());
}

void create_divider_knob(Knob_Callback callback) { Divider(callback); }

void create_text_knob(Knob_Callback callback, KnobParameters parameters)
{
  if (parameters.name != "")
  {
    Named_Text_knob(callback, parameters.name.c_str(),
                    parameters.label.c_str());
  }
  else
  {
    Text_knob(callback, parameters.label.c_str());
  }
}

void set_knobchanged(DD::Image::Op const &node, rust::string name,
                     KnobChanged knob_changed)
{
  auto knob = node.knob(name.c_str());
  if (!knob)
  {
    return;
  }
  knob->enable(knob_changed.enabled);
  knob->visible(knob_changed.visible);

  if (knob_changed.set_value)
  {
    switch (knob_changed.value_change.value_type)
    {
    case ValueType::Bool:
      knob->set_value(knob_changed.value_change.bool_value);
      break;
    case ValueType::Int:
      knob->set_value(knob_changed.value_change.int_value);
      break;
    case ValueType::Text:
    {
      Text_KnobI *text_knob = dynamic_cast<Text_KnobI *>(knob);
      if (!text_knob)
      {
        log_warning("Knob is not a Text_KnobI.");
        return;
      }
      std::string stdstring = std::string(knob_changed.value_change.text_value);
      text_knob->text(stdstring);
    }
    break;
    default:
      knob->set_value(knob_changed.value_change.float_value);
      break;
    }
  }
}

void set_enabled(Knob *knob, bool enable)
{
  if (!knob)
  {
    return;
  }
  knob->enable(enable);
}

bool input_connected(DD::Image::Op const &node, uint32_t input)
{
  return node.node_input(input);
}

float sample_channel(DD::Image::Op const &node, DD::Image::Channel channel,
                     std::array<float, 2> coordinates)
{
  const DD::Image::Iop *this_node = dynamic_cast<const DD::Image::Iop *>(&node);
  if (!this_node)
  {
    log_warning("Invalid node provided during sampling");
    return 0.0;
  }
  DD::Image::Iop *input_iop = this_node->input(0);
  if (!input_iop)
  {
    log_warning("Input node is not connected or is not an image op.");
    return 0.0;
  };
  input_iop->validate(true);

  DD::Image::Box sample_box(coordinates[0] - 1, coordinates[1] - 1,
                            coordinates[0], coordinates[1]);

  if (!this_node->info().box().contains(sample_box))
  {
    return 0.0;
  }

  return input_iop->at(sample_box.x(), sample_box.y(), channel);
}

bool aborted(DD::Image::Op const &node) { return node.aborted(); }

std::shared_ptr<DD::Image::ChannelSet> get_channelset(DD::Image::ChannelSetInit channels)
{
  return std::make_shared<DD::Image::ChannelSet>(channels);
}

std::shared_ptr<DD::Image::Format> create_format()
{
  return std::make_shared<DD::Image::Format>();
}
