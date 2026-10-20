// Floor probe: larson/mstress T=1 with sefer / mimalloc / trivial bump floor.
// The bump floor is a GlobalAlloc over one leaked 64 MiB arena:
// alloc = align the cursor to Layout.align and bump it; dealloc = no-op.
// Two ZST variants control inlining of the bump body independently:
//   Bump<false> -> #[inline(always)] body (LLVM inlines into the harness)
//   Bump<true>  -> #[inline(never)]  body (real call per alloc)
// so the harness overhead is identical across all four allocators and the
// only difference is the allocator path itself.
use malloc_bench_rs::{run, Config, Workload};
use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicUsize, Ordering};

const ARENA: usize = 64 * 1024 * 1024;
static BUF: AtomicUsize = AtomicUsize::new(0);
static CUR: AtomicUsize = AtomicUsize::new(0);

fn arena_base() -> usize {
    let b = BUF.load(Ordering::Relaxed);
    if b != 0 {
        return b;
    }
    let mut v: Vec<u8> = Vec::with_capacity(ARENA);
    let p = v.as_mut_ptr() as usize;
    std::mem::forget(v); // leak: bump arena lives for the whole process
    BUF.store(p, Ordering::Relaxed);
    p
}

// Shared bump body. `never` toggles only the inlining attribute of the
// trampoline that the GlobalAlloc impl calls.
#[inline(always)]
fn bump_body_inl(l: Layout) -> *mut u8 {
    let base = arena_base();
    let al = l.align();
    let sz = l.size();
    loop {
        let cur = CUR.load(Ordering::Relaxed);
        let p = (base + cur + al - 1) & !(al - 1);
        let end = p + sz;
        if end > base + ARENA {
            return std::ptr::null_mut();
        }
        if CUR
            .compare_exchange(cur, end - base, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return p as *mut u8;
        }
    }
}

#[inline(never)]
fn bump_body_nev(l: Layout) -> *mut u8 {
    bump_body_inl(l)
}

struct Bump<const NEVER: bool>;
unsafe impl<const NEVER: bool> GlobalAlloc for Bump<NEVER> {
    #[inline]
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if NEVER {
            bump_body_nev(l)
        } else {
            bump_body_inl(l)
        }
    }
    #[inline]
    unsafe fn dealloc(&self, _p: *mut u8, _l: Layout) {}
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let alloc = a.get(1).map(String::as_str).unwrap_or("sefer");
    let wl = match a.get(2).map(String::as_str).unwrap_or("larson") {
        "mstress" => Workload::Mstress,
        _ => Workload::Larson,
    };
    let steps: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let cfg = Config { threads: 1, steps_per_thread: steps, working_set: 768, mstress_blocks: 512 };
    let r = match alloc {
        "mimalloc" => run(wl, &cfg, || mimalloc::MiMalloc),
        "bumpa" => run(wl, &cfg, || Bump::<false>),
        "bumpn" => run(wl, &cfg, || Bump::<true>),
        _ => run(wl, &cfg, sefer_alloc::SeferAlloc::new),
    };
    println!("{alloc} {r:.0}");
}
