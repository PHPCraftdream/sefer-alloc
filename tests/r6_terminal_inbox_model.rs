//! Deterministic state-machine oracles for the Stage 2A offset protocol.
//! The production primitive is unit-tested in `remote_inbox/tests.rs`;
//! these traces cover the stale-numeric-offset ABA and a broken publish order.

const EMPTY: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct Node {
    next: u32,
    class: u32,
}

struct Model {
    head: u32,
    nodes: [Node; 3],
}

impl Model {
    fn new() -> Self {
        Self {
            head: EMPTY,
            nodes: [Node {
                next: EMPTY,
                class: EMPTY,
            }; 3],
        }
    }

    fn cas(&mut self, expected: u32, own: u32) -> Result<(), u32> {
        if self.head == expected {
            self.head = own;
            Ok(())
        } else {
            Err(self.head)
        }
    }

    fn cut(&mut self) -> u32 {
        std::mem::replace(&mut self.head, EMPTY)
    }

    fn drain(&self, mut offset: u32) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        while offset != EMPTY {
            let node = self.nodes[offset as usize];
            out.push((offset, node.class));
            offset = node.next;
            assert!(out.len() <= self.nodes.len(), "cycle in detached chain");
        }
        out
    }
}

#[test]
fn paused_before_cas_is_excluded_and_offset_aba_joins_current_instance() {
    let mut m = Model::new();
    m.nodes[0] = Node {
        next: EMPTY,
        class: 10,
    };
    assert_eq!(m.cas(EMPTY, 0), Ok(()));

    // B captures only the numeric predecessor offset, never reads its node.
    let b_expected = m.head;
    let before = m.cut();
    assert_eq!(m.drain(before), vec![(0, 10)]);
    assert_eq!(m.cut(), EMPTY);

    // Owner reclaims and reissues offset 0 inside the same live segment.
    m.nodes[0] = Node {
        next: EMPTY,
        class: 11,
    };
    assert_eq!(m.cas(EMPTY, 0), Ok(()));
    assert_eq!(b_expected, m.head); // a -> EMPTY -> a
    m.nodes[1] = Node {
        next: b_expected,
        class: 20,
    };
    assert_eq!(m.cas(b_expected, 1), Ok(()));
    let cut = m.cut();
    assert_eq!(m.drain(cut), vec![(1, 20), (0, 11)]);
}

#[test]
fn failed_strong_cas_rewrites_only_private_successor() {
    let mut m = Model::new();
    let expected = m.head;
    m.nodes[0] = Node {
        next: EMPTY,
        class: 1,
    };
    assert_eq!(m.cas(EMPTY, 0), Ok(()));
    m.nodes[1].class = 2;
    m.nodes[1].next = expected;
    let current = m.cas(expected, 1).expect_err("head changed");
    m.nodes[1].next = current;
    assert_eq!(m.cas(current, 1), Ok(()));
    let cut = m.cut();
    assert_eq!(m.drain(cut), vec![(1, 2), (0, 1)]);
}

#[test]
fn negative_control_head_before_payload_is_observed_as_uninitialized() {
    let mut broken = Model::new();
    assert_eq!(broken.cas(EMPTY, 0), Ok(())); // intentionally wrong order
    let cut = broken.cut();
    let observed = broken.drain(cut);
    assert_ne!(observed, vec![(0, 7)]); // desired oracle rejects broken order
    broken.nodes[0].class = 7; // too late: owner already observed it
    assert_eq!(broken.nodes[0].class, 7);
    assert_eq!(observed, vec![(0, EMPTY)]);
}
