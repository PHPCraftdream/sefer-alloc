_RINvCsgNgFRE0vEzP_15malloc_bench_rs10alloc_shimNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc6global11sefer_alloc4core10SeferAllocECsfzcrG5C0XrY_12malloc_macro:
.Lfunc_begin2:
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	movq	%rdx, %rsi
	movq	%rcx, %rdi
	movl	_tls_index(%rip), %eax
	movq	%gs:88, %rcx
	movq	(%rcx,%rax,8), %rax
	movq	_RNvNCNKNvNtNtCs8xaSFZrx2Ng_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rax), %rcx
	cmpq	$1, %rcx
	jbe	.LBB2_4
.LBB2_1:
	leaq	-1(%rsi), %rax
	shrq	$4, %rax
	leaq	_RNvNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc10alloc_core7segment6bitmap8live_tag8SIZE2TAG(%rip), %rdx
	movzbl	(%rax,%rdx), %edx
	movl	%edx, %eax
	shll	$7, %eax
	leaq	(%rax,%rdx,8), %r8
	movzbl	1056(%rcx,%r8), %eax
	testq	%rax, %rax
	je	.LBB2_6
	addq	%r8, %rcx
	addq	$1056, %rcx
	decq	%rax
	movb	%al, (%rcx)
	andl	$15, %eax
	movq	8(%rcx,%rax,8), %rax
	movq	%rax, %rcx
	andq	$-4194304, %rcx
	movl	%eax, %edx
	shrl	$4, %edx
	andl	$262143, %edx
	andb	$63, 5512(%rcx,%rdx)
.LBB2_3:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
	.p2align	5
.LBB2_4:
	testq	%rcx, %rcx
	je	.LBB2_10
.LBB2_5:
	movl	$8, %edx
	movq	%rdi, %rcx
	movq	%rsi, %r8
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvNtNtNtCs8xaSFZrx2Ng_11sefer_alloc6global11sefer_alloc12global_alloc18alloc_via_fallback
	.p2align	5
.LBB2_6:
	decq	%rdx
	callq	_RNvMNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry9heap_core5alloc3hotNtNtB6_4core8HeapCore20refill_magazine_slow
	testq	%rax, %rax
	jne	.LBB2_3
	movq	_RNvNtNtCs8xaSFZrx2Ng_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	je	.LBB2_12
	cmpq	$1, %rax
	je	.LBB2_9
.LBB2_13:
	callq	*%rax
	testb	%al, %al
	jne	.LBB2_15
.LBB2_9:
	xorl	%eax, %eax
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
	.p2align	5
.LBB2_10:
	movq	%rdi, %rcx
	callq	_RNvNtNtCs8xaSFZrx2Ng_11sefer_alloc6global8tls_heap28bind_slow_tagged_with_config
	cmpq	$1, %rax
	je	.LBB2_5
	movq	%rdx, %rcx
	jmp	.LBB2_1
	.p2align	5
.LBB2_12:
	callq	_RNvNtNtCs8xaSFZrx2Ng_11sefer_alloc6global12process_exit7resolve
	cmpq	$1, %rax
	je	.LBB2_9
	jmp	.LBB2_13
	.p2align	5
.LBB2_15:
	movl	$8, %ecx
	movq	%rsi, %rdx
	xorl	%r8d, %r8d
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvNtNtNtCs8xaSFZrx2Ng_11sefer_alloc6global11sefer_alloc12global_alloc17exit_escape_alloc
.Lfunc_end2:
