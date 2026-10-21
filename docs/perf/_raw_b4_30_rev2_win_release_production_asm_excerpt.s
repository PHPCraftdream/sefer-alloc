# identity: HEAD=4a2071eda743c63557f9dc065f34fff8e1fabf5e (asm of the --lib crate, NOT the
#   b4-30 test; any test sha256 in older headers of this file was misattributed and is removed).
#   rustc at capture: 1.97.0 (2d8144b78 2026-07-07), host x86_64-pc-windows-msvc.
# source asm: CARGO_TARGET_DIR=<shared> cargo rustc --release --features production --lib -- --emit=asm
#   (x86_64-pc-windows-msvc, lto=thin, codegen-units=1). NOTE: this is --features production
#   WITHOUT internals — NOT the measured b4-30 configuration (production internals); the excerpt
#   is a code-shape exhibit only, not a measured artifact.
# asm file: sefer_alloc-304a115edda9b1b1.s (29538 lines; __chkstk occurrences: 2)
#
# TRUNCATED EXCERPT — BEGIN (R14-10 style markers)
# reproduce full file: git show 4a2071eda743c63557f9dc065f34fff8e1fabf5e:sefer_alloc-304a115edda9b1b1.s
#   is unavailable (asm not tracked); regenerate with the command above, asm file name may vary.
# excerpt covers only the two functions below; everything else in the 29538-line file is omitted.
#
# ==== excerpt 1: HeapRegistry::claim_lease_with_config (claim/materialise path)
#     frame = 9080 bytes via __chkstk + sub rsp; AllocCore::new_with_config_and_owner is
#     CALLED here (callq with an sret buffer at 2416(%rsp)) — NOT inlined into this frame.
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvMNtNtNtCs8wEP0EGUd4H_11sefer_alloc8registry13heap_registry5claimNtB2_12HeapRegistry23claim_lease_with_config,unique,53
	.p2align	4
_RNvMNtNtNtCs8wEP0EGUd4H_11sefer_alloc8registry13heap_registry5claimNtB2_12HeapRegistry23claim_lease_with_config:
.seh_proc _RNvMNtNtNtCs8wEP0EGUd4H_11sefer_alloc8registry13heap_registry5claimNtB2_12HeapRegistry23claim_lease_with_config
	pushq	%r15
	.seh_pushreg %r15
	pushq	%r14
	.seh_pushreg %r14
	pushq	%r13
	.seh_pushreg %r13
	pushq	%r12
	.seh_pushreg %r12
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rbx
	.seh_pushreg %rbx
	movl	$9080, %eax
	callq	__chkstk
	subq	%rax, %rsp
	.seh_stackalloc 9080
	.seh_endprologue
	movq	%rdx, 64(%rsp)
	movq	%rcx, 48(%rsp)
	leaq	_RNvNtNtNtCs8wEP0EGUd4H_11sefer_alloc8registry9bootstrap8registry8REGISTRY(%rip), %r12
	movq	$-1, %rbp
	xorl	%esi, %esi
	movl	$0, 40(%rsp)
	movl	$0, 44(%rsp)
	jmp	.LBB53_1
.LBB53_34:
	incl	40(%rsp)
	callq	_RNvNtNtCsaZOKLwlFm27_3std6thread9functions9yield_now
	.p2align	4
# TRUNCATED (unrelated straight-line body elided here; R14-10 marker)

#     ... AllocCore::new_with_config_and_owner is CALLED inside the same frame (sret 2416(%rsp)) ...
	movb	$2, %cl
	lock		cmpxchgb	%cl, (%rbx)
	jne	.LBB53_1
.LBB53_62:
	movl	$1, %esi
	lock		xaddq	%rsi, 8(%rbx)
	leaq	2416(%rsp), %rcx
	movq	%rdx, %r9
	movq	64(%rsp), %rdx
	movl	$1, %r8d
	movq	%r9, %rdi
	callq	_RNvMs_NtNtNtCs8wEP0EGUd4H_11sefer_alloc10alloc_core10alloc_core9lifecycleNtNtB6_15alloc_core_impl9AllocCore25new_with_config_and_owner
	movq	2416(%rsp), %r13
	cmpq	$-1, %r13
	jne	.LBB53_63
	movb	$2, %al
	movb	$3, %cl
	lock		cmpxchgb	%cl, (%rbx)
	jne	.LBB53_91
	movl	$1, %eax
	movl	%r15d, %ecx

# ==== excerpt 2: global::fallback::heap_ptr_with_config (first-alloc bootstrap)
#     frame = 9056 bytes via __chkstk + sub rsp:
	.def	_RNvNtNtCs8wEP0EGUd4H_11sefer_alloc6global8fallback20heap_ptr_with_config;
	.scl	2;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvNtNtCs8wEP0EGUd4H_11sefer_alloc6global8fallback20heap_ptr_with_config,unique,116
	.globl	_RNvNtNtCs8wEP0EGUd4H_11sefer_alloc6global8fallback20heap_ptr_with_config
	.p2align	4
_RNvNtNtCs8wEP0EGUd4H_11sefer_alloc6global8fallback20heap_ptr_with_config:
.Lfunc_begin32:
.seh_proc _RNvNtNtCs8wEP0EGUd4H_11sefer_alloc6global8fallback20heap_ptr_with_config
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%r14
	.seh_pushreg %r14
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	movl	$9056, %eax
	callq	__chkstk
	subq	%rax, %rsp
	.seh_stackalloc 9056
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 8920(%rbp)
	movzbl	_RNvNtNtCs8wEP0EGUd4H_11sefer_alloc6global8fallback10INIT_STATE(%rip), %eax
	leaq	_RNvNtNtCs8wEP0EGUd4H_11sefer_alloc6global8fallback8FALLBACK(%rip), %rsi
	cmpb	$2, %al
	je	.LBB116_30
	movq	%rcx, %rdi
	movb	$1, %bl
	jmp	.LBB116_3

# TRUNCATED EXCERPT -- END (both excerpts truncated mid-function; full asm file had 29538 lines)
