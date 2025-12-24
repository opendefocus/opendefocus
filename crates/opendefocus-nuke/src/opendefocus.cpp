/// OpenDefocus Nuke plugin

#include "opendefocus-nuke/include/opendefocus.hpp"
#include "DDImage/Application.h"
#include "DDImage/CameraOp.h"
#include "DDImage/Format.h"
#include "DDImage/Iop.h"
#include "DDImage/Knob.h"
#include "DDImage/Knobs.h"
#include "DDImage/Op.h"
#include "DDImage/TextKnobI.h"
#include <future>
#include <memory>


DD::Image::Op *OpenDefocus::default_input(int idx) const {

  if (idx == CAMERA_INPUT) {
    return nullptr;
  }
  return DD::Image::PlanarIop::default_input(idx);
}

OpenDefocus::~OpenDefocus() {}

static DD::Image::Iop *OpenDefocusCreate(Node *node) {
  return new OpenDefocus(node);
}
const DD::Image::Iop::Description
    OpenDefocus::description(CLASS, "Filter/OpenDefocus", OpenDefocusCreate);

void OpenDefocus::setup_filter_rendering() {
  auto resolution = instance->calculate_filter_box();
  DD::Image::Box box(resolution[0], resolution[1], resolution[2],
                     resolution[3]);
  info_.setBox(box);

  instance->nuke_settings().filter_format->set(box);

  info_.full_size_format(*instance->nuke_settings().filter_format);
  info_.format(*instance->nuke_settings().filter_format);

  info_.channels(DD::Image::Mask_RGBA);
  set_out_channels(DD::Image::Mask_RGBA);
}

void OpenDefocus::_validate(bool for_real) {

  if (instance->render_filter_only()) {
    setup_filter_rendering();
    return;
  };
  instance->set_proxy_scale(outputContext().scale_x());
  if (!this->node_input(IMAGE_INPUT)) {
    set_out_channels(DD::Image::Mask_None);
    return;
  };
  copy_info();

  DD::Image::CameraOp *camera_input =
      dynamic_cast<DD::Image::CameraOp *>(DD::Image::Op::input(CAMERA_INPUT));

  if (camera_input) {
    camera_input->validate(for_real);
    NukeCameraData camera_data = get_camera_data(camera_input);
    instance->set_camera_data(camera_data);
  };

  if (!instance->is_2d()) {
    validate_depth();
  }

  try {
    instance->validate(*this, {info_.w(), info_.h()},
                       {info_.center_x() - info_.box().x(),
                        info_.center_y() - info_.box().y()},
                       info_.format().pixel_aspect());
  } catch (rust::Error error) {
    DD::Image::Op::error(error.what());
  };

  if (instance->fetch_filter()) {
    input(1)->validate(for_real);
  }
  set_out_channels(*(instance->nuke_settings().channels.get()));
}

void OpenDefocus::getRequests(const DD::Image::Box &box,
                              const DD::Image::ChannelSet &channels, int count,
                              DD::Image::RequestOutput &reqData) const {
  DD::Image::ChannelSet required_channels(channels);
  DD::Image::Box input_box = box;
  input_box.expand(instance->get_padding());
  if (instance->fetch_filter() && DD::Image::Op::input(FILTER_INPUT)) {
    DD::Image::Box filter_box = input(FILTER_INPUT)->info().box();
    reqData.request(input(FILTER_INPUT), filter_box, required_channels, count);
  }

  if (!instance->is_2d()) {
    required_channels += instance->nuke_settings().depth_channel;
  }
  reqData.request(&input0(), input_box, required_channels, count);
}

void OpenDefocus::renderStripe(DD::Image::ImagePlane &output_plane) {
  instance->set_aborted(aborted());

  DD::Image::Box process_box = output_plane.bounds();

  process_box.expand(instance->get_padding());

  auto main_input = input(IMAGE_INPUT);
  if (!main_input && instance->fetch_image()) {
    DD::Image::Op::error("Main input is not connected but is required.");
    return;
  };

  if (instance->fetch_image()) {
    auto image_info = main_input->info();
    process_box.intersect(image_info);
  }

  DD::Image::ImagePlane depth_plane(process_box,
                                    DD::Image::PlanarI::ePackedPreferencePacked,
                                    instance->nuke_settings().depth_channel);
  DD::Image::ImagePlane working_plane(
      process_box, DD::Image::PlanarI::ePackedPreferencePacked,
      output_plane.channels());
  if (aborted()) {
    return;
  }
  if (instance->fetch_image()) {
    main_input->fetchPlane(working_plane);
  } else {
    working_plane.makeWritable();
    foreach (c, output_plane.channels()) {
      working_plane.fillChannelThreaded(c, 0.0);
    }
  }

  if (instance->fetch_depth()) {
    main_input->fetchPlane(depth_plane);
  }

  DD::Image::ImagePlane filter_plane(
      DD::Image::Box(0, 0, 0, 0), DD::Image::PlanarI::ePackedPreferencePacked,
      DD::Image::Mask_None, 0);
  if (instance->fetch_filter()) {
    auto input_filter = input(FILTER_INPUT);
    if (!input_filter) {
      DD::Image::Op::error("Input filter is not connected");
      return;
    }
    auto filter_box = input_filter->info().box();
    filter_plane = DD::Image::ImagePlane(
        filter_box, DD::Image::PlanarI::ePackedPreferencePacked,
        output_plane.channels());
    input_filter->fetchPlane(filter_plane);
  }

  if (aborted()) {
    return;
  }

  rust::Slice<float> image_slice(working_plane.writable(),
                                 get_imageplane_size(working_plane));
  rust::Slice<const float> depth_slice =
      instance->fetch_depth() ? rust::Slice(depth_plane.readable(),
                                            get_imageplane_size(depth_plane))
                              : rust::Slice<const float>(0, 0);
  rust::Slice<const float> filter_slice =
      instance->fetch_filter() ? rust::Slice(filter_plane.readable(),
                                             get_imageplane_size(filter_plane))
                               : rust::Slice<const float>(0, 0);

  DD::Image::Box stripe_region = process_box;
  stripe_region.expand(2);

  try {
    instance->render(*this, image_slice, depth_slice,
                     working_plane.channels().size(),
                     {
                         process_box.x() - info_.box().x(),
                         process_box.y() - info_.box().y(),
                         process_box.r() - info_.box().x(),
                         process_box.t() - info_.box().y(),
                     },
                     {
                         stripe_region.x() - info_.box().x(),
                         stripe_region.y() - info_.box().y(),
                         stripe_region.r() - info_.box().x(),
                         stripe_region.t() - info_.box().y(),
                     },
                     filter_slice, filter_plane.channels().size(),
                     {
                         filter_plane.bounds().w(),
                         filter_plane.bounds().h(),
                     });
  } catch (const rust::Error &error) {
    DD::Image::Op::error(error.what());
  } catch (const std::exception &e) {
    std::cerr << "Standard exception: " << e.what() << std::endl;
  } catch (...) {
    std::cerr << "Unknown error occurred." << std::endl;
  }

  if (aborted()) {
    return;
  }

  output_plane.copyIntersectionFrom(working_plane);
}
/// @brief Validate the selected depth channel.
/// @param input_channels Input channel set.
/// @note Raises an error if the depth channel is not available.
void OpenDefocus::validate_depth() {
  DD::Image::Channel depth_channel = instance->nuke_settings().depth_channel;
  if (info_.channels().contains(depth_channel)) {
    return;
  }
  const char *depth_channel_name = DD::Image::getName(depth_channel);
  std::string error_message = "Selected depth channel '" +
                              std::string(depth_channel_name) +
                              "' is not available from input.";
  DD::Image::Op::error(error_message.c_str());
}

NukeCameraData get_camera_data(DD::Image::CameraOp *camera) {
#if kDDImageVersionMajorNum < 14 // Nuke 13 support
  return NukeCameraData::create(
      camera->focal_length(), camera->fstop(), camera->focal_point(),
      {(float)camera->film_width(), (float)camera->film_height()},
      camera->Near(), camera->Far());

#else
  return NukeCameraData::create(
      camera->focalLength(), camera->fStop(), camera->focusDistance(),
      {(float)camera->horizontalAperture(), (float)camera->verticalAperture()},
      camera->nearPlaneDistance(), camera->farPlaneDistance());
#endif
}

int OpenDefocus::knob_changed(DD::Image::Knob *k) {
  {
    try {
      if (instance->knob_changed(*this, k->name())) {
        return 1;
      }
    } catch (rust::Error error) {
      log_error(error.what());
    };

    return DD::Image::PlanarIop::knob_changed(k);
  }
}

size_t get_imageplane_size(DD::Image::ImagePlaneDescriptor descriptor) {
  return descriptor.bounds().w() * descriptor.bounds().h() *
         descriptor.channels().size();
}

void set_parameters(DD::Image::Knob_Callback callback,
                    KnobParameters &parameters) {
  SetFlags(callback, parameters.with_flags);
  ClearFlags(callback, parameters.without_flags);
  if (parameters.tooltip.length()) {
    Tooltip(callback, parameters.tooltip.c_str());
  }
  if (parameters.range[1] != 0.0 || parameters.range[1] != 0.0) {
    SetRange(callback, parameters.range[0], parameters.range[1]);
  }
}

void create_float_knob(DD::Image::Knob_Callback callback, float *value,
                       KnobParameters parameters) {
  Float_knob(callback, value, parameters.name.c_str(),
             parameters.label.empty() ? nullptr : parameters.label.c_str());
  set_parameters(callback, parameters);
}

void create_int_knob(DD::Image::Knob_Callback callback, int *value,
                     KnobParameters parameters) {
  Int_knob(callback, value, parameters.name.c_str(),
           parameters.label.empty() ? nullptr : parameters.label.c_str());
  set_parameters(callback, parameters);
}
void create_bool_knob(DD::Image::Knob_Callback callback, bool *value,
                      KnobParameters parameters) {
  Bool_knob(callback, value, parameters.name.c_str(),
            parameters.label.empty() ? nullptr : parameters.label.c_str());
  set_parameters(callback, parameters);
}
void create_xy_knob(DD::Image::Knob_Callback callback, rust::Slice<float> value,
                    KnobParameters parameters) {
  XY_knob(callback, value.data(), parameters.name.c_str(),
          parameters.label.empty() ? nullptr : parameters.label.c_str());
}
void create_enumeration_knob(DD::Image::Knob_Callback callback, int *value,
                             KnobParameters parameters) {
  // Use std::vector to store enum labels
  std::vector<const char *> enum_labels(parameters.enum_labels.size() + 1);

  for (size_t i = 0; i < parameters.enum_labels.size(); ++i) {
    enum_labels[i] = parameters.enum_labels[i].c_str();
  }

  // Set the last element to nullptr
  enum_labels[parameters.enum_labels.size()] = nullptr;

  // Call Enumeration_knob with enum_labels
  Enumeration_knob(callback, value, enum_labels.data(), parameters.name.c_str(),
                   parameters.label.empty() ? nullptr
                                            : parameters.label.c_str());

  // Set additional parameters
  set_parameters(callback, parameters);
}

void create_inputchannelset_knob(DD::Image::Knob_Callback callback,
                                 std::unique_ptr<DD::Image::ChannelSet> *channel,
                                 KnobParameters parameters) {
  Input_ChannelSet_knob(callback, channel->get(), parameters.input,
                        parameters.label.empty() ? nullptr
                                                 : parameters.label.c_str());
  set_parameters(callback, parameters);
}

void create_inputonlychannel_knob(DD::Image::Knob_Callback callback,
                                  DD::Image::Channel *channel,
                                  KnobParameters parameters) {
  InputOnly_Channel_knob(
      callback, reinterpret_cast<DD::Image::Channel *>(channel),
      parameters.count, parameters.input,
      parameters.label.empty() ? nullptr : parameters.label.c_str());
  set_parameters(callback, parameters);
}

void create_tab_knob(DD::Image::Knob_Callback callback,
                     KnobParameters parameters) {
  Tab_knob(callback,
           parameters.label.empty() ? nullptr : parameters.label.c_str());
}

void create_newline_knob(DD::Image::Knob_Callback callback,
                         KnobParameters parameters) {
  Newline(callback, parameters.label.empty() ? " " : parameters.label.c_str());
}

void create_divider_knob(DD::Image::Knob_Callback callback) {
  Divider(callback);
}

void create_text_knob(DD::Image::Knob_Callback callback,
                      KnobParameters parameters) {
  if (parameters.name != "") {
    Named_Text_knob(callback, parameters.name.c_str(),
                    parameters.label.c_str());
  } else {
    Text_knob(callback, parameters.label.c_str());
  }
}

void set_knobchanged(DD::Image::Op const &node, rust::string name,
                     KnobChanged knob_changed) {
  auto knob = node.knob(name.c_str());
  if (!knob) {
    return;
  }
  knob->enable(knob_changed.enabled);
  knob->visible(knob_changed.visible);

  if (knob_changed.set_value) {
    switch (knob_changed.value_change.value_type) {
    case ValueType::Bool:
      knob->set_value(knob_changed.value_change.bool_value);
      break;
    case ValueType::Int:
      knob->set_value(knob_changed.value_change.int_value);
      break;
    case ValueType::Text: {
      DD::Image::Text_KnobI *text_knob = dynamic_cast<DD::Image::Text_KnobI *>(knob);
      if (!text_knob) {
        log_warning("Knob is not a Text_KnobI.");
        return;
      }
      std::string stdstring = std::string(knob_changed.value_change.text_value);
      text_knob->text(stdstring);
    } break;
    default:
      knob->set_value(knob_changed.value_change.float_value);
      break;
    }
  }
}

void set_enabled(DD::Image::Knob *knob, bool enable) {
  if (!knob) {
    return;
  }
  knob->enable(enable);
}

bool input_connected(DD::Image::Op const &node, uint32_t input) {
  return node.node_input(input);
}

float sample_channel(DD::Image::Op const &node, DD::Image::Channel channel,
                     std::array<float, 2> coordinates) {
  const DD::Image::Iop *this_node = dynamic_cast<const DD::Image::Iop *>(&node);
  if (!this_node) {
    log_warning("Invalid node provided during sampling");
    return 0.0;
  }
  DD::Image::Iop *input_iop = this_node->input(0);
  if (!input_iop) {
    log_warning("Input node is not connected or is not an image op.");
    return 0.0;
  };
  input_iop->validate(true);

  DD::Image::Box sample_box(coordinates[0] - 1, coordinates[1] - 1,
                            coordinates[0], coordinates[1]);

  if (!this_node->info().box().contains(sample_box)) {
    return 0.0;
  }

  return input_iop->at(sample_box.x(), sample_box.y(), channel);
}

bool aborted(DD::Image::Op const &node) { return node.aborted(); }

std::shared_ptr<DD::Image::ChannelSet>
get_channelset(DD::Image::ChannelSetInit channels) {
  return std::make_shared<DD::Image::ChannelSet>(channels);
}

std::shared_ptr<DD::Image::Format> create_format() {
  return std::make_shared<DD::Image::Format>();
}
