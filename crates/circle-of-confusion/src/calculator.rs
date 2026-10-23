//! Evaluates the circle of confusion for a single depth value, using the
//! camera-based model when camera data is present and a direct map otherwise.

use crate::{Calculator, DepthOfField, Math, Settings, WorldUnit};

impl Calculator {
    /// Perform calculation based on the input value.
    ///
    /// This can be called upon a depth map for each pixel for example,
    /// to calculate the size of convolution for each pixel.
    #[must_use]
    pub fn calculate(&self, value: f32) -> f32 {
        let mut converted_value = Self::convert_value_to_distance(value, self.settings.math());

        if self.settings.camera_data.is_some() {
            converted_value *= self.world_unit_multiplier;
            converted_value = self.calculate_circle_of_confusion(converted_value);
            converted_value *= self.settings.pixel_aspect;
        } else {
            converted_value = self.calculate_direct_map(converted_value);
        }
        converted_value.clamp(-self.settings.max_size, self.settings.max_size)
    }
}

impl Calculator {
    /// Create a new instance of the Calculator with the specified settings.
    ///
    /// Automatically calculates all necessary values for calculations.
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        let mut instance = Self {
            settings,
            ..Self::default()
        };
        instance.compute();
        instance
    }
    /// Precompute the internal data
    fn compute(&mut self) {
        self.world_unit_multiplier = self.get_world_unit_multiplier();
        self.internal_focus = self.get_internal_focus();
        self.depth_of_field = self.get_depth_of_field();

        self.hyperfocal_distance = Self::calculate_hyperfocal_distance(&self.settings);
    }

    /// Just a wrapper for pythagoras (length) calculation.
    fn length(a: f32, b: f32) -> f32 {
        libm::sqrtf(libm::powf(a, 2.0) + libm::powf(b, 2.0))
    }

    /// Calculate the Zeiss formula to get the criterion.
    ///
    /// More information can be found here:
    /// <https://resources.wolframcloud.com/FormulaRepository/resources/Zeiss-Formula>
    fn calculate_zeiss_formula(settings: &Settings) -> f32 {
        let Some(camera_data) = &settings.camera_data else {
            return 1.0;
        };
        Self::length(camera_data.filmback.width, camera_data.filmback.height) / 1730.0
    }

    /// Calculate the distance where it does not matter
    /// anymore if the focal plane changes
    ///
    /// The `CoC` stays the same after this value.
    ///
    /// More information:
    /// <https://www.watchprosite.com/editors-picks/using-the-zeiss-formula-to-understand-the-circle-of-confusion/1278.1127636.8608906>/
    fn calculate_hyperfocal_distance(settings: &Settings) -> f32 {
        let Some(camera_data) = &settings.camera_data else {
            return 0.0;
        };
        let zeiss_formula = Self::calculate_zeiss_formula(settings);
        (libm::powf(camera_data.focal_length, 2.0) / (camera_data.f_stop * zeiss_formula))
            + camera_data.focal_length
    }

    /// Map the world unit from the settings to a multiplication value
    fn get_world_unit_multiplier(&self) -> f32 {
        let world_unit = match &self.settings.camera_data {
            Some(data) => data.world_unit(),
            None => return 1.0,
        };
        match world_unit {
            WorldUnit::Mm => 1.0,
            WorldUnit::Cm => 10.0,
            WorldUnit::Dm => 100.0,
            WorldUnit::M => 1000.0,
            WorldUnit::In => 25.4,
            WorldUnit::Ft => 304.8,
        }
    }

    /// Convert the distance selected according to the math of the input value and the world unit.
    fn get_internal_focus(&self) -> f32 {
        Self::convert_value_to_distance(self.settings.focal_plane, self.settings.math()).max(0.0)
            * self.world_unit_multiplier
    }

    /// Calculate the depth of field range for a safe region.
    /// This is used to increase the region which is considered to be in focus.
    fn get_depth_of_field(&self) -> DepthOfField {
        if self.settings.protect == 0.0 || self.internal_focus == 0.0 {
            return DepthOfField {
                x: self.internal_focus,
                y: self.internal_focus,
            };
        }
        if self.settings.camera_data.is_some() {
            return DepthOfField {
                x: self.internal_focus
                    - ((self.settings.protect * 0.5) * self.world_unit_multiplier),
                y: self.internal_focus
                    + ((self.settings.protect * 0.5) * self.world_unit_multiplier),
            };
        }
        let normalized_focus = 1.0 / self.internal_focus;
        DepthOfField {
            x: 1.0 / (normalized_focus + (normalized_focus * (self.settings.protect * 0.5))),
            y: 1.0 / (normalized_focus - (normalized_focus * (self.settings.protect * 0.5))),
        }
    }

    /// Convert input value to distance value.
    fn convert_value_to_distance(value: f32, math: Math) -> f32 {
        if value == 0.0 {
            return 9999.0;
        }
        match math {
            Math::Real => value,
            Math::OneDividedByZ => 1.0 / value,
        }
    }

    /// Apply the Circle of Confusion algorithm to the distance, to calculate the disc
    /// size of confusion which a real camera would also have.
    fn calculate_circle_of_confusion(&self, distance: f32) -> f32 {
        let Some(camera_data) = &self.settings.camera_data else {
            return 0.0;
        };
        if distance == 0.0 {
            return distance;
        }
        let mut calculated_focal_distance = self.internal_focus;
        let simple_point = DepthOfField { x: 0.0, y: 0.0 };
        if self.depth_of_field != simple_point
            && (distance > self.depth_of_field.x && distance < self.depth_of_field.y)
        {
            return 0.0;
        }
        if distance < self.depth_of_field.x {
            calculated_focal_distance = self.depth_of_field.x;
        } else if distance > self.internal_focus {
            calculated_focal_distance = self.depth_of_field.y;
        }

        calculated_focal_distance = calculated_focal_distance.min(self.hyperfocal_distance);
        let circle_of_confusion = ((calculated_focal_distance - distance)
            * libm::powf(camera_data.focal_length, 2.0))
            / (camera_data.f_stop
                * distance
                * (calculated_focal_distance - camera_data.focal_length));

        -(circle_of_confusion
            / (Self::length(camera_data.filmback.width, camera_data.filmback.height))
            * (Self::length(
                camera_data.resolution.width as f32,
                camera_data.resolution.height as f32,
            ) * 0.5))
    }

    /// Calculate the Circle of Confusion based on the manual values selected.
    /// This is not physically accurate, but gives a nice falloff.
    fn calculate_direct_map(&self, pixel_value: f32) -> f32 {
        // Exact equality is intended: only the focal value itself, not a near match, maps to zero.
        #[allow(
            clippy::float_cmp,
            reason = "exact focus match is the intended zero case, a tolerance would change results"
        )]
        let at_focus = self.internal_focus == pixel_value;
        if at_focus || (pixel_value > self.depth_of_field.x && pixel_value < self.depth_of_field.y)
        {
            return 0.0;
        }

        let converted_pixel_value = if pixel_value == 0.0 {
            0.0
        } else {
            1.0 / pixel_value
        };

        let mut calculated_value = 0.0;
        if self.internal_focus < pixel_value {
            let calculated_focus_point = if self.depth_of_field.y == 0.0 {
                0.0
            } else {
                1.0 / self.depth_of_field.y
            };
            calculated_value =
                (calculated_focus_point - converted_pixel_value) / calculated_focus_point;
        }
        if self.internal_focus > pixel_value {
            let calculated_focus_point = if self.depth_of_field.x == 0.0 {
                0.0
            } else {
                1.0 / self.depth_of_field.x
            };
            let calculated_near_field =
                (converted_pixel_value - calculated_focus_point) / calculated_focus_point;
            calculated_value =
                -calculated_near_field.min(self.settings.max_size / self.settings.size);
        }
        calculated_value * self.settings.size
    }
}

#[cfg(test)]
mod tests {
    use core::panic;

    use super::*;
    use crate::{CameraData, Filmback, Resolution};
    use approx::{self, assert_relative_eq, relative_eq};
    use serde_json::Value;

    #[derive(Debug)]
    struct TestResult {
        #[allow(
            dead_code,
            reason = "only shown through the Debug output of failing cases"
        )]
        pub settings: Settings,
        #[allow(
            dead_code,
            reason = "only shown through the Debug output of failing cases"
        )]
        pub coc: f32,
        pub result: f32,
        pub expected: f32,
    }

    impl TestResult {
        pub fn is_success(&self) -> bool {
            !relative_eq!(self.result, self.expected, epsilon = 1e-2)
        }
    }

    fn case_to_settings(case: &Value) -> Settings {
        let mut settings = Settings::default();
        if case["size"].is_f64() {
            settings.size = case["size"].as_f64().unwrap() as f32;
        }
        if case["max_size"].is_f64() {
            settings.max_size = case["max_size"].as_f64().unwrap() as f32;
        }
        if case["focal_plane"].is_f64() {
            settings.focal_plane = case["focal_plane"].as_f64().unwrap() as f32;
        }
        if let Some(camera_data) = case.get("camera_data") {
            let mut camera_instance = CameraData::default();
            if camera_data["focal_length"].is_f64() {
                camera_instance.focal_length = camera_data["focal_length"].as_f64().unwrap() as f32;
            }
            if camera_data["f_stop"].is_f64() {
                camera_instance.f_stop = camera_data["f_stop"].as_f64().unwrap() as f32;
            }

            settings.camera_data = Some(camera_instance);
        }
        if case["math"].is_string() {
            settings.math = match case["math"].as_str().unwrap() {
                "ONE_DIVIDED_BY_Z" => Math::OneDividedByZ,
                _ => Math::Real,
            }
            .into();
        }

        settings
    }

    #[test]
    fn test_calculation() {
        let cases = include_str!("../test/cases.json");

        let json: Vec<Value> = serde_json::from_str(cases).unwrap();

        let mut results = Vec::new();
        for case in &json {
            let settings = case_to_settings(&case["settings"]);
            let expected = case["expected"].as_f64().unwrap() as f32;
            let coc = case["coc"].as_f64().unwrap() as f32;
            let calculator = Calculator::new(settings);
            let result = calculator.calculate(coc);
            results.push(TestResult {
                settings,
                coc,
                result,
                expected,
            });
        }

        results.retain(TestResult::is_success);

        if results.is_empty() {
            return;
        }

        panic!("Test failed: {results:#?}")
    }

    fn direct_settings(focal_plane: f32, protect: f32) -> Settings {
        Settings {
            focal_plane,
            protect,
            math: Math::Real as i32,
            ..Settings::default()
        }
    }

    /// Camera with a 1730 mm sensor diagonal and matching resolution, so the
    /// Zeiss factor is 1 and the pixel scale is exactly 0.5 per millimetre.
    fn camera(f_stop: f32) -> CameraData {
        CameraData {
            focal_length: 50.0,
            f_stop,
            filmback: Filmback {
                width: 1730.0,
                height: 0.0,
            },
            resolution: Resolution {
                width: 1730,
                height: 0,
            },
            world_unit: WorldUnit::Mm as i32,
            ..CameraData::default()
        }
    }

    fn camera_settings(focal_plane: f32, protect: f32) -> Settings {
        Settings {
            camera_data: Some(camera(2.0)),
            ..direct_settings(focal_plane, protect)
        }
    }

    #[test]
    fn direct_map_is_zero_at_focus_plane() {
        let calculator = Calculator::new(direct_settings(10.0, 0.0));
        assert_eq!(calculator.calculate(10.0), 0.0);
    }

    #[test]
    fn direct_map_far_side_scales_with_inverse_distance() {
        let calculator = Calculator::new(direct_settings(10.0, 0.0));
        assert_relative_eq!(calculator.calculate(20.0), 2.5, epsilon = 1e-4);
    }

    #[test]
    fn direct_map_near_side_is_negative() {
        let calculator = Calculator::new(direct_settings(10.0, 0.0));
        assert_relative_eq!(calculator.calculate(5.0), -5.0, epsilon = 1e-4);
    }

    #[test]
    fn direct_map_clamps_to_max_size_on_both_sides() {
        let settings = Settings {
            max_size: 2.0,
            ..direct_settings(10.0, 0.0)
        };
        let calculator = Calculator::new(settings);
        assert_relative_eq!(calculator.calculate(20.0), 2.0, epsilon = 1e-4);
        assert_relative_eq!(calculator.calculate(5.0), -2.0, epsilon = 1e-4);
    }

    #[test]
    fn direct_map_zero_depth_is_treated_as_far_distance() {
        let calculator = Calculator::new(direct_settings(10.0, 0.0));
        assert_relative_eq!(calculator.calculate(0.0), 4.995, epsilon = 1e-3);
    }

    #[test]
    fn direct_map_protect_region_has_zero_coc_and_bounds_its_edges() {
        let calculator = Calculator::new(direct_settings(10.0, 1.0));
        assert_relative_eq!(calculator.depth_of_field.x, 20.0 / 3.0, epsilon = 1e-4);
        assert_relative_eq!(calculator.depth_of_field.y, 20.0, epsilon = 1e-4);
        assert_eq!(calculator.calculate(15.0), 0.0);
        assert_relative_eq!(calculator.calculate(6.0), -5.0 / 9.0, epsilon = 1e-4);
        assert_relative_eq!(calculator.calculate(25.0), 1.0, epsilon = 1e-4);
    }

    #[test]
    fn negative_focal_plane_is_clamped_to_zero_focus() {
        let calculator = Calculator::new(direct_settings(-5.0, 0.0));
        assert_eq!(calculator.internal_focus, 0.0);
    }

    #[test]
    fn world_unit_multiplier_matches_each_unit() {
        let expected = [
            (WorldUnit::Mm, 1.0),
            (WorldUnit::Cm, 10.0),
            (WorldUnit::Dm, 100.0),
            (WorldUnit::M, 1000.0),
            (WorldUnit::In, 25.4),
            (WorldUnit::Ft, 304.8),
        ];
        for (unit, multiplier) in expected {
            let mut data = camera(2.0);
            data.set_world_unit(unit);
            let settings = Settings {
                camera_data: Some(data),
                ..Settings::default()
            };
            let calculator = Calculator::new(settings);
            assert_relative_eq!(calculator.world_unit_multiplier, multiplier, epsilon = 1e-4);
        }
    }

    #[test]
    fn world_unit_multiplier_is_one_without_camera() {
        let calculator = Calculator::new(direct_settings(10.0, 0.0));
        assert_eq!(calculator.world_unit_multiplier, 1.0);
    }

    #[test]
    fn hyperfocal_distance_is_focal_squared_over_fstop_plus_focal() {
        let calculator = Calculator::new(camera_settings(1000.0, 0.0));
        assert_relative_eq!(calculator.hyperfocal_distance, 1300.0, epsilon = 1e-3);
    }

    #[test]
    fn hyperfocal_distance_uses_zeiss_factor_from_filmback() {
        let mut data = camera(2.0);
        data.filmback.width = 865.0;
        let settings = Settings {
            camera_data: Some(data),
            ..direct_settings(1000.0, 0.0)
        };
        let calculator = Calculator::new(settings);
        assert_relative_eq!(calculator.hyperfocal_distance, 2550.0, epsilon = 1e-3);
    }

    #[test]
    fn hyperfocal_distance_is_zero_without_camera() {
        let calculator = Calculator::new(direct_settings(10.0, 0.0));
        assert_eq!(calculator.hyperfocal_distance, 0.0);
    }

    #[test]
    fn camera_far_side_coc_matches_thin_lens_formula() {
        let calculator = Calculator::new(camera_settings(1000.0, 0.0));
        assert_relative_eq!(calculator.calculate(2000.0), 0.328_947_4, epsilon = 1e-4);
    }

    #[test]
    fn camera_near_side_coc_is_negative_of_same_magnitude() {
        let calculator = Calculator::new(camera_settings(1000.0, 0.0));
        assert_relative_eq!(calculator.calculate(500.0), -0.657_894_7, epsilon = 1e-4);
    }

    #[test]
    fn camera_coc_is_scaled_by_pixel_aspect() {
        let settings = Settings {
            pixel_aspect: 2.0,
            ..camera_settings(1000.0, 0.0)
        };
        let calculator = Calculator::new(settings);
        assert_relative_eq!(calculator.calculate(2000.0), 0.657_894_7, epsilon = 1e-4);
    }

    #[test]
    fn camera_coc_is_clamped_to_max_size() {
        let settings = Settings {
            max_size: 0.3,
            ..camera_settings(1000.0, 0.0)
        };
        let calculator = Calculator::new(settings);
        assert_relative_eq!(calculator.calculate(2000.0), 0.3, epsilon = 1e-6);
        assert_relative_eq!(calculator.calculate(500.0), -0.3, epsilon = 1e-6);
    }

    #[test]
    fn camera_focus_distance_is_capped_at_hyperfocal() {
        let calculator = Calculator::new(camera_settings(5000.0, 0.0));
        assert_relative_eq!(calculator.calculate(10_000.0), 0.435, epsilon = 1e-4);
    }

    #[test]
    fn camera_coc_is_zero_inside_protected_range() {
        let calculator = Calculator::new(camera_settings(1000.0, 200.0));
        assert_relative_eq!(calculator.depth_of_field.x, 900.0, epsilon = 1e-4);
        assert_relative_eq!(calculator.depth_of_field.y, 1100.0, epsilon = 1e-4);
        assert_eq!(calculator.calculate(1000.0), 0.0);
        assert_eq!(calculator.calculate(1050.0), 0.0);
        assert_relative_eq!(calculator.calculate(800.0), -0.091_911_76, epsilon = 1e-4);
    }

    #[test]
    fn camera_coc_at_protected_range_edge_is_zero() {
        let calculator = Calculator::new(camera_settings(1000.0, 200.0));
        assert_relative_eq!(calculator.calculate(1100.0), 0.0, epsilon = 1e-6);
    }

    #[test]
    fn camera_world_unit_scales_input_depth_and_focus() {
        let mut data = camera(2.0);
        data.set_world_unit(WorldUnit::M);
        let settings = Settings {
            camera_data: Some(data),
            ..direct_settings(1.0, 0.0)
        };
        let calculator = Calculator::new(settings);
        assert_relative_eq!(calculator.calculate(2.0), 0.328_947_4, epsilon = 1e-4);
    }

    #[test]
    fn inverse_depth_input_is_converted_to_distance() {
        let settings = Settings {
            math: Math::OneDividedByZ as i32,
            ..camera_settings(0.001, 0.0)
        };
        let calculator = Calculator::new(settings);
        assert_relative_eq!(calculator.calculate(0.0005), 0.328_947_4, epsilon = 1e-4);
    }

    #[test]
    fn camera_zero_depth_is_treated_as_far_distance() {
        let calculator = Calculator::new(camera_settings(1000.0, 0.0));
        assert_relative_eq!(calculator.calculate(0.0), 0.5921, epsilon = 1e-3);
        assert_eq!(calculator.calculate(0.0), calculator.calculate(9999.0));
    }

    #[test]
    fn camera_zero_focus_plane_focuses_at_infinity() {
        let calculator = Calculator::new(camera_settings(0.0, 0.0));
        assert_eq!(calculator.internal_focus, 9999.0);
        assert_relative_eq!(calculator.calculate(2000.0), 0.175, epsilon = 1e-4);
    }
}
