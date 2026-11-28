use std::time::Duration;

use freya::prelude::*;
use freya_testing::prelude::*;

fn setup(virtualized: bool, platform: TargetPlatform) -> (TestingRunner, ScrollController) {
    let (mut test, controller) = TestingRunner::new(
        move || {
            let controller = use_consume::<ScrollController>();
            if virtualized {
                VirtualScrollView::new_controlled(
                    |item, _| rect().key(item.index).height(Size::px(50.)).into(),
                    controller,
                )
                .length(100usize)
                .item_size(50.)
                .into_element()
            } else {
                ScrollView::new_controlled(controller)
                    .child(rect().height(Size::px(5000.)))
                    .into_element()
            }
        },
        (500., 500.).into(),
        |runner| {
            runner.provide_root_context(|| platform);
            runner.provide_root_context(|| ScrollController::new(0., 0.))
        },
        1.,
    );
    test.sync_and_update();
    (test, controller)
}

fn wheel(test: &mut TestingRunner, phase: TouchPhase, delta: f64) {
    test.send_event(PlatformEvent::Wheel {
        name: WheelEventName::Wheel,
        scroll: (0., delta).into(),
        cursor: (5., 5.).into(),
        source: WheelSource::Pixel,
        phase,
    });
    test.sync_and_update();
}

fn position(test: &TestingRunner, controller: ScrollController) -> (f64, f64) {
    test.run_in(|| {
        let (horizontal, vertical): (f64, f64) = controller.into();
        let rendered = controller.animated_position((horizontal, vertical).into());
        (rendered.y, vertical)
    })
}

#[test]
fn linux_scroll_gesture_smoothing_momentum_and_interruption() {
    for virtualized in [false, true] {
        let (mut test, controller) = setup(virtualized, TargetPlatform::Linux);
        wheel(&mut test, TouchPhase::Started, -40.);
        let (rendered, target) = position(&test, controller);
        assert_eq!(rendered, 0.);
        assert_eq!(target, 40.);

        test.poll(Duration::from_millis(8), Duration::from_millis(32));
        wheel(&mut test, TouchPhase::Moved, -40.);
        test.poll(Duration::from_millis(8), Duration::from_millis(32));
        wheel(&mut test, TouchPhase::Moved, -40.);
        let (rendered, target) = position(&test, controller);
        assert!(rendered > 0. && rendered < target);
        wheel(&mut test, TouchPhase::Ended, 0.);
        assert!(position(&test, controller).1 > target);

        test.poll(Duration::from_millis(8), Duration::from_millis(32));
        wheel(&mut test, TouchPhase::Started, 0.);
        wheel(&mut test, TouchPhase::Cancelled, 0.);
        let stopped = position(&test, controller);
        test.poll(Duration::from_millis(8), Duration::from_millis(150));
        assert_eq!(position(&test, controller), stopped);
    }
}

#[test]
fn linux_scroll_gesture_pause_does_not_fling() {
    for virtualized in [false, true] {
        let (mut test, controller) = setup(virtualized, TargetPlatform::Linux);
        wheel(&mut test, TouchPhase::Started, -40.);
        test.poll(Duration::from_millis(8), Duration::from_millis(32));
        wheel(&mut test, TouchPhase::Moved, -40.);
        test.poll(Duration::from_millis(8), Duration::from_millis(150));
        let target = position(&test, controller).1;
        wheel(&mut test, TouchPhase::Ended, 0.);
        assert_eq!(position(&test, controller).1, target);
    }
}

#[test]
fn android_touch_fling() {
    for virtualized in [false, true] {
        for direction in [-1., 1.] {
            let (mut test, mut controller) = setup(virtualized, TargetPlatform::Android);
            controller.scroll_to_y(2250.);
            test.sync_and_update();

            test.press_touch((100., 250.));
            for step in 1..=3 {
                test.move_touch((100., 250. + direction * step as f64 * 10.));
            }
            let before_release = position(&test, controller).0;
            test.release_touch((100., 250. + direction * 30.));

            let (rendered, target) = position(&test, controller);
            assert_eq!(rendered, before_release);
            assert!(target > 0. && target < 4500.);
            assert!((target - rendered) * direction < 0.);

            test.poll_n(Duration::from_millis(8), 4);
            let animated = position(&test, controller).0;
            assert!((animated - rendered) * direction < 0.);
            assert!((target - animated) * direction < 0.);
        }
    }
}

#[test]
fn other_platforms_keep_direct_pixel_scrolling() {
    for platform in [TargetPlatform::MacOs, TargetPlatform::Windows] {
        for virtualized in [false, true] {
            let (mut test, controller) = setup(virtualized, platform);
            wheel(&mut test, TouchPhase::Started, -40.);
            assert_eq!(position(&test, controller), (40., 40.));
            wheel(&mut test, TouchPhase::Moved, -40.);
            wheel(&mut test, TouchPhase::Ended, 0.);
            assert_eq!(position(&test, controller), (80., 80.));
        }
    }
}

fn touch(test: &mut TestingRunner, name: TouchEventName, finger_id: u64, location: (f64, f64)) {
    let phase = match name {
        TouchEventName::TouchStart => TouchPhase::Started,
        TouchEventName::TouchMove => TouchPhase::Moved,
        TouchEventName::TouchEnd => TouchPhase::Ended,
        TouchEventName::TouchCancel => TouchPhase::Cancelled,
    };
    test.send_event(PlatformEvent::Touch {
        name,
        location: location.into(),
        finger_id,
        phase,
        force: None,
    });
}

#[test]
fn touch_drag_follows_its_finger() {
    for virtualized in [false, true] {
        let (mut test, controller) = setup(virtualized, TargetPlatform::Android);
        test.press_touch((100., 400.));
        test.move_touch((100., 350.));
        let dragged = position(&test, controller).1;
        assert!(dragged > 0.);

        // A second finger neither moves the content nor ends the drag.
        touch(&mut test, TouchEventName::TouchStart, 1, (100., 300.));
        test.sync_and_update();
        touch(&mut test, TouchEventName::TouchMove, 1, (100., 200.));
        test.sync_and_update();
        assert_eq!(position(&test, controller), (dragged, dragged));
        touch(&mut test, TouchEventName::TouchEnd, 1, (100., 200.));
        test.sync_and_update();
        assert_eq!(position(&test, controller), (dragged, dragged));

        test.move_touch((100., 300.));
        assert_eq!(position(&test, controller).1, dragged + 50.);

        // The same finger pressing again restarts the drag, a cancelled touch has no release.
        test.press_touch((100., 400.));
        test.move_touch((100., 350.));
        let restarted = position(&test, controller).1;
        test.move_touch((100., 330.));
        assert_eq!(position(&test, controller).1, restarted + 20.);

        // After a cancel, another finger can drag.
        touch(&mut test, TouchEventName::TouchCancel, 0, (100., 330.));
        test.sync_and_update();
        touch(&mut test, TouchEventName::TouchStart, 1, (100., 400.));
        test.sync_and_update();
        touch(&mut test, TouchEventName::TouchMove, 1, (100., 350.));
        test.sync_and_update();
        let other = position(&test, controller).1;
        touch(&mut test, TouchEventName::TouchMove, 1, (100., 330.));
        test.sync_and_update();
        assert_eq!(position(&test, controller).1, other + 20.);
        touch(&mut test, TouchEventName::TouchEnd, 1, (100., 330.));
        test.sync_and_update();
    }
}

#[test]
fn touch_drag_ends_when_its_view_is_removed() {
    let (mut test, (mut show_inner, page)) = TestingRunner::new(
        || {
            let (show_inner, page) = use_consume::<(State<bool>, ScrollController)>();
            ScrollView::new_controlled(page)
                .child(rect().height(Size::px(100.)))
                .maybe_child(show_inner().then(|| {
                    ScrollView::new()
                        .horizontal()
                        .height(Size::px(100.))
                        .child(rect().width(Size::px(1500.)).height(Size::px(100.)))
                }))
                .child(rect().height(Size::px(5000.)))
                .into_element()
        },
        (500., 500.).into(),
        |runner| {
            runner.provide_root_context(|| TargetPlatform::Android);
            runner.provide_root_context(|| (State::create(true), ScrollController::new(0., 0.)))
        },
        1.,
    );
    test.sync_and_update();

    test.press_touch((100., 150.));
    test.move_touch((50., 150.));
    *show_inner.write() = false;
    test.sync_and_update();
    test.release_touch((100., 150.));

    touch(&mut test, TouchEventName::TouchStart, 1, (100., 400.));
    test.sync_and_update();
    touch(&mut test, TouchEventName::TouchMove, 1, (100., 350.));
    test.sync_and_update();
    let dragged = position(&test, page).1;
    touch(&mut test, TouchEventName::TouchMove, 1, (100., 330.));
    test.sync_and_update();
    assert_eq!(position(&test, page).1, dragged + 20.);
    touch(&mut test, TouchEventName::TouchEnd, 1, (100., 330.));
    test.sync_and_update();
}
