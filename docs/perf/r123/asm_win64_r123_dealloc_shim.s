_RINvCsgNgFRE0vEzP_15malloc_bench_rs12dealloc_shimNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc6global11sefer_alloc4core10SeferAllocECsfzcrG5C0XrY_12malloc_macro:
.Lfunc_begin5:
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	.seh_endprologue
	movl	_tls_index(%rip), %eax
	movq	%gs:88, %r9
	movq	(%r9,%rax,8), %rax
	movq	_RNvNCNKNvNtNtCs8xaSFZrx2Ng_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rax), %r9
	cmpq	$2, %r9
	jb	.LBB5_9
	cmpq	$16, %rdx
	ja	.LBB5_10
	leaq	-1(%r8), %r11
	cmpq	$258752, %r11
	jae	.LBB5_10
	movq	%rcx, %rax
	andq	$-4194304, %rax
	cmpq	%rax, 304(%r9)
	jne	.LBB5_10
	movq	312(%r9), %rax
	movq	%rcx, %r10
	subq	%rax, %r10
	movzbl	28(%rax), %esi
	leaq	_RNvNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry9heap_core4free12dealloc_fast7LANE_PS(%rip), %rdi
	cmpq	(%rdi,%rsi,8), %r10
	jb	.LBB5_10
	rolq	$60, %r10
	movq	(%rax), %rsi
	shrq	$4, %rsi
	cmpq	%rsi, %r10
	jae	.LBB5_10
	shrq	$4, %r11
	leaq	_RNvNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc10alloc_core7segment6bitmap8live_tag8SIZE2TAG(%rip), %rsi
	movzbl	(%r11,%rsi), %r11d
	cmpb	%r11b, 5512(%rax,%r10)
	jne	.LBB5_10
	movq	%r11, %rdx
	decq	%rdx
	movq	%rdx, %r8
	shlq	$7, %r8
	leaq	(%r8,%rdx,8), %rsi
	movzbl	1192(%r9,%rsi), %r8d
	leaq	_RNvNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry9heap_core5state6tcache13FREE_PARK_CAP(%rip), %rdi
	cmpb	(%rdx,%rdi), %r8b
	jae	.LBB5_8
	leaq	(%r9,%rsi), %rdx
	addq	$1192, %rdx
	leal	64(%r11), %r9d
	movb	%r9b, 5512(%rax,%r10)
	andl	$15, %r8d
	movl	%r8d, %eax
	movq	%rcx, 8(%rdx,%rax,8)
	leal	1(%r8), %eax
	movb	%al, (%rdx)
	.seh_startepilogue
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
	.p2align	5
.LBB5_10:
	.seh_startepilogue
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry9heap_core4free12dealloc_fast9lane_miss
	.p2align	5
.LBB5_9:
	.seh_startepilogue
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvMNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry17heap_core_xthread7routingNtNtNtB6_9heap_core4core8HeapCore15publish_foreign
	.p2align	5
.LBB5_8:
	movq	%rcx, %r10
	movq	%r9, %rcx
	movq	%rax, %r8
	movq	%r10, %r9
	.seh_startepilogue
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvMs_NtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry9heap_core4free16dealloc_own_baseNtNtB8_4core8HeapCore23free_magazine_full_fast
.Lfunc_end5:
