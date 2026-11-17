// The kimchi example project (examples/kimchi.picle.json): valid, 24 frames, and the turntable turns.

use picle::eval::Evaluator;
use picle::project;
use std::path::Path;

#[test]
fn kimchi_loads_and_turns() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/kimchi.picle.json");
    let l = project::load(&path).unwrap();
    assert!(l.errors.is_empty(), "{:?}", l.errors);
    let p = l.project.unwrap();
    assert_eq!((p.settings.width, p.settings.height, p.settings.frames), (64, 64, 24));
    let mut ev = Evaluator::new();
    let mut at = |f| ev.eval(&p, path.parent().unwrap(), "out", f).unwrap().into_surface().unwrap();
    let (a, b) = (at(0), at(12));
    assert!(a.data.chunks_exact(4).any(|px| px[3] == 255), "frame 0 is empty");
    assert_ne!(a, b, "frame 12 equals frame 0: the turntable does not turn");
}
