# B4-30 fix7 inline-check — TRUNCATED asm excerpt (P2-3 fix8).
# Cell: win release production internals. Environment: windows x86_64,
# rustc 1.97.0, features "production internals".
# Build command (full output reproducible):
#   RUSTFLAGS="--emit asm" cargo test --release --test b4_30_first_alloc_min_stack --features "production internals" --no-run
# Kept verbatim from the full dump: the eater `eat`, the black_box'd
# scenario-body closure-dispatcher (its vtable jumps to sc_*), and the
# allocator-frame scenarios sc_small / sc_large / sc_foreign_free.
# Findings these excerpts support: (1) eat contains only self-recursion +
# one indirect call (the body); (2) the allocator inline(always) entry
# chain is inlined ONLY into sc_* frames; (3) sc_eat_only is empty and LLVM
# removed it (no symbol); (4) scenario is a pure tail-jump dispatcher.
# TRUNCATED — the rest of the 21422-line full dump (other fns, data,
# unwind) is omitted; regenerate via the build command above.

	.def	_RNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00B5_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00B5_,unique,33
	.p2align	4
_RNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00B5_:
.Lfunc_begin32:
.seh_proc _RNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00B5_
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	movq	(%rcx), %rdx
	movq	8(%rdx), %rax
	movq	16(%rdx), %rdx
	cmpq	$5, %rdx
	je	.LBB32_5
	cmpq	$8, %rdx
	jne	.LBB32_2
	movabsq	$8749489599857058149, %rcx
	cmpq	%rcx, (%rax)
	jne	.LBB32_4
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	retq
.LBB32_5:
	movl	$1818324339, %ecx
	xorl	(%rax), %ecx
	movzbl	4(%rax), %edx
	xorl	$108, %edx
	orl	%ecx, %edx
	je	.LBB32_11
	movl	$1735549292, %ecx
	xorl	(%rax), %ecx
	movzbl	4(%rax), %eax
	xorl	$101, %eax
	orl	%ecx, %eax
	jne	.LBB32_4
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	jmp	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_large
.LBB32_2:
	cmpq	$12, %rdx
	jne	.LBB32_4
	movabsq	$3273667681516154726, %rdx
	xorq	(%rax), %rdx
	movl	8(%rax), %eax
	xorq	$1701147238, %rax
	orq	%rdx, %rax
	jne	.LBB32_4
	movq	8(%rcx), %rax
	movq	(%rax), %rcx
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	jmp	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15sc_foreign_free
.LBB32_11:
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	jmp	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_small
.LBB32_4:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.114(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.115(%rip), %r8
	movl	$33, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	ud2
.Lfunc_end32:
	.seh_endproc
	.def	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15sc_foreign_free;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15sc_foreign_free,unique,39
	.p2align	4
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15sc_foreign_free:
.Lfunc_begin38:
.seh_proc _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15sc_foreign_free
	pushq	%r14
	.seh_pushreg %r14
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	testq	%rcx, %rcx
	je	.LBB38_36
	movq	%rcx, %rax
	movl	_tls_index(%rip), %ecx
	movq	%gs:88, %rdx
	movq	(%rdx,%rcx,8), %rcx
	movq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rcx), %rcx
	leaq	-1(%rcx), %rdx
	cmpq	$-2, %rdx
	jae	.LBB38_9
	movq	%rax, %rdx
	andq	$-4194304, %rdx
	cmpq	%rdx, 304(%rcx)
	jne	.LBB38_4
	movq	312(%rcx), %rbx
	jmp	.LBB38_15
.LBB38_4:
	movq	%rdx, %r9
	shrq	$22, %r9
	movl	%r9d, %r10d
	andl	$15, %r10d
	shll	$4, %r10d
	leaq	(%rcx,%r10), %r8
	addq	$48, %r8
	cmpq	%rdx, 48(%rcx,%r10)
	jne	.LBB38_5
	movq	8(%r8), %rbx
	jmp	.LBB38_15
.LBB38_5:
	movq	%rdx, %r10
	shrq	$35, %r10
	movq	%rdx, %r11
	shrq	$48, %r11
	xorl	%r11d, %r10d
	xorl	%r10d, %r9d
	andl	$8191, %r9d
	movq	320(%rcx), %r10
	movq	%r9, %r11
	.p2align	4
.LBB38_6:
	movq	(%r10,%r11,8), %rbx
	testq	%rbx, %rbx
	je	.LBB38_9
	movq	%rbx, %rsi
	andq	$-4194304, %rsi
	cmpq	%rdx, %rsi
	je	.LBB38_10
	incl	%r11d
	andl	$8191, %r11d
	cmpq	%r9, %r11
	jne	.LBB38_6
	jmp	.LBB38_9
.LBB38_10:
	movq	%rbx, %r9
	andq	$4194303, %r9
	je	.LBB38_13
	leaq	-1(%r9), %r10
	movl	344(%rcx), %r11d
	cmpq	%r11, %r10
	jae	.LBB38_9
	movq	32(%rcx), %r10
	movq	-8(%r10,%r9,8), %rbx
	testq	%rbx, %rbx
	je	.LBB38_9
.LBB38_13:
	movq	%rdx, (%r8)
	movq	%rbx, 8(%r8)
	cmpq	%rdx, %rbx
	jne	.LBB38_15
	movq	%rdx, 304(%rcx)
	movq	%rbx, 312(%rcx)
.LBB38_15:
	movq	%rax, %r14
	subq	%rbx, %r14
	movzbl	28(%rbx), %r8d
	movl	$4194304, %edx
	cmpl	$2, %r8d
	jne	.LBB38_17
	movq	56(%rbx), %rdx
.LBB38_17:
	cmpq	%rdx, %r14
	jae	.LBB38_9
	leaq	(%rbx,%r14), %rsi
	movzbl	1600(%rcx), %edx
	testl	%r8d, %r8d
	je	.LBB38_19
	cmpl	$2, %r8d
	je	.LBB38_32
	cmpl	$1, %r8d
	jne	.LBB38_31
	movl	$270336, %r8d
	movl	%r14d, %eax
	cmpq	%r8, %rax
	jb	.LBB38_31
	jmp	.LBB38_24
.LBB38_32:
	movq	48(%rbx), %rdx
	addq	%rbx, %rdx
	movq	8(%rbx), %r8
	setb	%r8b
	cmpq	%rax, %rdx
	setne	%al
	orb	%r8b, %al
	jne	.LBB38_31
	movq	%rbx, 32(%rsp)
	movl	$16, %r8d
	movl	$64, %r9d
	movq	%rsi, %rdx
	callq	_RNvNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core4free16dealloc_own_base19route_to_large_free
	jmp	.LBB38_36
.LBB38_19:
	movl	$389120, %r8d
	movl	%r14d, %eax
	cmpq	%r8, %rax
	jae	.LBB38_24
.LBB38_31:
	lock		incq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core10alloc_core8counters18LOCAL_REJECT_FREES(%rip)
.LBB38_36:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	.seh_endepilogue
	retq
.LBB38_24:
	testb	$15, %r14b
	jne	.LBB38_36
	shrl	$4, %r14d
	movzbl	5512(%rbx,%r14), %r8d
	movzbl	%r8b, %r9d
	testl	%r9d, %r9d
	je	.LBB38_30
	cmpl	$4, %r9d
	jne	.LBB38_35
	cmpq	(%rbx), %rax
	jae	.LBB38_31
	cmpb	$15, %dl
	ja	.LBB38_34
	movzbl	%dl, %eax
	movb	$68, 5512(%rbx,%r14)
	movq	%rsi, 1608(%rcx,%rax,8)
	incb	%dl
	movb	%dl, 1600(%rcx)
	jmp	.LBB38_36
.LBB38_9:
	movl	$16, %edx
	movl	$64, %r8d
	movq	%rax, %rcx
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	.seh_endepilogue
	jmp	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry17heap_core_xthread7routingNtNtNtB6_9heap_core4core8HeapCore15publish_foreign
.LBB38_30:
	cmpq	(%rbx), %rax
	jae	.LBB38_31
	jmp	.LBB38_36
.LBB38_35:
	cmpb	$64, %r8b
	jae	.LBB38_31
	jmp	.LBB38_36
.LBB38_34:
	leaq	1608(%rcx), %r8
	movl	$3, %edx
	movl	$8, %r9d
	movq	%rcx, %rdi
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small25alloc_core_small_magazineNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore20flush_class_internal
	movups	1672(%rdi), %xmm0
	movups	1688(%rdi), %xmm1
	movups	1704(%rdi), %xmm2
	movups	1720(%rdi), %xmm3
	movups	%xmm3, 1656(%rdi)
	movups	%xmm2, 1640(%rdi)
	movups	%xmm1, 1624(%rdi)
	movups	%xmm0, 1608(%rdi)
	movb	$68, 5512(%rbx,%r14)
	movq	%rsi, 1672(%rdi)
	movb	$9, 1600(%rdi)
	jmp	.LBB38_36
.Lfunc_end38:
	.seh_endproc
	.def	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3eat;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3eat,unique,40
	.p2align	4
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3eat:
.Lfunc_begin39:
.seh_proc _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3eat
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$64, %rsp
	.seh_stackalloc 64
	.seh_endprologue
	movb	$0, 47(%rsp)
	leaq	47(%rsp), %rsi
	movq	%rsi, 48(%rsp)
	leaq	48(%rsp), %rax
	#APP
	#NO_APP
	movq	%rsi, %r9
	xchgq	%r9, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4PREV(%rip)
	xorl	%eax, %eax
	subq	%rsi, %r9
	cmovaeq	%r9, %rax
	jbe	.LBB39_3
	movq	%rax, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack10STRIDE_ACT.0(%rip)
	cmpl	$1, (%rdx)
	jne	.LBB39_3
	xorl	%r10d, %r10d
	movq	%rsi, %r11
	subq	8(%rdx), %r11
	movq	16(%rdx), %r9
	cmovaeq	%r11, %r10
	cmpq	%r9, %r10
	jae	.LBB39_5
.LBB39_3:
	testq	%rcx, %rcx
	je	.LBB39_7
.LBB39_4:
	decq	%rcx
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3eat
	jmp	.LBB39_8
.LBB39_5:
	xorl	%r11d, %r11d
	subq	%rax, %r10
	cmovaeq	%r10, %r11
	testq	%rcx, %rcx
	je	.LBB39_7
	cmpq	%r9, %r11
	jae	.LBB39_4
.LBB39_7:
	movq	%rsi, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4DEEP.0(%rip)
	movq	%r8, 48(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.23(%rip), %rax
	movq	%rax, 56(%rsp)
	leaq	48(%rsp), %rax
	#APP
	#NO_APP
	movq	48(%rsp), %rcx
	movq	56(%rsp), %rax
	callq	*32(%rax)
.LBB39_8:
	movq	%rsi, 48(%rsp)
	leaq	48(%rsp), %rax
	#APP
	#NO_APP
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rsi
	.seh_endepilogue
	retq
.Lfunc_end39:
	.seh_endproc
	.def	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_large;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_large,unique,44
	.p2align	4
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_large:
.Lfunc_begin43:
.seh_proc _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_large
	subq	$136, %rsp
	.seh_stackalloc 136
	.seh_endprologue
	movl	_tls_index(%rip), %eax
	movq	%gs:88, %rcx
	movq	(%rcx,%rax,8), %rax
	movq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rax), %rcx
	leaq	-1(%rcx), %rax
	cmpq	$-2, %rax
	jb	.LBB43_4
	testq	%rcx, %rcx
	je	.LBB43_2
.LBB43_12:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665(%rip), %rcx
	movl	$16, %edx
	movl	$259776, %r8d
	callq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc18alloc_via_fallback
	testq	%rax, %rax
	je	.LBB43_13
.LBB43_11:
	movq	%rax, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3PTR(%rip)
	movq	$259776, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4SIZE(%rip)
	.seh_startepilogue
	addq	$136, %rsp
	.seh_endepilogue
	retq
.LBB43_2:
	xorps	%xmm0, %xmm0
	movaps	%xmm0, 112(%rsp)
	movaps	%xmm0, 96(%rsp)
	movaps	%xmm0, 80(%rsp)
	movaps	%xmm0, 64(%rsp)
	movaps	%xmm0, 48(%rsp)
	movq	$0, 128(%rsp)
	leaq	48(%rsp), %rcx
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap28bind_slow_tagged_with_config
	cmpq	$1, %rax
	je	.LBB43_12
	movq	%rdx, %rcx
.LBB43_4:
	movl	$16, %edx
	movl	$259776, %r8d
	xorl	%r9d, %r9d
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core5alloc3hotNtNtB6_4core8HeapCore28alloc_large_without_magazine
	testq	%rax, %rax
	jne	.LBB43_11
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	je	.LBB43_6
	cmpq	$1, %rax
	je	.LBB43_13
.LBB43_8:
	callq	*%rax
	testb	%al, %al
	jne	.LBB43_9
.LBB43_13:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.110(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.111(%rip), %r8
	movl	$30, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking5panic
	ud2
.LBB43_9:
	movl	$16, %ecx
	movl	$259776, %edx
	xorl	%r8d, %r8d
	callq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc17exit_escape_alloc
	testq	%rax, %rax
	jne	.LBB43_11
	jmp	.LBB43_13
.LBB43_6:
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit7resolve
	cmpq	$1, %rax
	jne	.LBB43_8
	jmp	.LBB43_13
.Lfunc_end43:
	.seh_endproc
	.def	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_small;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_small,unique,45
	.p2align	4
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_small:
.Lfunc_begin44:
.seh_proc _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack8sc_small
	subq	$120, %rsp
	.seh_stackalloc 120
	.seh_endprologue
	movl	_tls_index(%rip), %eax
	movq	%gs:88, %rcx
	movq	(%rcx,%rax,8), %rax
	movq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rax), %rcx
	leaq	-1(%rcx), %rax
	cmpq	$-2, %rax
	jae	.LBB44_3
	movzbl	1192(%rcx), %eax
	testq	%rax, %rax
	je	.LBB44_8
.LBB44_2:
	decq	%rax
	movb	%al, 1192(%rcx)
	andl	$15, %eax
	movq	1200(%rcx,%rax,8), %rax
	movq	%rax, %rcx
	andq	$-4194304, %rcx
	movl	%eax, %edx
	shrl	$4, %edx
	andl	$262143, %edx
	andb	$63, 5512(%rcx,%rdx)
	movq	%rax, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3PTR(%rip)
	movq	$16, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4SIZE(%rip)
	.seh_startepilogue
	addq	$120, %rsp
	.seh_endepilogue
	retq
.LBB44_3:
	testq	%rcx, %rcx
	je	.LBB44_6
.LBB44_4:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665(%rip), %rcx
	movl	$16, %edx
	movl	$16, %r8d
	callq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc18alloc_via_fallback
	testq	%rax, %rax
	je	.LBB44_12
.LBB44_5:
	movq	%rax, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3PTR(%rip)
	movq	$16, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4SIZE(%rip)
	.seh_startepilogue
	addq	$120, %rsp
	.seh_endepilogue
	retq
.LBB44_6:
	xorps	%xmm0, %xmm0
	movaps	%xmm0, 96(%rsp)
	movaps	%xmm0, 80(%rsp)
	movaps	%xmm0, 64(%rsp)
	movaps	%xmm0, 48(%rsp)
	movaps	%xmm0, 32(%rsp)
	movq	$0, 112(%rsp)
	leaq	32(%rsp), %rcx
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap28bind_slow_tagged_with_config
	cmpq	$1, %rax
	je	.LBB44_4
	movq	%rdx, %rcx
	movzbl	1192(%rcx), %eax
	testq	%rax, %rax
	jne	.LBB44_2
.LBB44_8:
	xorl	%edx, %edx
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core5alloc3hotNtNtB6_4core8HeapCore20refill_magazine_slow
	testq	%rax, %rax
	jne	.LBB44_5
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	je	.LBB44_14
	cmpq	$1, %rax
	je	.LBB44_12
.LBB44_11:
	callq	*%rax
	testb	%al, %al
	jne	.LBB44_13
.LBB44_12:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.110(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.112(%rip), %r8
	movl	$30, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking5panic
	ud2
.LBB44_13:
	movl	$16, %ecx
	movl	$16, %edx
	xorl	%r8d, %r8d
	callq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc17exit_escape_alloc
	testq	%rax, %rax
	jne	.LBB44_5
	jmp	.LBB44_12
.LBB44_14:
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit7resolve
	cmpq	$1, %rax
	jne	.LBB44_11
	jmp	.LBB44_12
.Lfunc_end44:
	.seh_endproc
