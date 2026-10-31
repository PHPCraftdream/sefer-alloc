_RINvCsgNgFRE0vEzP_15malloc_bench_rs12dealloc_shimNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc6global11sefer_alloc4core10SeferAllocECsfzcrG5C0XrY_12malloc_macro:
.Lfunc_begin5:
	pushq	%r14
	.seh_pushreg %r14
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	.seh_endprologue
	movq	%r8, %r10
	movq	%rdx, %r8
	movq	%rcx, %rax
	movl	_tls_index(%rip), %ecx
	movq	%gs:88, %rdx
	movq	(%rdx,%rcx,8), %rcx
	movq	_RNvNCNKNvNtNtCs8xaSFZrx2Ng_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rcx), %rcx
	leaq	-1(%rcx), %rdx
	cmpq	$-3, %rdx
	ja	.LBB5_14
	cmpq	$16, %r8
	ja	.LBB5_15
	leaq	-1(%r10), %rdx
	cmpq	$258752, %rdx
	jae	.LBB5_15
	shrq	$4, %rdx
	leaq	_RNvNtNtNtCs8xaSFZrx2Ng_11sefer_alloc10alloc_core8platform12size_classes10SIZE2CLASS(%rip), %r9
	movzbl	(%rdx,%r9), %edx
	cmpq	$48, %rdx
	ja	.LBB5_15
	movq	%rax, %r9
	andq	$-4194304, %r9
	cmpq	%r9, 304(%rcx)
	jne	.LBB5_15
	movq	312(%rcx), %r11
	movzbl	28(%r11), %esi
	cmpq	$1, %rsi
	ja	.LBB5_15
	leaq	anon.c01a4d29ffff6eb27a04e41f3adec04e.78(%rip), %rdi
	movq	%rax, %r9
	subq	%r11, %r9
	cmpq	(%rdi,%rsi,8), %r9
	jb	.LBB5_15
	testb	$15, %r9b
	jne	.LBB5_15
	cmpq	(%r11), %r9
	jae	.LBB5_15
	movl	%r9d, %esi
	shrl	$4, %esi
	movzbl	5512(%r11,%rsi), %ebx
	leal	1(%rdx), %edi
	cmpb	%dil, %bl
	jne	.LBB5_15
	movq	%r9, %rax
	addq	%r11, %rax
	movl	%edx, %r8d
	shll	$7, %r8d
	leaq	(%r8,%rdx,8), %rdi
	movzbl	1192(%rcx,%rdi), %r10d
	movzbl	%r10b, %r8d
	leaq	_RNvNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry9heap_core5state6tcache13FREE_PARK_CAP(%rip), %r14
	cmpb	(%rdx,%r14), %r8b
	jae	.LBB5_13
	addq	%rdi, %rcx
	addq	$1192, %rcx
	orb	$64, %bl
	movb	%bl, 5512(%r11,%rsi)
	andl	$15, %r8d
	movq	%rax, 8(%rcx,%r8,8)
	incb	%r10b
	movb	%r10b, (%rcx)
.LBB5_12:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	.seh_endepilogue
	retq
	.p2align	5
.LBB5_15:
	movq	%rax, %rdx
	movq	%r10, %r9
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	.seh_endepilogue
	jmp	_RNvMNtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry9heap_core4free12dealloc_fastNtNtB6_4core8HeapCore13dealloc_chain
	.p2align	5
.LBB5_14:
	movq	%rax, %rcx
	movq	%r8, %rdx
	movq	%r10, %r8
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	.seh_endepilogue
	jmp	_RNvMNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry17heap_core_xthread7routingNtNtNtB6_9heap_core4core8HeapCore15publish_foreign
	.p2align	5
.LBB5_13:
	movq	%rax, 40(%rsp)
	movq	%r11, 32(%rsp)
	callq	_RNvMs_NtNtNtNtCs8xaSFZrx2Ng_11sefer_alloc8registry9heap_core4free16dealloc_own_baseNtNtB8_4core8HeapCore23free_magazine_full_fast
	jmp	.LBB5_12
.Lfunc_end5:
