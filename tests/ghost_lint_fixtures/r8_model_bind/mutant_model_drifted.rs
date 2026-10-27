// expect: prod rule requires ["Release", "Relaxed"]
// ghost-bind: fixture/r1-lease-drop-release
const DROP_ORDERING: (Ordering, Ordering) = (Ordering::Relaxed, Ordering::Relaxed);
