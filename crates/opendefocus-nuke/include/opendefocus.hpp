/// OpenDefocus Nuke plugin

#pragma once

#include "DDImage/Application.h"
#include "DDImage/CameraOp.h"
#include "DDImage/PlanarIop.h"
#include "DDImage/Knobs.h"
#include "DDImage/Op.h"
#include "opendefocus-nuke/src/lib.rs.h"
#include "rust/cxx.h"

static const char *const CLASS = "OpenDefocus";
static const char *const HELP = "All in one defocus node able to "
                                "create optical accurate lens effects.";

/// Image node input index
static const int IMAGE_INPUT = 0;
/// Image node input index
static const int FILTER_INPUT = 1;
/// Image node input index
static const int CAMERA_INPUT = 2;

NukeCameraData get_camera_data(DD::Image::CameraOp *camera);

/// Nuke node implementation for the OpenDefocus library.
/// Basically a wrapper around the Rust API to Nuke, no calculations are
/// actually done here.
class OpenDefocus : public DD::Image::PlanarIop
{
private:
  rust::Box<OpenDefocusNukeInstance> instance;
  std::unique_ptr<DD::Image::Format> preview_format;
  std::unique_ptr<DD::Image::FormatPair> format_pair;

  DD::Image::DeepOp *get_deep_op();

  /// @brief Get the default input for a specific index.
  /// @param idx Input index.
  /// @return Pointer to the default input operation, or nullptr if not
  /// available.
  DD::Image::Op *default_input(int idx) const;

  DD::Image::FormatPair *get_bokeh_preview_format_pair();
  void update_bokeh_preview_format_pair();
  int get_bokeh_resolution();
  DD::Image::Box get_filter_bbox();
  void render_preview_bokeh(DD::Image::ImagePlane *output_plane);
  /// @brief check if selected depth channel is valid and else raise op error.
  void is_depth_channel_valid(DD::Image::ChannelSet input_channels);
  void setup_filter_rendering();

public:
  /// @brief Constructor for OpenDefocus class.
  /// @param node Pointer to the Nuke node.
  OpenDefocus(Node *node)
      : PlanarIop(node),
        instance(OpenDefocusNukeInstance::create(NukeSpecificSettings::create(), DD::Image::Application::gui))
  {
    slowness(100);
    inputs(3);
  }

  /// @brief Destructor for OpenDefocus class.
  ~OpenDefocus();

  void validate_depth();
  /// @brief Create and initialize knobs (UI controls).
  /// @param f Knob callback function.
  void knobs(DD::Image::Knob_Callback f) { instance->create_knobs(f); }

  void _validate(bool for_real);

  DD::Image::Knob *get_knob(rust::string name);

  /// @brief Get the input label for a specific input.
  /// @param input Input index.
  /// @param buffer Character buffer to store the label.
  /// @return Pointer to the input label string.
  const char *input_label(int input, char *buffer) const
  {
    switch (input)
    {
    case IMAGE_INPUT:
      return "image";
    case FILTER_INPUT:
      return "filter";
    case CAMERA_INPUT:
      return "cam";
    default:
      return 0;
    }
  }
  /// @brief Test if a specific input is allowed to connect.
  /// @param input Input index.
  /// @param op Pointer to the input operation.
  /// @return True if the input is allowed, false otherwise.
  bool test_input(int input, DD::Image::Op *op) const
  {
    switch (input)
    {
    case IMAGE_INPUT:
      return dynamic_cast<DD::Image::Iop *>(op);
    case CAMERA_INPUT:
      return dynamic_cast<DD::Image::CameraOp *>(op);
    default:
      break;
    }
    return DD::Image::Iop::test_input(input, op);
  }

  /// @brief Get rendering requests for the specified region.
  /// @param box Region to process.
  /// @param channels Channels to process.
  /// @param count Number of requests.
  /// @param reqData Target to let Nuke now what we need for data
  void getRequests(const DD::Image::Box &box,
                   const DD::Image::ChannelSet &channels, int count,
                   DD::Image::RequestOutput &reqData) const;

  /// @brief Check if stripes should be used for rendering.
  /// @return True if stripes should be used, false otherwise.
  virtual bool useStripes() const { return instance->use_stripes(); }
  /// @brief Get the packed preference for the operation.
  /// @return Packed preference.
  virtual DD::Image::PlanarI::PackedPreference packedPreference() const
  {
    return PackedPreference::
        ePackedPreferenceNone; // actually i would prefer packed but some nodes
                               // dont respect this settings (looking at you
                               // median)
  }
  /// @brief Get the optimal stripe height.
  /// @return Stripe height.
  virtual size_t stripeHeight() const { return instance->stripe_height(); }
  void renderStripe(DD::Image::ImagePlane &outputPlane);
  int knob_changed(DD::Image::Knob *k);

  const char *Class() const { return CLASS; }
  const char *node_help() const { return HELP; }
  const char *displayName() const { return CLASS; }
  static const Iop::Description description;
};

size_t get_imageplane_size(DD::Image::ImagePlaneDescriptor descriptor);

static DD::Image::Iop *OpenDefocusCreate(Node *node);

void create_float_knob(DD::Image::Knob_Callback callback, float *value,
                       KnobParameters parameters);
void create_int_knob(DD::Image::Knob_Callback callback, int *value,
                     KnobParameters parameters);
void create_bool_knob(DD::Image::Knob_Callback callback, bool *value,
                      KnobParameters parameters);
void create_xy_knob(DD::Image::Knob_Callback callback, rust::Slice<float> value,
                    KnobParameters parameters);
void create_enumeration_knob(DD::Image::Knob_Callback callback, int *value,
                             KnobParameters parameters);
void create_inputchannelset_knob(DD::Image::Knob_Callback callback,
                                 std::unique_ptr<DD::Image::ChannelSet> *channel,
                                 KnobParameters parameters);
void create_inputonlychannel_knob(DD::Image::Knob_Callback callback, DD::Image::Channel *channel,
                                  KnobParameters parameters);
void create_tab_knob(DD::Image::Knob_Callback callback, KnobParameters parameters);
void create_newline_knob(DD::Image::Knob_Callback callback, KnobParameters parameters);
void create_text_knob(DD::Image::Knob_Callback callback, KnobParameters parameters);
void create_divider_knob(DD::Image::Knob_Callback callback);

void set_knobchanged(DD::Image::Op const &node, rust::string name,
                     KnobChanged knob_changed);

bool input_connected(DD::Image::Op const &node, uint32_t input);

float sample_channel(DD::Image::Op const &node, DD::Image::Channel channel,
                     std::array<float, 2> coordinates);

bool aborted(DD::Image::Op const &node);

std::shared_ptr<DD::Image::ChannelSet> get_channelset(DD::Image::ChannelSetInit channels);
std::shared_ptr<DD::Image::Format> create_format();