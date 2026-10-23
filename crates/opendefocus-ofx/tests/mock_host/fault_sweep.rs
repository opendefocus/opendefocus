//! Failure sweeps: each host call an action makes is failed in turn, one run per call. The plugin
//! must either tolerate that failure (the status is OK) or pass the injected status on, so the
//! error branch of every host call is taken for real.

use super::*;

/// Status injected into one host call. No plugin code raises it itself, so seeing it means the
/// plugin passed the host's failure through.
const INJECTED: OfxStatus = OFX_STAT_ERR_MEMORY;

/// A session that has loaded the plugin.
fn loaded() -> Session {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    session
}

/// A loaded session that has described the plugin and declared its filter context.
fn described() -> Session {
    let session = loaded();
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    session
}

/// A described session with a connected Nuke camera.
fn described_with_camera() -> Session {
    let session = Session::with_nuke_camera(MockCamera {
        connected: true,
        focal: 50.0,
        haperture: 36.0,
        vaperture: 24.0,
    });
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    session
}

/// A described session with an instance created on it.
fn with_instance() -> Session {
    let session = described();
    assert_eq!(session.create_instance(), OFX_STAT_OK);
    session
}

/// Maps a settings read to the status a host action would report.
fn settings_status(result: &opendefocus_ofx::suites::Result<Settings>) -> OfxStatus {
    match result {
        Ok(_) => OFX_STAT_OK,
        Err(error) => error.0,
    }
}

/// Runs `run` once clean to count its host calls, then once per call on a fresh session from
/// `make`, with that call failing. Each run must return OK (the call was tolerated), the injected
/// status, or FAILED with the injected status in its message. Returns how many runs passed the
/// injected status on.
fn sweep(make: impl Fn() -> Session, run: impl Fn(&Session) -> OfxStatus) -> usize {
    let calls = {
        let clean = make();
        clean.arm_fault(None);
        assert_eq!(run(&clean), OFX_STAT_OK, "the clean run must succeed");
        clean.host_calls()
    };
    assert!(calls > 0, "the action makes host calls");
    let mut passed_on = 0;
    for index in 0..calls {
        let session = make();
        session.arm_fault(Some((index, INJECTED)));
        match run(&session) {
            OFX_STAT_OK => {}
            status if status == INJECTED => passed_on += 1,
            OFX_STAT_FAILED => {
                let text = message_text(&session).unwrap_or_default();
                assert!(
                    text.contains(&INJECTED.to_string()),
                    "call {index}: failure without the injected status: {text}"
                );
                passed_on += 1;
            }
            other => panic!("call {index}: unexpected status {other}"),
        }
    }
    passed_on
}

/// Every host call in `Describe` either is tolerated (it only sets a hint) or fails the action.
#[test]
fn describe_passes_on_each_failed_host_call() {
    let passed_on = sweep(loaded, Session::describe);
    assert!(passed_on > 0);
}

/// Every host call in `DescribeInContext` (clips, parameters and pages) passes its failure on.
#[test]
fn describe_in_context_passes_on_each_failed_host_call() {
    let passed_on = sweep(
        || {
            let session = loaded();
            assert_eq!(session.describe(), OFX_STAT_OK);
            session
        },
        |s| s.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
    );
    assert!(passed_on > 0);
}

/// The camera input's host calls in `DescribeInContext` pass their failures on as well.
#[test]
fn describe_in_context_with_camera_passes_on_each_failed_host_call() {
    let passed_on = sweep(
        || {
            let session = Session::with_nuke_camera(MockCamera {
                connected: true,
                focal: 50.0,
                haperture: 36.0,
                vaperture: 24.0,
            });
            assert_eq!(session.load(), OFX_STAT_OK);
            assert_eq!(session.describe(), OFX_STAT_OK);
            session
        },
        |s| s.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
    );
    assert!(passed_on > 0);
}

/// Reading the settings with a connected camera passes on each failed camera or parameter read.
#[test]
fn settings_with_camera_pass_on_each_failed_host_call() {
    let passed_on = sweep(described_with_camera, |s| settings_status(&s.settings(0.0)));
    assert!(passed_on > 0);
}

/// Every host call in `GetClipPreferences` passes its failure on.
#[test]
fn clip_preferences_pass_on_each_failed_host_call() {
    let passed_on = sweep(loaded, |s| s.clip_preferences().0);
    assert!(passed_on > 0);
}

/// Every host call in `CreateInstance` passes its failure on; the enabled-state sync is best effort.
#[test]
fn create_instance_passes_on_each_failed_host_call() {
    let passed_on = sweep(described, Session::create_instance);
    assert!(passed_on > 0);
}

/// Every host call in `InstanceChanged` for a parameter is tolerated: the Enable sync is cosmetic, so
/// a failure there must not make the host see the edit as failed.
#[test]
fn instance_changed_tolerates_each_failed_host_call() {
    let passed_on = sweep(with_instance, |s| {
        s.action(
            OFX_ACTION_INSTANCE_CHANGED,
            s.effect(),
            change_args(&OFX_TYPE_PARAMETER.to_string_lossy()),
        )
    });
    assert_eq!(passed_on, 0);
}

/// Every host call in a Render passes its failure on, including the GPU-free checks, the image
/// releases and the window clearing. Needs a WGPU adapter, because a render that no call fails
/// draws the window.
#[test]
#[ignore = "needs a WGPU adapter"]
fn render_passes_on_each_failed_host_call() {
    let passed_on = sweep(render_fixture, |s| s.render([0, 0, 16, 16], 0.0));
    assert!(passed_on > 0);
}

/// Every host call in a region-of-interest request passes its failure on. Needs a WGPU adapter,
/// because a request that reaches the renderer creates it.
#[test]
#[ignore = "needs a WGPU adapter"]
fn regions_of_interest_pass_on_each_failed_host_call() {
    let passed_on = sweep(render_fixture, |s| {
        s.regions_of_interest(Some([0.0, 0.0, 16.0, 16.0]), 0.0).0
    });
    assert!(passed_on > 0);
}

/// A ready render session with a 16 by 16 Source and Output, and an instance.
fn render_fixture() -> Session {
    let session = ready_session();
    session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session.set_clip_image("Output", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session
}
