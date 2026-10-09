use eframe::egui::{self, Event, Key, MouseWheelUnit};

/// Apply after zoom has consumed Ctrl+wheel; covers wheels and trackpad scrolling.
pub fn accelerate_scroll(ctx: &egui::Context) {
    ctx.input_mut(|input| {
        input.raw_scroll_delta *= 2.0;
        input.smooth_scroll_delta *= 2.0;
    });
}

/// Use the same persisted scale as Appearance. Wheel zoom never scrolls the chat.
pub fn handle(ctx: &egui::Context, scale: &mut f32) {
    ctx.options_mut(|options| options.zoom_with_keyboard = false);
    let (steps, reset) = ctx.input_mut(|input| {
        let mut steps = 0.0_f32;
        let mut wheel = false;
        input.events.retain(|event| {
            if let Event::MouseWheel { unit, delta, modifiers } = event {
                if modifiers.ctrl {
                    steps += delta.y / match unit {
                        MouseWheelUnit::Point => 120.0,
                        MouseWheelUnit::Line => 3.0,
                        MouseWheelUnit::Page => 1.0,
                    };
                    wheel = true;
                    return false;
                }
            }
            true
        });
        if wheel {
            input.raw_scroll_delta = egui::Vec2::ZERO;
            input.smooth_scroll_delta = egui::Vec2::ZERO;
        }
        let reset = input.consume_key(egui::Modifiers::CTRL, Key::Num0);
        if input.consume_key(egui::Modifiers::CTRL, Key::Plus)
            || input.consume_key(egui::Modifiers::CTRL, Key::Equals) { steps += 1.0; }
        if input.consume_key(egui::Modifiers::CTRL, Key::Minus) { steps -= 1.0; }
        (steps, reset)
    });
    if reset { *scale = 1.0; }
    else if steps.is_finite() && steps != 0.0 {
        *scale = (*scale * 1.1_f32.powf(steps.clamp(-10.0, 10.0))).clamp(0.75, 1.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrolling_is_twice_as_fast_without_changing_ctrl_wheel_zoom() {
        let scroll = |unit, ctrl, faster| {
            let ctx = egui::Context::default();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
            let mut point = egui::Pos2::ZERO;
            let mut offset = 0.0;
            let mut scale = 1.0;
            let _ = ctx.run(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let area = egui::ScrollArea::vertical().max_height(100.0).vertical_scroll_offset(500.0)
                        .show(ui, |ui| ui.set_min_size(egui::vec2(200.0, 2000.0)));
                    point = area.inner_rect.center();
                });
            });
            let wheel = Event::MouseWheel { unit, delta: egui::vec2(0.0, 1.0), modifiers: egui::Modifiers { ctrl, ..Default::default() } };
            let _ = ctx.run(egui::RawInput { screen_rect: Some(screen), events: vec![Event::PointerMoved(point), wheel], ..Default::default() }, |ctx| {
                handle(ctx, &mut scale);
                if faster { accelerate_scroll(ctx); }
                egui::CentralPanel::default().show(ctx, |ui| {
                    offset = egui::ScrollArea::vertical().max_height(100.0)
                        .show(ui, |ui| ui.set_min_size(egui::vec2(200.0, 2000.0))).state.offset.y;
                });
            });
            (500.0 - offset, scale)
        };
        for unit in [MouseWheelUnit::Point, MouseWheelUnit::Line, MouseWheelUnit::Page] {
            let (normal, _) = scroll(unit, false, false);
            let (faster, scale) = scroll(unit, false, true);
            assert!(normal > 0.0);
            assert!((faster - normal * 2.0).abs() < 0.01);
            assert_eq!(scale, 1.0);
            let (movement, scale) = scroll(unit, true, true);
            assert_eq!(movement, 0.0);
            assert!(scale > 1.0);
        }
    }
    #[test]
    fn ctrl_wheel_zooms_in_out_and_consumes_scroll_but_plain_wheel_does_not() {
        let ctx = egui::Context::default();
        let mut scale = 1.0;
        for (ctrl, delta, expected) in [(true, 3.0, 1.1), (true, -3.0, 1.0), (false, 3.0, 1.0), (true, 1000.0, 1.5), (true, -1000.0, 0.75)] {
            let event = Event::MouseWheel { unit: MouseWheelUnit::Line, delta: egui::vec2(0.0, delta), modifiers: egui::Modifiers { ctrl, ..Default::default() } };
            let _ = ctx.run(egui::RawInput { events: vec![event], ..Default::default() }, |ctx| {
                handle(ctx, &mut scale);
                assert!((scale - expected).abs() < 0.001);
                assert_eq!(ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::MouseWheel { .. }))), !ctrl);
                if ctrl { assert_eq!(ctx.input(|i| i.smooth_scroll_delta), egui::Vec2::ZERO); }
            });
        }
        let event = Event::Key { key: Key::Num0, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::CTRL };
        let _ = ctx.run(egui::RawInput { events: vec![event], ..Default::default() }, |ctx| handle(ctx, &mut scale));
        assert_eq!(scale, 1.0);
    }
}
