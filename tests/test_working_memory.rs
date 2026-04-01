use token_compress_engine::{
    algorithms::working_memory::{ContextFrame, WorkingMemory},
    types::{Mode, SlotSource, WmSlot, AGGRESSIVE_WM_CAPACITY, GENTLE_WM_CAPACITY},
};

fn make_slot(content: &str, salience: f32) -> WmSlot {
    WmSlot {
        content: content.into(),
        salience,
        source: SlotSource::Delta,
    }
}

#[test]
fn test_gentle_hard_cap_at_3() {
    let mut wm = WorkingMemory::new(&Mode::Gentle);
    wm.load(vec![
        make_slot("task", 0.9),
        make_slot("deliv", 0.8),
        make_slot("ctx1", 0.7),
        make_slot("ctx2", 0.6), // this should be displaced or not loaded
        make_slot("ctx3", 0.5),
    ]);
    let frame = wm.get_context_frame();
    assert_eq!(
        frame.slots_used, GENTLE_WM_CAPACITY,
        "Gentle WM must hard-cap at 3"
    );
}

#[test]
fn test_aggressive_hard_cap_at_7() {
    let mut wm = WorkingMemory::new(&Mode::Aggressive);
    let items: Vec<WmSlot> = (0..10)
        .map(|i| make_slot(&i.to_string(), i as f32 * 0.1))
        .collect();
    wm.load(items);
    let frame = wm.get_context_frame();
    assert_eq!(
        frame.slots_used, AGGRESSIVE_WM_CAPACITY,
        "Aggressive WM must hard-cap at 7"
    );
}

#[test]
fn test_displacement_keeps_highest_salience() {
    let mut wm = WorkingMemory::new(&Mode::Gentle);
    wm.load(vec![
        make_slot("low", 0.2),
        make_slot("mid", 0.5),
        make_slot("high", 0.9),
        make_slot("new", 0.8), // should displace "low"
    ]);
    let frame = wm.get_context_frame();
    assert!(
        !frame.items.iter().any(|s| s.content == "low"),
        "Lowest salience slot must be displaced"
    );
    assert!(frame.items.iter().any(|s| s.content == "high"));
    assert!(frame.items.iter().any(|s| s.content == "new"));
}

#[test]
fn test_fidelity_floor_gentle() {
    let mut wm = WorkingMemory::new(&Mode::Gentle);
    wm.load(vec![
        make_slot("a", 0.9),
        make_slot("b", 0.8),
        make_slot("c", 0.7),
    ]);
    let frame = wm.get_context_frame();
    assert!(
        frame.fidelity >= 0.65,
        "Gentle fidelity floor must be at least 65%"
    );
    assert!(
        frame.fidelity <= 0.80,
        "Gentle fidelity ceiling must be at most 80%"
    );
}

#[test]
fn test_fidelity_ceiling_aggressive() {
    let mut wm = WorkingMemory::new(&Mode::Aggressive);
    wm.load(vec![
        make_slot("a", 0.9),
        make_slot("b", 0.8),
        make_slot("c", 0.7),
        make_slot("d", 0.6),
        make_slot("e", 0.5),
        make_slot("f", 0.4),
        make_slot("g", 0.3),
    ]);
    let frame = wm.get_context_frame();
    // With 7/7 slots used, utilization = 1.0, fidelity = 1.0 * 0.95 = 0.95
    assert!((frame.fidelity - 0.95).abs() < 0.01);
}

#[test]
fn test_context_frame_contents() {
    let mut wm = WorkingMemory::new(&Mode::Gentle);
    wm.load(vec![
        make_slot("first", 0.9),
        make_slot("second", 0.8),
        make_slot("third", 0.7),
    ]);
    let frame = wm.get_context_frame();

    assert_eq!(frame.slots_used, 3);
    assert_eq!(frame.capacity, GENTLE_WM_CAPACITY);
    assert!((frame.utilisation - 1.0).abs() < 0.01); // 3/3 = 1.0
    assert_eq!(frame.items.len(), 3);

    // Items should be sorted by salience descending
    assert_eq!(frame.items[0].content, "first");
    assert_eq!(frame.items[1].content, "second");
    assert_eq!(frame.items[2].content, "third");
}
