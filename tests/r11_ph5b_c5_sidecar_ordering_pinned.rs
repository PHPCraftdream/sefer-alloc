//! Source pins действующего ChunkedBank; не замена actual-source Loom/Miri.

/// Удаляет CRLF и отдельные строки комментариев.
fn src(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn bank_src() -> String {
    src(include_str!(
        "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap/chunked_bank.rs"
    ))
}

fn scan_src() -> String {
    src(include_str!(
        "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"
    ))
}

/// Тело метода до закрывающей скобки с четырьмя пробелами отступа.
fn fn_body(text: &str, name: &str) -> String {
    let start = text
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("fn {name} present"));
    let rest = &text[start..];
    let end = rest
        .find("\n    }\n")
        .unwrap_or_else(|| panic!("end of fn {name}"));
    rest[..end].to_string()
}

#[test]
fn producer_publish_is_acqrel_fetch_or() {
    let body = fn_body(&bank_src(), "publish");
    assert_eq!(
        body.matches(".fetch_or(mask, Ordering::AcqRel)").count(),
        1,
        "terminal publication выполняет один AcqRel fetch_or:\n{body}"
    );
}

#[test]
fn owner_issue_start_store_is_release() {
    let body = fn_body(&bank_src(), "issue");
    assert!(body.contains(".store(old | mask, Ordering::Release)"));
    assert!(body.contains(".load(Ordering::Acquire)"));
}

#[test]
fn owner_cut_is_acqrel_swap() {
    let bank = fn_body(&bank_src(), "pending_swap");
    assert_eq!(bank.matches(".swap(0, Ordering::AcqRel)").count(), 1);
    let scan = fn_body(&scan_src(), "next_cut");
    assert!(
        scan.contains("Some(bank) => bank.pending_swap(word)"),
        "production scanner должен вызывать проверенный chunked cut:\n{scan}"
    );
    // Плотный test-oracle тоже сохраняет тот же ordering.
    assert_eq!(scan.matches(".swap(0, Ordering::AcqRel)").count(), 1);
}

#[test]
fn chunk_construction_is_release_published_and_acquire_observed() {
    let bank = bank_src();
    let construction = fn_body(&bank, "ensure_chunk");
    assert_eq!(
        construction
            .matches("ready.store(old | mask, Ordering::Release)")
            .count(),
        1
    );
    let pending = construction
        .find("pending.add(i).write(AtomicU64::new(0))")
        .unwrap();
    let starts = construction
        .find("starts.add(i).write(AtomicU64::new(0))")
        .unwrap();
    let publish = construction
        .find("ready.store(old | mask, Ordering::Release)")
        .unwrap();
    assert!(pending < publish && starts < publish);
    let observer = fn_body(&bank, "is_ready");
    assert_eq!(observer.matches(".load(Ordering::Acquire)").count(), 1);
}
