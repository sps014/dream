use super::{CACHE_ENTRIES, TextEngine};

fn shape(engine: &mut TextEngine, text: &str, width: f32) -> std::sync::Arc<super::ShapedText> {
    engine
        .layout(text, "Noto Sans", 20.0, 400.0, width, 0)
        .unwrap()
}

#[test]
fn metrics_report_lines_and_baseline() {
    let mut engine = TextEngine::new();
    let single = shape(&mut engine, "Hello world", 0.0);
    let [width, height, lines, baseline] = single.metrics();
    assert!(width > 40.0);
    assert!(height >= 20.0);
    assert_eq!(lines, 1.0);
    assert!(baseline > 0.0 && baseline < height);

    let wrapped = shape(&mut engine, "Hello world, wrapped paragraph", 60.0);
    assert!(wrapped.line_count() > 1);
    assert!(wrapped.height > single.height);

    let empty = shape(&mut engine, "", 0.0);
    assert!(empty.line_count() <= 1);
}

#[test]
fn hit_testing_uses_utf16_offsets() {
    let mut engine = TextEngine::new();
    let text = shape(&mut engine, "a😀b", 0.0);
    assert_eq!(text.index_at(-10.0, 5.0), 0);
    assert_eq!(text.index_at(10_000.0, 5.0), 4);

    let before = text.caret(0);
    let after_emoji = text.caret(3);
    assert!(after_emoji[0] > before[0]);
    assert_eq!(before[2], 1.0);
    assert!(before[3] > 0.0);
    // An offset inside the surrogate pair snaps to the emoji's start.
    assert_eq!(text.caret(2), text.caret(1));
    assert_eq!(text.caret(99), text.caret(4));

    let selection = text.selection(1, 3);
    assert_eq!(selection.len(), 4);
    assert!(selection[2] > 0.0);
    assert!(text.selection(2, 2).is_empty() || text.selection(2, 2)[2] == 0.0);
}

#[test]
fn selection_spans_wrapped_lines() {
    let mut engine = TextEngine::new();
    let text = shape(&mut engine, "alpha beta gamma delta", 50.0);
    assert!(text.line_count() > 1);
    assert!(text.selection(0, 22).len() >= 8);
}

#[test]
fn cache_evicts_least_recently_used() {
    let mut engine = TextEngine::new();
    let first = shape(&mut engine, "keep", 0.0);
    for index in 0..CACHE_ENTRIES {
        shape(&mut engine, &format!("entry {index}"), 0.0);
        if index % 16 == 0 {
            assert!(std::sync::Arc::ptr_eq(
                &first,
                &shape(&mut engine, "keep", 0.0)
            ));
        }
    }
    assert_eq!(engine.cache.len(), CACHE_ENTRIES);
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &shape(&mut engine, "keep", 0.0)
    ));
    let total: usize = engine.cache.iter().map(|(_, (_, bytes))| bytes).sum();
    assert_eq!(total, engine.cache_bytes);
    assert!(!engine.cache.iter().any(|((text, ..), _)| text == "entry 0"));
}
