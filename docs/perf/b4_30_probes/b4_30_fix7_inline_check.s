	.def	@feat.00;
	.scl	3;
	.type	0;
	.endef
	.globl	@feat.00
@feat.00 = 0
	.file	"b4_30_first_alloc_min_stack.602daaf8bd80a59f-cgu.0"
	.def	_RINvMNtNtCsaZOKLwlFm27_3std4sync9once_lockINtB3_8OnceLockTjjjbEE10initializeNCINvB2_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zEB1B_;
	.scl	3;
	.type	32;
	.endef
	.section	.text$unlikely,"xr",one_only,_RINvMNtNtCsaZOKLwlFm27_3std4sync9once_lockINtB3_8OnceLockTjjjbEE10initializeNCINvB2_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zEB1B_,unique,0
_RINvMNtNtCsaZOKLwlFm27_3std4sync9once_lockINtB3_8OnceLockTjjjbEE10initializeNCINvB2_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zEB1B_:
.Lfunc_begin0:
.seh_proc _RINvMNtNtCsaZOKLwlFm27_3std4sync9once_lockINtB3_8OnceLockTjjjbEE10initializeNCINvB2_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zEB1B_
	subq	$72, %rsp
	.seh_stackalloc 72
	.seh_endprologue
	movl	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL+32(%rip), %eax
	testl	%eax, %eax
	jne	.LBB0_1
.LBB0_2:
	.seh_startepilogue
	addq	$72, %rsp
	.seh_endepilogue
	retq
.LBB0_1:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL(%rip), %rax
	leaq	56(%rsp), %rcx
	movq	%rax, (%rcx)
	leaq	47(%rsp), %rax
	movq	%rax, 8(%rcx)
	leaq	48(%rsp), %r8
	movq	%rcx, (%r8)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.2(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL+32(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.0(%rip), %r9
	movb	$1, %dl
	callq	_RNvMs0_NtNtNtNtCsaZOKLwlFm27_3std3sys4sync4once5futexNtB5_4Once4call
	jmp	.LBB0_2
.Lfunc_end0:
	.seh_endproc

	.def	_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text$unlikely,"xr",one_only,_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,1
	.p2align	4
_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin1:
.seh_proc _RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, (%rbp)
	movq	%rcx, %rax
	cmpq	$0, 24(%rcx)
	je	.LBB1_2
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.LBB1_2:
.Ltmp0:
	movq	%rdx, %rcx
	movq	%rax, %rdx
	movq	%rax, -8(%rbp)
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp1:
	movq	%rax, %rsi
	movl	$40, %edx
	movl	$8, %r8d
	movq	-8(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%rsi, %rax
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text$unlikely,"xr",one_only,_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,1
	.seh_endproc
	.def	"?dtor$4@?0?_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$4@?0?_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$4@?0?_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB1_4:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	48(%rdx), %rbp
	.seh_endprologue
	movl	$40, %edx
	movl	$8, %r8d
	movq	-8(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end1:
	.seh_handlerdata
	.section	.text$unlikely,"xr",one_only,_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,1
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,0
	.p2align	2, 0x0
$cppxdata$_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	48
	.long	0
	.long	1
$stateUnwindMap$_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$4@?0?_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin1@IMGREL
	.long	-1
	.long	.Ltmp0@IMGREL
	.long	0
	.long	.Ltmp1@IMGREL
	.long	-1
	.section	.text$unlikely,"xr",one_only,_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,1

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,2
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin2:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$64, %rsp
	.seh_stackalloc 64
	leaq	64(%rsp), %rbp
	.seh_setframe %rbp, 64
	.seh_endprologue
	movq	$-2, -8(%rbp)
	cmpq	$0, (%rcx)
	je	.LBB2_9
	movq	8(%rcx), %rax
	movq	%rax, -24(%rbp)
	testq	%rax, %rax
	je	.LBB2_9
	movq	16(%rcx), %rax
	movq	%rax, -16(%rbp)
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB2_4
.Ltmp2:
	movq	-24(%rbp), %rcx
	callq	*%rax
	nop
.Ltmp3:
.LBB2_4:
	movq	-16(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB2_9
	movq	16(%rax), %r8
	movq	-24(%rbp), %rcx
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB2_9:
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,2
	.seh_endproc
	.def	"?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB2_6:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-16(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB2_8
	movq	-16(%rbp), %rax
	movq	16(%rax), %r8
	movq	-24(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB2_8:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end2:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,2
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,1
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	56
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin2@IMGREL
	.long	-1
	.long	.Ltmp2@IMGREL
	.long	0
	.long	.Ltmp3@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,2

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsaZOKLwlFm27_3std7process6OutputNtNtNtB11_2io5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsaZOKLwlFm27_3std7process6OutputNtNtNtB11_2io5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,3
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsaZOKLwlFm27_3std7process6OutputNtNtNtB11_2io5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin3:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsaZOKLwlFm27_3std7process6OutputNtNtNtB11_2io5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$32, %rsp
	.seh_stackalloc 32
	.seh_endprologue
	movq	(%rcx), %rdx
	cmpq	$-1, %rdx
	je	.LBB3_5
	testq	%rdx, %rdx
	je	.LBB3_3
	movq	8(%rcx), %rax
	movl	$1, %r8d
	movq	%rcx, %rsi
	movq	%rax, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%rsi, %rcx
.LBB3_3:
	movq	24(%rcx), %rdx
	testq	%rdx, %rdx
	je	.LBB3_6
	movq	32(%rcx), %rcx
	movl	$1, %r8d
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB3_5:
	addq	$8, %rcx
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rsi
	.seh_endepilogue
	jmp	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB3_6:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rsi
	.seh_endepilogue
	retq
.Lfunc_end3:
	.seh_endproc

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,4
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin4:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$48, %rsp
	.seh_stackalloc 48
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, -8(%rbp)
	cmpb	$-1, (%rcx)
	je	.LBB4_1
	.seh_startepilogue
	addq	$48, %rsp
	popq	%rbp
	.seh_endepilogue
	jmp	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB4_1:
	movq	8(%rcx), %rcx
	movq	(%rcx), %rax
	cmpq	$1, %rax
	movq	%rcx, -16(%rbp)
	je	.LBB4_6
	testq	%rax, %rax
	jne	.LBB4_7
	movq	-16(%rbp), %rax
	movq	16(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB4_7
	movq	-16(%rbp), %rax
	movq	8(%rax), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	jmp	.LBB4_7
.LBB4_6:
	addq	$8, %rcx
.Ltmp4:
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp5:
.LBB4_7:
	movl	$40, %edx
	movl	$8, %r8d
	movq	-16(%rbp), %rcx
	.seh_startepilogue
	addq	$48, %rsp
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,4
	.seh_endproc
	.def	"?dtor$5@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$5@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$5@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB4_5:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	48(%rdx), %rbp
	.seh_endprologue
	movl	$40, %edx
	movl	$8, %r8d
	movq	-16(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end4:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,4
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,2
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	40
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$5@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin4@IMGREL
	.long	-1
	.long	.Ltmp4@IMGREL
	.long	0
	.long	.Ltmp5@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,4

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,5
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin5:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$64, %rsp
	.seh_stackalloc 64
	leaq	64(%rsp), %rbp
	.seh_setframe %rbp, 64
	.seh_endprologue
	movq	$-2, -8(%rbp)
	movq	%rcx, -24(%rbp)
	movq	%rdx, -16(%rbp)
	movq	(%rdx), %rax
	testq	%rax, %rax
	je	.LBB5_2
.Ltmp6:
	movq	-24(%rbp), %rcx
	callq	*%rax
	nop
.Ltmp7:
.LBB5_2:
	movq	-24(%rbp), %rcx
	movq	-16(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB5_7
	movq	16(%rax), %r8
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB5_7:
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,5
	.seh_endproc
	.def	"?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB5_4:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-24(%rbp), %rcx
	movq	-16(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB5_6
	movq	-16(%rbp), %rax
	movq	16(%rax), %r8
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB5_6:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end5:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,5
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,3
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	56
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin5@IMGREL
	.long	-1
	.long	.Ltmp6@IMGREL
	.long	0
	.long	.Ltmp7@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,5

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,6
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin6:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
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
	subq	$96, %rsp
	.seh_stackalloc 96
	leaq	96(%rsp), %rbp
	.seh_setframe %rbp, 96
	.seh_endprologue
	movq	$-2, -8(%rbp)
	movq	8(%rcx), %rax
	movq	%rax, -16(%rbp)
	movq	%rcx, -40(%rbp)
	movq	16(%rcx), %rax
	testq	%rax, %rax
	je	.LBB6_7
	movq	-16(%rbp), %rcx
	addq	$24, %rcx
	movl	$1, %edx
	subq	%rax, %rdx
	jmp	.LBB6_2
	.p2align	4
.LBB6_6:
	movq	-56(%rbp), %rcx
	addq	$16, %rcx
	movq	-48(%rbp), %rdx
	incq	%rdx
	cmpq	$1, %rdx
	je	.LBB6_7
.LBB6_2:
	movq	%rdx, -48(%rbp)
	movq	-24(%rcx), %rax
	movq	%rax, -32(%rbp)
	movq	%rcx, -56(%rbp)
	movq	-16(%rcx), %rax
	movq	%rax, -24(%rbp)
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB6_4
.Ltmp8:
	movq	-32(%rbp), %rcx
	callq	*%rax
	nop
.Ltmp9:
.LBB6_4:
	movq	-32(%rbp), %rcx
	movq	-24(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB6_6
	movq	16(%rax), %r8
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	jmp	.LBB6_6
.LBB6_7:
	movq	-40(%rbp), %rax
	movq	(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB6_22
	shlq	$4, %rdx
	movl	$8, %r8d
	movq	-16(%rbp), %rcx
	.seh_startepilogue
	addq	$96, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB6_22:
	.seh_startepilogue
	addq	$96, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,6
	.seh_endproc
	.def	"?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB6_9:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	96(%rdx), %rbp
	.seh_endprologue
	movq	-32(%rbp), %rcx
	movq	-24(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB6_11
	movq	-24(%rbp), %rax
	movq	16(%rax), %r8
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB6_11:
	movq	-48(%rbp), %r14
	testq	%r14, %r14
	movq	-56(%rbp), %rbx
	jne	.LBB6_12
.LBB6_18:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.LBB6_12:
	negq	%r14
	jmp	.LBB6_13
	.p2align	4
.LBB6_17:
	addq	$16, %rbx
	decq	%r14
	je	.LBB6_18
.LBB6_13:
	movq	-8(%rbx), %rsi
	movq	(%rbx), %rdi
	movq	(%rdi), %rax
	testq	%rax, %rax
	je	.LBB6_15
	movq	%rsi, %rcx
	callq	*%rax
.LBB6_15:
	movq	8(%rdi), %rdx
	testq	%rdx, %rdx
	je	.LBB6_17
	movq	16(%rdi), %r8
	movq	%rsi, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	jmp	.LBB6_17
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,6
	.seh_endproc
	.def	"?dtor$19@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$19@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$19@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB6_19:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	96(%rdx), %rbp
	.seh_endprologue
	movq	-40(%rbp), %rax
	movq	(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB6_21
	shlq	$4, %rdx
	movl	$8, %r8d
	movq	-16(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB6_21:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end6:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,6
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,4
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	2
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	88
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$19@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	0
	.long	"?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin6@IMGREL
	.long	-1
	.long	.Ltmp8@IMGREL
	.long	1
	.long	.Ltmp9@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,6

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,7
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin7:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$72, %rsp
	.seh_stackalloc 72
	leaq	64(%rsp), %rbp
	.seh_setframe %rbp, 64
	.seh_endprologue
	movq	$-2, (%rbp)
	movq	%rcx, %rax
	movq	8(%rcx), %rcx
	movq	%rax, -24(%rbp)
	movq	16(%rax), %rax
	movq	%rax, -32(%rbp)
	movq	$0, -16(%rbp)
	movq	%rcx, -8(%rbp)
	.p2align	4
.LBB7_1:
	movq	-16(%rbp), %rax
	cmpq	%rax, -32(%rbp)
	je	.LBB7_9
	incq	%rax
	movq	%rax, -16(%rbp)
	leaq	32(%rcx), %rsi
.Ltmp10:
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp11:
	movq	%rsi, %rcx
	jmp	.LBB7_1
.LBB7_9:
	movq	-24(%rbp), %rax
	movq	(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB7_10
	shlq	$5, %rdx
	movl	$8, %r8d
	movq	-8(%rbp), %rcx
	.seh_startepilogue
	addq	$72, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB7_10:
	.seh_startepilogue
	addq	$72, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,7
	.seh_endproc
	.def	"?dtor$3@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$3@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$3@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB7_3:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-32(%rbp), %rdi
	movq	-16(%rbp), %rbx
	cmpq	%rbx, %rdi
	movq	-8(%rbp), %rsi
	je	.LBB7_5
	.p2align	4
.LBB7_4:
	movq	%rbx, %rcx
	shlq	$5, %rcx
	addq	%rsi, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	incq	%rbx
	cmpq	%rdi, %rbx
	jne	.LBB7_4
.LBB7_5:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,7
	.seh_endproc
	.def	"?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB7_6:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-24(%rbp), %rax
	movq	(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB7_8
	shlq	$5, %rdx
	movl	$8, %r8d
	movq	-8(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB7_8:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end7:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,7
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,5
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	2
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	64
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	0
	.long	"?dtor$3@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin7@IMGREL
	.long	-1
	.long	.Ltmp10@IMGREL
	.long	1
	.long	.Ltmp11@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,7

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin8:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
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
	subq	$112, %rsp
	.seh_stackalloc 112
	leaq	112(%rsp), %rbp
	.seh_setframe %rbp, 112
	.seh_endprologue
	movq	$-2, -16(%rbp)
	leaq	16(%rcx), %rdi
	movq	24(%rcx), %rbx
	movq	32(%rcx), %rax
	testq	%rax, %rax
	setne	%r14b
	testq	%rbx, %rbx
	movq	%rcx, -40(%rbp)
	je	.LBB8_6
	testq	%rax, %rax
	je	.LBB8_6
	movq	%rax, -32(%rbp)
	movb	%r14b, -17(%rbp)
	movq	%rbx, -72(%rbp)
	movq	%rdi, -56(%rbp)
	movq	40(%rcx), %rax
	movq	%rax, -48(%rbp)
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB8_4
.Ltmp12:
	movq	-32(%rbp), %rcx
	callq	*%rax
	nop
.Ltmp13:
.LBB8_4:
	movq	-48(%rbp), %r8
	movq	8(%r8), %rdx
	testq	%rdx, %rdx
	movq	-40(%rbp), %rcx
	movq	-56(%rbp), %rdi
	movq	-72(%rbp), %rbx
	movzbl	-17(%rbp), %r14d
	movq	-32(%rbp), %rax
	je	.LBB8_6
	movq	16(%r8), %r8
	movq	%rcx, %rsi
	movq	%rax, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%rsi, %rcx
.LBB8_6:
	movq	$0, 24(%rcx)
.LBB8_7:
	movq	(%rdi), %rsi
	testq	%rsi, %rsi
	je	.LBB8_14
	andb	%r14b, %bl
	je	.LBB8_9
	movb	$1, 32(%rsi)
.LBB8_9:
	lock		decq	24(%rsi)
	jne	.LBB8_12
	movq	16(%rsi), %rcx
	movb	$1, %al
	xchgb	%al, 40(%rcx)
	cmpb	$-1, %al
	je	.LBB8_11
.LBB8_12:
	lock		decq	(%rsi)
	jne	.LBB8_14
.LBB8_13:
	#MEMBARRIER
.Ltmp18:
	movq	%rdi, %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread6scoped9ScopeDataE9drop_slowBM_
	nop
.Ltmp19:
.LBB8_14:
	movq	-40(%rbp), %rax
	cmpq	$0, 24(%rax)
	je	.LBB8_20
	movq	32(%rax), %rcx
	movq	%rcx, -32(%rbp)
	testq	%rcx, %rcx
	je	.LBB8_20
	movq	40(%rax), %rax
	movq	%rax, -40(%rbp)
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB8_18
.Ltmp20:
	movq	-32(%rbp), %rcx
	callq	*%rax
	nop
.Ltmp21:
.LBB8_18:
	movq	-40(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB8_20
	movq	16(%rax), %r8
	movq	-32(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB8_20:
	.seh_startepilogue
	addq	$112, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.LBB8_11:
	addq	$40, %rcx
	callq	*__imp_WakeByAddressSingle(%rip)
	lock		decq	(%rsi)
	je	.LBB8_13
	jmp	.LBB8_14
.LBB8_26:
$ehgcr_8_26:
	cmpq	$0, -32(%rbp)
	movq	-56(%rbp), %rdi
	movq	-72(%rbp), %rbx
	movzbl	-17(%rbp), %r14d
	je	.LBB8_7
	movl	$0, -64(%rbp)
	movb	$0, -60(%rbp)
.Ltmp14:
	leaq	-64(%rbp), %rcx
	callq	_RNvYNtNtNtNtCsaZOKLwlFm27_3std3sys5stdio7windows6StderrNtNtBa_2io5Write9write_fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp15:
	movq	%rax, -80(%rbp)
	testq	%rax, %rax
	je	.LBB8_30
.Ltmp16:
	leaq	-80(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp17:
.LBB8_30:
	movl	$7, %ecx
	#APP

	int	$41

	#NO_APP
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.seh_endproc
	.def	"?dtor$21@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$21@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$21@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB8_21:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	112(%rdx), %rbp
	.seh_endprologue
	movq	-48(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB8_23
	movq	-48(%rbp), %rax
	movq	16(%rax), %r8
	movq	-32(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB8_23:
	movq	-40(%rbp), %rax
	movq	$0, 24(%rax)
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.seh_endproc
	.def	"?catch$24@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?catch$24@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?catch$24@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
	.seh_handler __CxxFrameHandler3, @unwind, @except
.LBB8_24:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	112(%rdx), %rbp
	.seh_endprologue
	movq	-8(%rbp), %rcx
	callq	_RNvNvNtCsaZOKLwlFm27_3std9panicking12catch_unwind7cleanup
	movq	%rax, -32(%rbp)
	movq	%rdx, -48(%rbp)
	leaq	.LBB8_26(%rip), %rax
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.seh_endproc
	.def	"?catch$25@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?catch$25@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?catch$25@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
	.seh_handler __CxxFrameHandler3, @unwind, @except
.LBB8_25:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	112(%rdx), %rbp
	.seh_endprologue
	xorl	%ecx, %ecx
	callq	_RNvNvNtCsaZOKLwlFm27_3std9panicking12catch_unwind7cleanup
	movq	%rax, -32(%rbp)
	movq	%rdx, -48(%rbp)
	leaq	.LBB8_26(%rip), %rax
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.seh_endproc
	.def	"?dtor$32@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$32@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$32@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB8_32:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	112(%rdx), %rbp
	.seh_endprologue
	movq	-56(%rbp), %rax
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB8_35
	lock		decq	(%rax)
	jne	.LBB8_35
	#MEMBARRIER
	movq	-56(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread6scoped9ScopeDataE9drop_slowBM_
	nop
.LBB8_35:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.seh_endproc
	.def	"?dtor$36@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$36@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$36@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB8_36:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	112(%rdx), %rbp
	.seh_endprologue
	movq	-40(%rbp), %rcx
	addq	$24, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_4cell10UnsafeCellINtNtB4_6option6OptionINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.seh_endproc
	.def	"?dtor$37@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$37@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$37@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB8_37:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	112(%rdx), %rbp
	.seh_endprologue
	movq	-40(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB8_39
	movq	-40(%rbp), %rax
	movq	16(%rax), %r8
	movq	-32(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB8_39:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.seh_endproc
	.def	"?dtor$40@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$40@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$40@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB8_40:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	112(%rdx), %rbp
	.seh_endprologue
	movq	-32(%rbp), %rcx
	movq	-48(%rbp), %rdx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultuINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end8:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,6
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	7
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	1
	.long	$tryMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	10
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	96
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$36@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	0
	.long	"?dtor$32@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	1
	.long	"?dtor$40@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	1
	.long	0
	.long	3
	.long	"?dtor$21@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	1
	.long	0
	.long	-1
	.long	"?dtor$37@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$tryMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	3
	.long	4
	.long	5
	.long	2
	.long	$handlerMap$0$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
$handlerMap$0$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	8
	.long	_RNvCs9hJ03s5DiqP_7___rustc22___rust_panic_type_info@IMGREL
	.long	104
	.long	"?catch$24@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	88
	.long	64
	.long	0
	.long	0
	.long	"?catch$25@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	88
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin8@IMGREL
	.long	-1
	.long	.Ltmp12@IMGREL
	.long	4
	.long	.Ltmp13@IMGREL
	.long	-1
	.long	.Ltmp18@IMGREL
	.long	0
	.long	.Ltmp20@IMGREL
	.long	6
	.long	.Ltmp21@IMGREL
	.long	-1
	.long	.Ltmp14@IMGREL
	.long	2
	.long	.Ltmp17@IMGREL
	.long	-1
	.long	"?catch$24@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	5
	.long	"?catch$25@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	5
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,9
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin9:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, (%rbp)
	movq	%rcx, %rsi
	movq	8(%rcx), %rax
	movq	%rax, -8(%rbp)
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB9_2
	movq	(%rsi), %rcx
.Ltmp22:
	movq	%rcx, -16(%rbp)
	callq	*%rax
	nop
.Ltmp23:
.LBB9_2:
	movq	-8(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB9_3
	movq	(%rsi), %rcx
	movq	16(%rax), %r8
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB9_3:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,9
	.seh_endproc
	.def	"?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB9_4:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	48(%rdx), %rbp
	.seh_endprologue
	movq	-16(%rbp), %rcx
	movq	-8(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB9_6
	movq	-8(%rbp), %rax
	movq	16(%rax), %r8
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB9_6:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end9:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,9
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,7
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	48
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin9@IMGREL
	.long	-1
	.long	.Ltmp22@IMGREL
	.long	0
	.long	.Ltmp23@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,9

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,10
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin10:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
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
	subq	$144, %rsp
	.seh_stackalloc 144
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 8(%rbp)
	movq	(%rcx), %rdx
	testq	%rdx, %rdx
	je	.LBB10_1
	movq	8(%rcx), %r8
	movq	16(%rcx), %rax
	movq	$0, -88(%rbp)
	movq	%rdx, -80(%rbp)
	movq	%r8, -72(%rbp)
	movq	$0, -56(%rbp)
	movq	%rdx, -48(%rbp)
	movq	%r8, -40(%rbp)
	movl	$1, %ecx
	jmp	.LBB10_3
.LBB10_1:
	xorl	%ecx, %ecx
	xorl	%eax, %eax
.LBB10_3:
	movq	%rcx, -96(%rbp)
	movq	%rcx, -64(%rbp)
	movq	%rax, -32(%rbp)
	leaq	-24(%rbp), %rcx
	leaq	-96(%rbp), %rsi
	movq	%rsi, %rdx
	callq	_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	-24(%rbp), %rbx
	testq	%rbx, %rbx
	je	.LBB10_9
	leaq	-24(%rbp), %rdi
	.p2align	4
.LBB10_5:
	movq	-8(%rbp), %r14
	movq	%rsi, (%rbp)
	leaq	(%r14,%r14,2), %rax
	movq	360(%rbx,%rax,8), %rdx
	testq	%rdx, %rdx
	je	.LBB10_7
	leaq	(%rbx,%rax,8), %rax
	addq	$360, %rax
	movq	8(%rax), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB10_7:
.Ltmp24:
	shlq	$5, %r14
	addq	%r14, %rbx
	movq	%rbx, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp25:
	movq	%rdi, %rcx
	movq	%rsi, %rdx
	callq	_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	-24(%rbp), %rbx
	testq	%rbx, %rbx
	jne	.LBB10_5
.LBB10_9:
	.seh_startepilogue
	addq	$144, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,10
	.seh_endproc
	.def	"?dtor$10@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$10@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$10@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB10_10:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	%rbp, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNvXsy_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtBK_8IntoIterpppENtNtNtB4_3ops4drop4Drop4drop9DropGuardNtNtBQ_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtBQ_5alloc6GlobalEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end10:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,10
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,8
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	136
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$10@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin10@IMGREL
	.long	-1
	.long	.Ltmp24@IMGREL
	.long	0
	.long	.Ltmp25@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,10

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtNtNtCsaZOKLwlFm27_3std4sync6poison5mutex10MutexGuarduEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtNtNtCsaZOKLwlFm27_3std4sync6poison5mutex10MutexGuarduEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,11
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtNtNtCsaZOKLwlFm27_3std4sync6poison5mutex10MutexGuarduEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin11:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtNtNtCsaZOKLwlFm27_3std4sync6poison5mutex10MutexGuarduEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	testb	$1, %cl
	jne	.LBB11_4
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	jne	.LBB11_2
.LBB11_4:
	xorl	%eax, %eax
	xchgb	%al, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip)
	cmpb	$2, %al
	je	.LBB11_6
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	retq
.LBB11_6:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip), %rcx
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	rex64 jmpq	*__imp_WakeByAddressSingle(%rip)
.LBB11_2:
	callq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count17is_zero_slow_path
	testb	%al, %al
	jne	.LBB11_4
	movb	$1, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE+1(%rip)
	jmp	.LBB11_4
.Lfunc_end11:
	.seh_endproc

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNvXsy_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtBK_8IntoIterpppENtNtNtB4_3ops4drop4Drop4drop9DropGuardNtNtBQ_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtBQ_5alloc6GlobalEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNvXsy_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtBK_8IntoIterpppENtNtNtB4_3ops4drop4Drop4drop9DropGuardNtNtBQ_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtBQ_5alloc6GlobalEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,12
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNvXsy_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtBK_8IntoIterpppENtNtNtB4_3ops4drop4Drop4drop9DropGuardNtNtBQ_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtBQ_5alloc6GlobalEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin12:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNvXsy_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtBK_8IntoIterpppENtNtNtB4_3ops4drop4Drop4drop9DropGuardNtNtBQ_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtBQ_5alloc6GlobalEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
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
	movq	(%rcx), %rsi
	leaq	32(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	32(%rsp), %rbx
	testq	%rbx, %rbx
	je	.LBB12_5
	leaq	32(%rsp), %rdi
	jmp	.LBB12_2
	.p2align	4
.LBB12_4:
	shlq	$5, %r14
	addq	%r14, %rbx
	movq	%rbx, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rdi, %rcx
	movq	%rsi, %rdx
	callq	_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	32(%rsp), %rbx
	testq	%rbx, %rbx
	je	.LBB12_5
.LBB12_2:
	movq	48(%rsp), %r14
	leaq	(%r14,%r14,2), %rax
	movq	360(%rbx,%rax,8), %rdx
	testq	%rdx, %rdx
	je	.LBB12_4
	leaq	(%rbx,%rax,8), %rax
	addq	$360, %rax
	movq	8(%rax), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	jmp	.LBB12_4
.LBB12_5:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	.seh_endepilogue
	retq
.Lfunc_end12:
	.seh_endproc

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_,unique,13
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_:
.Lfunc_begin13:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$72, %rsp
	.seh_stackalloc 72
	leaq	64(%rsp), %rbp
	.seh_setframe %rbp, 64
	.seh_endprologue
	movq	$-2, (%rbp)
	movq	%rcx, %rsi
	movq	16(%rcx), %rdx
	testq	%rdx, %rdx
	je	.LBB13_2
	movq	24(%rsi), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB13_2:
	leaq	56(%rsi), %rax
	movq	%rax, -24(%rbp)
	movq	%rsi, -8(%rbp)
	leaq	80(%rsi), %rcx
.Ltmp26:
	movq	%rcx, -16(%rbp)
	callq	_RNvXNtNtCsaZOKLwlFm27_3std6thread9spawnhookNtB2_10SpawnHooksNtNtNtCsd9eshbB3hfz_4core3ops4drop4Drop4drop
	nop
.Ltmp27:
	movq	-16(%rbp), %rcx
	movq	(%rcx), %rax
	testq	%rax, %rax
	je	.LBB13_6
	lock		decq	(%rax)
	jne	.LBB13_6
	#MEMBARRIER
.Ltmp28:
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook9SpawnHookE9drop_slowBM_
	nop
.Ltmp29:
.LBB13_6:
.Ltmp30:
	movq	-24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp31:
	movq	-8(%rbp), %rcx
	movq	88(%rcx), %rax
	lock		decq	(%rax)
	jne	.LBB13_17
	addq	$88, %rcx
	#MEMBARRIER
	.seh_startepilogue
	addq	$72, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEE9drop_slowBN_
.LBB13_17:
	.seh_startepilogue
	addq	$72, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_,unique,13
	.seh_endproc
	.def	"?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA":
.seh_proc "?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA"
.LBB13_9:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-16(%rbp), %rax
	movq	%rax, %rcx
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB13_12
	lock		decq	(%rax)
	jne	.LBB13_12
	#MEMBARRIER
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook9SpawnHookE9drop_slowBM_
	nop
.LBB13_12:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_,unique,13
	.seh_endproc
	.def	"?dtor$13@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$13@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA":
.seh_proc "?dtor$13@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA"
.LBB13_13:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_,unique,13
	.seh_endproc
	.def	"?dtor$14@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$14@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA":
.seh_proc "?dtor$14@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA"
.LBB13_14:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-8(%rbp), %rax
	movq	88(%rax), %rax
	lock		decq	(%rax)
	jne	.LBB13_16
	movq	-8(%rbp), %rcx
	addq	$88, %rcx
	#MEMBARRIER
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEE9drop_slowBN_
	nop
.LBB13_16:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end13:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_,unique,13
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_,unique,9
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_:
	.long	429065506
	.long	3
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@IMGREL
	.long	0
	.long	0
	.long	5
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@IMGREL
	.long	64
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_:
	.long	-1
	.long	"?dtor$14@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA"@IMGREL
	.long	0
	.long	"?dtor$13@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA"@IMGREL
	.long	1
	.long	"?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_:
	.long	.Lfunc_begin13@IMGREL
	.long	-1
	.long	.Ltmp26@IMGREL
	.long	2
	.long	.Ltmp28@IMGREL
	.long	1
	.long	.Ltmp30@IMGREL
	.long	0
	.long	.Ltmp31@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_,unique,13

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std3env8VarErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std3env8VarErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,14
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std3env8VarErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin14:
	movq	(%rcx), %rdx
	testq	%rdx, %rdx
	jle	.LBB14_1
	movq	8(%rcx), %rcx
	movl	$1, %r8d
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB14_1:
	retq
.Lfunc_end14:

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process6OutputECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process6OutputECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,15
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process6OutputECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin15:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process6OutputECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$32, %rsp
	.seh_stackalloc 32
	.seh_endprologue
	movq	%rcx, %rsi
	movq	(%rcx), %rdx
	testq	%rdx, %rdx
	je	.LBB15_2
	movq	8(%rsi), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB15_2:
	movq	24(%rsi), %rdx
	testq	%rdx, %rdx
	je	.LBB15_3
	movq	32(%rsi), %rcx
	movl	$1, %r8d
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB15_3:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rsi
	.seh_endepilogue
	retq
.Lfunc_end15:
	.seh_endproc

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,16
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin16:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, (%rbp)
	movq	%rcx, %rdi
	movq	(%rcx), %rdx
	testq	%rdx, %rdx
	je	.LBB16_2
	movq	8(%rdi), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB16_2:
	movq	40(%rdi), %rsi
	movq	%rdi, -8(%rbp)
	movq	48(%rdi), %rdi
	testq	%rdi, %rdi
	je	.LBB16_7
	leaq	16(%rsi), %rbx
	jmp	.LBB16_5
	.p2align	4
.LBB16_4:
	addq	$40, %rbx
	decq	%rdi
	je	.LBB16_7
.LBB16_5:
	movq	-8(%rbx), %rdx
	testq	%rdx, %rdx
	je	.LBB16_4
	movq	(%rbx), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	jmp	.LBB16_4
.LBB16_7:
	movq	-8(%rbp), %rdi
	movq	32(%rdi), %rax
	testq	%rax, %rax
	je	.LBB16_9
	shlq	$3, %rax
	leaq	(%rax,%rax,4), %rdx
	movl	$8, %r8d
	movq	%rsi, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB16_9:
	movq	136(%rdi), %rcx
	movq	144(%rdi), %rbx
	testq	%rcx, %rcx
	setne	%dl
	movq	152(%rdi), %rsi
	testq	%rsi, %rsi
	setne	%al
	andb	%dl, %al
	cmpb	$1, %al
	jne	.LBB16_43
	movq	%rcx, %rax
	xorl	%ecx, %ecx
	movl	$984, %edi
	jmp	.LBB16_12
	.p2align	4
.LBB16_11:
	movb	$1, %dl
	movl	$0, %eax
	decq	%rsi
	je	.LBB16_51
.LBB16_12:
	testb	$1, %dl
	je	.LBB16_67
	testq	%rcx, %rcx
	je	.LBB16_16
	movzwl	978(%rcx), %edx
	cmpq	%rdx, %rbx
	jae	.LBB16_25
.LBB16_15:
	movq	%rbx, %r15
	movq	%rax, %rbx
	movq	%rcx, %r14
	jmp	.LBB16_27
	.p2align	4
.LBB16_16:
	testq	%rbx, %rbx
	je	.LBB16_21
	movq	%rbx, %rdx
	movq	%rax, %rcx
	andq	$7, %rdx
	je	.LBB16_22
	xorl	%r8d, %r8d
	.p2align	4
.LBB16_19:
	movq	984(%rcx), %rcx
	incq	%r8
	cmpq	%r8, %rdx
	jne	.LBB16_19
	movq	%rbx, %rax
	subq	%r8, %rax
	cmpq	$8, %rbx
	jae	.LBB16_23
	jmp	.LBB16_24
.LBB16_21:
	movq	%rax, %rcx
	jmp	.LBB16_24
.LBB16_22:
	movq	%rbx, %rax
	cmpq	$8, %rbx
	jb	.LBB16_24
	.p2align	4
.LBB16_23:
	movq	984(%rcx), %rcx
	movq	984(%rcx), %rcx
	movq	984(%rcx), %rcx
	movq	984(%rcx), %rcx
	movq	984(%rcx), %rcx
	movq	984(%rcx), %rcx
	movq	984(%rcx), %rcx
	movq	984(%rcx), %rcx
	addq	$-8, %rax
	jne	.LBB16_23
.LBB16_24:
	xorl	%ebx, %ebx
	xorl	%eax, %eax
	movzwl	978(%rcx), %edx
	cmpq	%rdx, %rbx
	jb	.LBB16_15
	.p2align	4
.LBB16_25:
	movq	352(%rcx), %r14
	testq	%r14, %r14
	je	.LBB16_66
	leaq	1(%rax), %rbx
	movzwl	976(%rcx), %r15d
	testq	%rax, %rax
	movl	$1080, %edx
	cmoveq	%rdi, %rdx
	movl	$8, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%r14, %rcx
	movq	%rbx, %rax
	cmpw	978(%r14), %r15w
	jae	.LBB16_25
.LBB16_27:
	testq	%rbx, %rbx
	je	.LBB16_32
	leaq	(%r14,%r15,8), %rax
	addq	$992, %rax
	movq	%rbx, %rdx
	andq	$7, %rdx
	je	.LBB16_33
	xorl	%r8d, %r8d
	.p2align	4
.LBB16_30:
	movq	(%rax), %rcx
	leaq	984(%rcx), %rax
	incq	%r8
	cmpq	%r8, %rdx
	jne	.LBB16_30
	movq	%rbx, %rdx
	subq	%r8, %rdx
	cmpq	$8, %rbx
	jb	.LBB16_36
	.p2align	4
.LBB16_35:
	movq	(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rcx
	leaq	984(%rcx), %rax
	addq	$-8, %rdx
	jne	.LBB16_35
.LBB16_36:
	xorl	%ebx, %ebx
	jmp	.LBB16_37
	.p2align	4
.LBB16_32:
	leaq	1(%r15), %rbx
	movq	%r14, %rcx
.LBB16_37:
	imulq	$56, %r15, %rax
	leaq	(%r14,%rax), %r13
	addq	$360, %r13
	movq	360(%r14,%rax), %rdx
	testq	%rdx, %rdx
	movq	%rcx, %r12
	je	.LBB16_39
	movq	8(%r13), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%r12, %rcx
.LBB16_39:
	movq	32(%r13), %rdx
	testq	%rdx, %rdx
	je	.LBB16_41
	movq	40(%r13), %rcx
	addq	%rdx, %rdx
	movl	$2, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%r12, %rcx
.LBB16_41:
	shlq	$5, %r15
	addq	%r15, %r14
	movq	(%r14), %rdx
	testq	%rdx, %rdx
	jle	.LBB16_11
	movq	8(%r14), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%r12, %rcx
	jmp	.LBB16_11
	.p2align	4
.LBB16_33:
	movq	%rbx, %rdx
	cmpq	$8, %rbx
	jb	.LBB16_36
	jmp	.LBB16_35
.LBB16_43:
	testq	%rcx, %rcx
	je	.LBB16_57
	testq	%rbx, %rbx
	je	.LBB16_51
	leaq	-1(%rbx), %rax
	movq	%rbx, %rdx
	andq	$7, %rdx
	je	.LBB16_49
	xorl	%r8d, %r8d
	.p2align	4
.LBB16_47:
	movq	984(%rcx), %rcx
	incq	%r8
	cmpq	%r8, %rdx
	jne	.LBB16_47
	subq	%r8, %rbx
.LBB16_49:
	cmpq	$7, %rax
	jb	.LBB16_51
	.p2align	4
.LBB16_50:
	movq	984(%rcx), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rax
	movq	984(%rax), %rcx
	addq	$-8, %rbx
	jne	.LBB16_50
.LBB16_51:
	movq	352(%rcx), %rax
	testq	%rax, %rax
	je	.LBB16_55
	xorl	%esi, %esi
	movl	$984, %edi
	.p2align	4
.LBB16_53:
	movq	%rax, %rbx
	subq	$1, %rsi
	movl	$1080, %edx
	cmovbq	%rdi, %rdx
	movl	$8, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%rbx, %rcx
	movq	352(%rbx), %rax
	testq	%rax, %rax
	jne	.LBB16_53
	testq	%rsi, %rsi
	movl	$984, %eax
	movl	$1080, %edx
	cmoveq	%rax, %rdx
	jmp	.LBB16_56
.LBB16_55:
	movl	$984, %edx
.LBB16_56:
	movl	$8, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	-8(%rbp), %rdi
.LBB16_57:
	movq	56(%rdi), %rdx
	testq	%rdx, %rdx
	jle	.LBB16_59
	movq	64(%rdi), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB16_59:
	movl	88(%rdi), %eax
	addl	$-4, %eax
	cmpl	$-6, %eax
	ja	.LBB16_60
	movq	96(%rdi), %rcx
	callq	*__imp_CloseHandle(%rip)
	movl	104(%rdi), %eax
	addl	$-4, %eax
	cmpl	$-6, %eax
	jbe	.LBB16_64
.LBB16_61:
	movl	120(%rdi), %eax
	addl	$-4, %eax
	cmpl	$-6, %eax
	ja	.LBB16_65
.LBB16_62:
	movq	128(%rdi), %rcx
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	rex64 jmpq	*__imp_CloseHandle(%rip)
.LBB16_60:
	movl	104(%rdi), %eax
	addl	$-4, %eax
	cmpl	$-6, %eax
	ja	.LBB16_61
.LBB16_64:
	movq	112(%rdi), %rcx
	callq	*__imp_CloseHandle(%rip)
	movl	120(%rdi), %eax
	addl	$-4, %eax
	cmpl	$-6, %eax
	jbe	.LBB16_62
.LBB16_65:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB16_66:
	testq	%rax, %rax
	movl	$984, %eax
	movl	$1080, %edx
	cmoveq	%rax, %rdx
	movl	$8, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.Ltmp34:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.16(%rip), %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13unwrap_failed
	nop
.Ltmp35:
	jmp	.LBB16_68
.LBB16_67:
.Ltmp32:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.8(%rip), %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13unwrap_failed
	nop
.Ltmp33:
.LBB16_68:
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,16
	.seh_endproc
	.def	"?dtor$69@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$69@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$69@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB16_69:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	48(%rdx), %rbp
	.seh_endprologue
	movq	-8(%rbp), %rax
	movq	56(%rax), %rdx
	testq	%rdx, %rdx
	jle	.LBB16_71
	movq	-8(%rbp), %rax
	movq	64(%rax), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB16_71:
	movq	-8(%rbp), %rax
	movl	88(%rax), %eax
	addl	$-4, %eax
	cmpl	$-5, %eax
	jae	.LBB16_73
	movq	-8(%rbp), %rax
	movq	96(%rax), %rcx
	callq	*__imp_CloseHandle(%rip)
.LBB16_73:
	movq	-8(%rbp), %rax
	movl	104(%rax), %eax
	addl	$-4, %eax
	cmpl	$-5, %eax
	jae	.LBB16_75
	movq	-8(%rbp), %rax
	movq	112(%rax), %rcx
	callq	*__imp_CloseHandle(%rip)
.LBB16_75:
	movq	-8(%rbp), %rax
	movl	120(%rax), %eax
	addl	$-4, %eax
	cmpl	$-5, %eax
	jae	.LBB16_77
	movq	-8(%rbp), %rax
	movq	128(%rax), %rcx
	callq	*__imp_CloseHandle(%rip)
	nop
.LBB16_77:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,16
	.seh_endproc
	.def	"?dtor$78@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$78@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$78@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB16_78:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	48(%rdx), %rbp
	.seh_endprologue
	ud2
.Lfunc_end16:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,16
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,10
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	2
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	4
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	48
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$78@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	-1
	.long	"?dtor$69@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin16@IMGREL
	.long	-1
	.long	.Ltmp34@IMGREL
	.long	0
	.long	.Ltmp32@IMGREL
	.long	1
	.long	.Ltmp33@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,16

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,17
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin17:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$48, %rsp
	.seh_stackalloc 48
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, -8(%rbp)
	movq	(%rcx), %rcx
	movq	(%rcx), %rax
	cmpq	$1, %rax
	movq	%rcx, -16(%rbp)
	je	.LBB17_5
	testq	%rax, %rax
	jne	.LBB17_6
	movq	-16(%rbp), %rax
	movq	16(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB17_6
	movq	-16(%rbp), %rax
	movq	8(%rax), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	jmp	.LBB17_6
.LBB17_5:
	addq	$8, %rcx
.Ltmp36:
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp37:
.LBB17_6:
	movl	$40, %edx
	movl	$8, %r8d
	movq	-16(%rbp), %rcx
	.seh_startepilogue
	addq	$48, %rsp
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,17
	.seh_endproc
	.def	"?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB17_4:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	48(%rdx), %rbp
	.seh_endprologue
	movl	$40, %edx
	movl	$8, %r8d
	movq	-16(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end17:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,17
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,11
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	40
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$4@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin17@IMGREL
	.long	-1
	.long	.Ltmp36@IMGREL
	.long	0
	.long	.Ltmp37@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,17

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error9ErrorCodeECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error9ErrorCodeECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,18
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error9ErrorCodeECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin18:
	movq	(%rcx), %rax
	testq	%rax, %rax
	je	.LBB18_3
	cmpl	$1, %eax
	jne	.LBB18_2
	addq	$8, %rcx
	jmp	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB18_3:
	movq	16(%rcx), %rdx
	testq	%rdx, %rdx
	je	.LBB18_2
	movq	8(%rcx), %rcx
	movl	$1, %r8d
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB18_2:
	retq
.Lfunc_end18:

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,19
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin19:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
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
	subq	$176, %rsp
	.seh_stackalloc 176
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 40(%rbp)
	movzbl	(%rcx), %eax
	cmpl	$3, %eax
	jae	.LBB19_1
.LBB19_12:
	.seh_startepilogue
	addq	$176, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.LBB19_1:
	je	.LBB19_13
	cmpl	$4, %eax
	jne	.LBB19_3
	movq	16(%rcx), %rax
	movq	%rcx, 16(%rbp)
	movq	24(%rcx), %rcx
	movq	%rcx, 8(%rbp)
	movq	$0, 24(%rbp)
	movq	%rax, 32(%rbp)
	movq	%rax, %rcx
	.p2align	4
.LBB19_17:
	movq	24(%rbp), %rax
	cmpq	%rax, 8(%rbp)
	je	.LBB19_26
	incq	%rax
	movq	%rax, 24(%rbp)
	leaq	32(%rcx), %rsi
.Ltmp38:
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp39:
	movq	%rsi, %rcx
	jmp	.LBB19_17
.LBB19_13:
	movq	8(%rcx), %rdx
	testq	%rdx, %rdx
	je	.LBB19_12
	movq	16(%rcx), %rcx
	movl	$1, %r8d
	jmp	.LBB19_15
.LBB19_3:
	movq	8(%rcx), %r8
	testq	%r8, %r8
	je	.LBB19_4
	movq	16(%rcx), %rdx
	movq	24(%rcx), %rax
	movq	$0, -88(%rbp)
	movq	%r8, -80(%rbp)
	movq	%rdx, -72(%rbp)
	movq	$0, -56(%rbp)
	movq	%r8, -48(%rbp)
	movq	%rdx, -40(%rbp)
	movl	$1, %ecx
	jmp	.LBB19_6
.LBB19_26:
	movq	16(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB19_12
	shlq	$5, %rdx
	movl	$8, %r8d
	movq	32(%rbp), %rcx
.LBB19_15:
	.seh_startepilogue
	addq	$176, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB19_4:
	xorl	%ecx, %ecx
	xorl	%eax, %eax
.LBB19_6:
	movq	%rcx, -96(%rbp)
	movq	%rcx, -64(%rbp)
	movq	%rax, -32(%rbp)
	leaq	-24(%rbp), %rcx
	leaq	-96(%rbp), %rsi
	movq	%rsi, %rdx
	callq	_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	-24(%rbp), %rbx
	testq	%rbx, %rbx
	je	.LBB19_12
	leaq	-24(%rbp), %rdi
	.p2align	4
.LBB19_8:
	movq	-8(%rbp), %r14
	movq	%rsi, (%rbp)
	leaq	(%r14,%r14,2), %rax
	movq	360(%rbx,%rax,8), %rdx
	testq	%rdx, %rdx
	je	.LBB19_10
	leaq	(%rbx,%rax,8), %rax
	addq	$360, %rax
	movq	8(%rax), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB19_10:
.Ltmp40:
	shlq	$5, %r14
	addq	%r14, %rbx
	movq	%rbx, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp41:
	movq	%rdi, %rcx
	movq	%rsi, %rdx
	callq	_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	-24(%rbp), %rbx
	testq	%rbx, %rbx
	jne	.LBB19_8
	jmp	.LBB19_12
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,19
	.seh_endproc
	.def	"?dtor$19@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$19@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$19@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB19_19:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	32(%rbp), %rsi
	movq	24(%rbp), %rdi
	cmpq	8(%rbp), %rdi
	je	.LBB19_22
	.p2align	4
.LBB19_21:
	movq	%rdi, %rcx
	shlq	$5, %rcx
	addq	%rsi, %rcx
	incq	%rdi
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	cmpq	8(%rbp), %rdi
	jne	.LBB19_21
.LBB19_22:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,19
	.seh_endproc
	.def	"?dtor$23@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$23@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$23@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB19_23:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	16(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB19_25
	shlq	$5, %rdx
	movl	$8, %r8d
	movq	32(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB19_25:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,19
	.seh_endproc
	.def	"?dtor$28@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$28@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$28@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB19_28:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	%rbp, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNvXsy_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtBK_8IntoIterpppENtNtNtB4_3ops4drop4Drop4drop9DropGuardNtNtBQ_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtBQ_5alloc6GlobalEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end19:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,19
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,12
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	3
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	5
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	168
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$28@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	-1
	.long	"?dtor$23@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	1
	.long	"?dtor$19@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin19@IMGREL
	.long	-1
	.long	.Ltmp38@IMGREL
	.long	2
	.long	.Ltmp39@IMGREL
	.long	-1
	.long	.Ltmp40@IMGREL
	.long	0
	.long	.Ltmp41@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,19

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,20
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin20:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$64, %rsp
	.seh_stackalloc 64
	leaq	64(%rsp), %rbp
	.seh_setframe %rbp, 64
	.seh_endprologue
	movq	$-2, -8(%rbp)
	movq	(%rcx), %rax
	movl	%eax, %ecx
	andl	$3, %ecx
	leal	-2(%rcx), %edx
	cmpl	$2, %edx
	jb	.LBB20_10
	testq	%rcx, %rcx
	jne	.LBB20_2
.LBB20_10:
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
.LBB20_2:
	leaq	-1(%rax), %rcx
	movq	%rcx, -32(%rbp)
	movq	-1(%rax), %rcx
	movq	%rcx, -24(%rbp)
	movq	7(%rax), %rax
	movq	%rax, -16(%rbp)
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB20_4
.Ltmp42:
	movq	-24(%rbp), %rcx
	callq	*%rax
	nop
.Ltmp43:
.LBB20_4:
	movq	-24(%rbp), %rcx
	movq	-16(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB20_6
	movq	16(%rax), %r8
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB20_6:
	movl	$24, %edx
	movl	$8, %r8d
	movq	-32(%rbp), %rcx
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,20
	.seh_endproc
	.def	"?dtor$7@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$7@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$7@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB20_7:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-24(%rbp), %rcx
	movq	-16(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB20_9
	movq	-16(%rbp), %rax
	movq	16(%rax), %r8
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB20_9:
	movl	$24, %edx
	movl	$8, %r8d
	movq	-32(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end20:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,20
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,13
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	56
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$7@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin20@IMGREL
	.long	-1
	.long	.Ltmp42@IMGREL
	.long	0
	.long	.Ltmp43@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,20

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,21
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin21:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, (%rbp)
	movq	%rcx, %rsi
	movq	(%rcx), %rax
	lock		decq	(%rax)
	jne	.LBB21_2
	#MEMBARRIER
	movq	%rsi, %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread6thread5InnerNtNtBM_5alloc6SystemE9drop_slowBM_
.LBB21_2:
	movq	8(%rsi), %rax
	movq	%rax, -16(%rbp)
	movq	16(%rsi), %rax
	movq	%rax, -8(%rbp)
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB21_4
.Ltmp44:
	movq	-16(%rbp), %rcx
	callq	*%rax
	nop
.Ltmp45:
.LBB21_4:
	movq	-16(%rbp), %rcx
	movq	-8(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB21_9
	movq	16(%rax), %r8
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB21_9:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,21
	.seh_endproc
	.def	"?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB21_6:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	48(%rdx), %rbp
	.seh_endprologue
	movq	-16(%rbp), %rcx
	movq	-8(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB21_8
	movq	-8(%rbp), %rax
	movq	16(%rax), %r8
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB21_8:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end21:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,21
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,14
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	48
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$6@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin21@IMGREL
	.long	-1
	.long	.Ltmp44@IMGREL
	.long	0
	.long	.Ltmp45@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,21

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,22
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin22:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$64, %rsp
	.seh_stackalloc 64
	leaq	64(%rsp), %rbp
	.seh_setframe %rbp, 64
	.seh_endprologue
	movq	$-2, -8(%rbp)
	movq	%rcx, -24(%rbp)
	addq	$24, %rcx
.Ltmp46:
	movq	%rcx, -16(%rbp)
	callq	_RNvXNtNtCsaZOKLwlFm27_3std6thread9spawnhookNtB2_10SpawnHooksNtNtNtCsd9eshbB3hfz_4core3ops4drop4Drop4drop
	nop
.Ltmp47:
	movq	-16(%rbp), %rcx
	movq	(%rcx), %rax
	testq	%rax, %rax
	je	.LBB22_4
	lock		decq	(%rax)
	jne	.LBB22_4
	#MEMBARRIER
.Ltmp48:
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook9SpawnHookE9drop_slowBM_
	nop
.Ltmp49:
.LBB22_4:
	movq	-24(%rbp), %rcx
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbp
	.seh_endepilogue
	jmp	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handlerdata
	.long	$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,22
	.seh_endproc
	.def	"?dtor$5@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$5@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$5@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB22_5:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-16(%rbp), %rax
	movq	%rax, %rcx
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB22_8
	lock		decq	(%rax)
	jne	.LBB22_8
	#MEMBARRIER
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook9SpawnHookE9drop_slowBM_
	nop
.LBB22_8:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,22
	.seh_endproc
	.def	"?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB22_9:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	64(%rdx), %rbp
	.seh_endprologue
	movq	-24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecINtNtBG_5boxed3BoxDINtNtNtB4_3ops8function6FnOnceuEp6OutputuNtNtB4_6marker4SendEL_EEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end22:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,22
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,15
	.p2align	2, 0x0
$cppxdata$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	2
	.long	$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	4
	.long	$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	56
	.long	0
	.long	1
$stateUnwindMap$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$9@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	0
	.long	"?dtor$5@?0?_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin22@IMGREL
	.long	-1
	.long	.Ltmp46@IMGREL
	.long	1
	.long	.Ltmp48@IMGREL
	.long	0
	.long	.Ltmp49@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,22

	.def	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueTNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueTNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,23
	.p2align	4
_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueTNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin23:
.seh_proc _RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueTNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$32, %rsp
	.seh_stackalloc 32
	.seh_endprologue
	movq	(%rcx), %rdx
	testq	%rdx, %rdx
	je	.LBB23_2
	movq	8(%rcx), %rax
	movl	$1, %r8d
	movq	%rcx, %rsi
	movq	%rax, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%rsi, %rcx
.LBB23_2:
	addq	$24, %rcx
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rsi
	.seh_endepilogue
	jmp	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.Lfunc_end23:
	.seh_endproc

	.def	_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,24
	.p2align	4
_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin24:
.seh_proc _RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	callq	*%rcx
	nop
	#APP
	#NO_APP
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	retq
.Lfunc_end24:
	.seh_endproc

	.def	_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNCNCINvNtNtB6_6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_000uEB1Z_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNCNCINvNtNtB6_6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_000uEB1Z_,unique,25
	.p2align	4
_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNCNCINvNtNtB6_6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_000uEB1Z_:
.Lfunc_begin25:
.seh_proc _RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNCNCINvNtNtB6_6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_000uEB1Z_
	subq	$72, %rsp
	.seh_stackalloc 72
	.seh_endprologue
	movups	(%rcx), %xmm0
	movups	16(%rcx), %xmm1
	movaps	%xmm1, 48(%rsp)
	movaps	%xmm0, 32(%rsp)
	leaq	32(%rsp), %rcx
	callq	_RNvMs_NtNtCsaZOKLwlFm27_3std6thread9spawnhookNtB4_15ChildSpawnHooks3run
	nop
	#APP
	#NO_APP
	.seh_startepilogue
	addq	$72, %rsp
	.seh_endepilogue
	retq
.Lfunc_end25:
	.seh_endproc

	.def	_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_,unique,26
	.p2align	4
_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_:
.Lfunc_begin26:
.seh_proc _RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$168, %rsp
	.seh_stackalloc 168
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 32(%rbp)
	movq	%rcx, %rdi
	movb	$0, 31(%rbp)
	leaq	31(%rbp), %rax
	movq	%rax, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack5ENTRY.0(%rip)
	movq	$0, -24(%rbp)
	movq	$0, -8(%rbp)
	leaq	-24(%rbp), %rcx
	leaq	-8(%rbp), %rdx
	callq	*__imp_GetCurrentThreadStackLimits(%rip)
	movq	-24(%rbp), %rax
	movq	-8(%rbp), %rcx
	movq	%rax, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3LOW.0(%rip)
	movq	%rcx, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4HIGH.0(%rip)
	movl	_tls_index(%rip), %ecx
	movq	%gs:88, %rdx
	movq	(%rdx,%rcx,8), %rsi
	movq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rsi), %rcx
	incq	%rcx
	xorl	%r15d, %r15d
	cmpq	$2, %rcx
	setb	%r12b
	movl	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9bootstrap8registry8REGISTRY+520(%rip), %ecx
	movq	%rcx, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM1.0(%rip)
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback17LOCK_ACQUISITIONS.llvm.2560762130558453100(%rip), %rcx
	movq	%rcx, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB0.0(%rip)
	leaq	16(%rdi), %rdx
	leaq	40(%rdi), %rcx
	movq	%rdx, -48(%rbp)
	movq	%rdx, -24(%rbp)
	movq	%rcx, -16(%rbp)
	movq	$0, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4PREV(%rip)
	movq	$0, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack10STRIDE_ACT.0(%rip)
	cmpb	$0, (%rdi)
	movq	%rdi, -56(%rbp)
	je	.LBB26_7
	movq	8(%rdi), %rcx
	movq	%rax, (%rbp)
	movq	%rcx, 8(%rbp)
	movq	$1, -8(%rbp)
.Ltmp52:
	leaq	-8(%rbp), %rdx
	leaq	-24(%rbp), %r8
	movl	$100000, %ecx
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3eat
	nop
.Ltmp53:
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4DEEP.0(%rip), %rax
	movq	%rax, -8(%rbp)
	testq	%rax, %rax
	jne	.LBB26_8
.Ltmp74:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.26(%rip), %rax
	movq	%rax, 40(%rsp)
	movq	$53, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.24(%rip), %r8
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.25(%rip), %r9
	leaq	-8(%rbp), %rdx
	movb	$1, %cl
	callq	_RINvNtCsd9eshbB3hfz_4core9panicking13assert_failedjjEB4_
	nop
.Ltmp75:
	jmp	.LBB26_62
.LBB26_7:
	movq	48(%rdi), %rcx
	movq	$0, -8(%rbp)
.Ltmp50:
	leaq	-8(%rbp), %rdx
	leaq	-24(%rbp), %r8
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3eat
	nop
.Ltmp51:
.LBB26_8:
	leaq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rsi), %r13
	xorl	%eax, %eax
	xchgq	%rax, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3PTR(%rip)
	testq	%rax, %rax
	je	.LBB26_13
	xorl	%r9d, %r9d
	xchgq	%r9, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4SIZE(%rip)
	movabsq	$9223372036854775793, %rcx
	xorl	%r8d, %r8d
	cmpq	%rcx, %r9
	setb	%dl
	jae	.LBB26_10
	movq	(%r13), %rcx
	leaq	-1(%rcx), %r10
	cmpq	$-2, %r10
	jae	.LBB26_12
	movb	%dl, %r8b
	shll	$4, %r8d
	movq	%rax, %rdx
	andq	$-4194304, %rdx
	cmpq	%rdx, 304(%rcx)
	jne	.LBB26_19
	movq	312(%rcx), %rsi
	jmp	.LBB26_30
.LBB26_19:
	movq	%rdx, %r11
	shrq	$22, %r11
	movl	%r11d, %esi
	andl	$15, %esi
	shll	$4, %esi
	leaq	(%rcx,%rsi), %r10
	addq	$48, %r10
	cmpq	%rdx, 48(%rcx,%rsi)
	jne	.LBB26_20
	movq	8(%r10), %rsi
.LBB26_30:
	movq	%rax, %rbx
	subq	%rsi, %rbx
	movzbl	28(%rsi), %r14d
	movl	$4194304, %edx
	cmpl	$2, %r14d
	jne	.LBB26_32
	movq	56(%rsi), %rdx
.LBB26_32:
	cmpq	%rdx, %rbx
	jae	.LBB26_24
	leaq	(%rsi,%rbx), %rdx
	cmpq	$17, %r9
	movl	$16, %r10d
	cmovaeq	%r9, %r10
	decq	%r10
	cmpq	$258751, %r10
	jbe	.LBB26_34
	cmpb	$2, %r14b
	jne	.LBB26_13
.Ltmp68:
	movq	%rsi, %r8
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core10alloc_core3mem8mem_implNtNtB6_15alloc_core_impl9AllocCore21dealloc_large_at_base
	nop
.Ltmp69:
	jmp	.LBB26_13
.LBB26_20:
	movq	%rdx, %rsi
	shrq	$35, %rsi
	movq	%rdx, %rdi
	shrq	$48, %rdi
	xorl	%edi, %esi
	xorl	%esi, %r11d
	andl	$8191, %r11d
	movq	320(%rcx), %rdi
	movq	%r11, %rbx
	.p2align	4
.LBB26_21:
	movq	(%rdi,%rbx,8), %rsi
	testq	%rsi, %rsi
	je	.LBB26_24
	movq	%rsi, %r14
	andq	$-4194304, %r14
	cmpq	%rdx, %r14
	je	.LBB26_25
	incl	%ebx
	andl	$8191, %ebx
	cmpq	%r11, %rbx
	jne	.LBB26_21
	jmp	.LBB26_24
.LBB26_34:
	shrq	$4, %r10
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes10SIZE2CLASS(%rip), %r11
	movzbl	(%r10,%r11), %edi
	cmpq	$49, %rdi
	jae	.LBB26_54
	movl	%edi, %r10d
	shll	$7, %r10d
	leaq	(%r10,%rdi,8), %r11
	movzbl	1192(%rcx,%r11), %r10d
	testl	%r14d, %r14d
	je	.LBB26_36
	cmpl	$2, %r14d
	je	.LBB26_55
	cmpl	$1, %r14d
	jne	.LBB26_56
	movl	$270336, %r8d
	movl	%ebx, %eax
	cmpq	%r8, %rax
	jb	.LBB26_56
	jmp	.LBB26_43
.LBB26_25:
	movq	%rsi, %r11
	andq	$4194303, %r11
	je	.LBB26_28
	leaq	-1(%r11), %rsi
	movl	344(%rcx), %edi
	cmpq	%rdi, %rsi
	jae	.LBB26_24
	movq	32(%rcx), %rsi
	movq	-8(%rsi,%r11,8), %rsi
	testq	%rsi, %rsi
	je	.LBB26_24
.LBB26_28:
	movq	%rdx, (%r10)
	movq	%rsi, 8(%r10)
	cmpq	%rdx, %rsi
	jne	.LBB26_30
	movq	%rdx, 304(%rcx)
	movq	%rsi, 312(%rcx)
	jmp	.LBB26_30
.LBB26_24:
.Ltmp56:
	movq	%rax, %rcx
	movq	%r8, %rdx
	movq	%r9, %r8
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry17heap_core_xthread7routingNtNtNtB6_9heap_core4core8HeapCore15publish_foreign
	nop
.Ltmp57:
.LBB26_13:
	movq	(%r13), %rcx
	decq	%rcx
	xorl	%eax, %eax
	cmpq	$-2, %rcx
	setb	%cl
	movl	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9bootstrap8registry8REGISTRY+520(%rip), %edx
	movq	%rdx, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM2.0(%rip)
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback17LOCK_ACQUISITIONS.llvm.2560762130558453100(%rip), %rdx
	movq	%rdx, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB1.0(%rip)
	movq	-56(%rbp), %r9
	cmpq	$8, 32(%r9)
	jne	.LBB26_15
	movq	24(%r9), %rdx
	movabsq	$8749489599857058149, %r8
	cmpq	%r8, (%rdx)
	je	.LBB26_63
.LBB26_15:
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB1.0(%rip), %rdx
	movq	%rdx, -72(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB0.0(%rip), %r8
	movq	%r8, -8(%rbp)
	cmpq	%r8, %rdx
	jne	.LBB26_16
.LBB26_63:
	movb	%r12b, %r15b
	movb	%cl, %al
	addl	%eax, %eax
	orq	%r15, %rax
	movq	%rax, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6RESULT.0(%rip)
	movq	-48(%rbp), %rax
	movq	(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB26_65
	movq	24(%r9), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB26_65:
	#APP
	#NO_APP
	.seh_startepilogue
	addq	$168, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB26_55:
	movq	48(%rsi), %r10
	addq	%rsi, %r10
	movq	8(%rsi), %r11
	setb	%r11b
	cmpq	%rax, %r10
	setne	%al
	orb	%r11b, %al
	jne	.LBB26_56
.Ltmp60:
	movq	%rsi, 32(%rsp)
	callq	_RNvNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core4free16dealloc_own_base19route_to_large_free
	nop
.Ltmp61:
	jmp	.LBB26_13
.LBB26_36:
	movl	$389120, %r8d
	movl	%ebx, %eax
	cmpq	%r8, %rax
	jae	.LBB26_43
.LBB26_56:
	lock		incq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core10alloc_core8counters18LOCAL_REJECT_FREES(%rip)
	jmp	.LBB26_13
.LBB26_43:
	testb	$15, %bl
	jne	.LBB26_13
	shrl	$4, %ebx
	movzbl	5512(%rsi,%rbx), %r14d
	leal	1(%rdi), %r8d
	cmpb	%r8b, %r14b
	jne	.LBB26_51
	cmpq	(%rsi), %rax
	jae	.LBB26_56
	addq	%rcx, %r11
	addq	$1192, %r11
	movzbl	%r10b, %r9d
	leaq	_RNvNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core5state6tcache13FREE_PARK_CAP(%rip), %rax
	cmpb	(%rdi,%rax), %r9b
	jae	.LBB26_47
	orb	$64, %r14b
	movb	%r14b, 5512(%rsi,%rbx)
	andl	$15, %r9d
	movq	%rdx, 8(%r11,%r9,8)
	incb	%r10b
	movb	%r10b, (%r11)
	jmp	.LBB26_13
.LBB26_51:
	testb	%r14b, %r14b
	je	.LBB26_53
	cmpb	$64, %r14b
	jb	.LBB26_13
	jmp	.LBB26_56
.LBB26_53:
	cmpq	(%rsi), %rax
	jb	.LBB26_13
	jmp	.LBB26_56
.LBB26_47:
	cmpb	$25, %dil
	movq	%rdx, -40(%rbp)
	movq	%r11, -32(%rbp)
	jbe	.LBB26_59
	leaq	-1(%r9), %rax
	cmpq	$16, %rax
	jae	.LBB26_61
	movq	(%r11,%r9,8), %rax
	movq	%rax, -8(%rbp)
.Ltmp66:
	leaq	-8(%rbp), %r8
	movq	%r9, 16(%rbp)
	movl	$1, %r9d
	movq	%rdi, %rdx
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small25alloc_core_small_magazineNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore20flush_class_internal
	nop
.Ltmp67:
	movq	-40(%rbp), %rax
	movq	-32(%rbp), %rcx
	movq	16(%rbp), %rdx
	movq	%rax, (%rcx,%rdx,8)
	orb	$64, %r14b
	movb	%r14b, 5512(%rsi,%rbx)
	jmp	.LBB26_13
.LBB26_59:
	leaq	8(%r11), %r8
.Ltmp62:
	movl	$8, %r9d
	movq	%rcx, 16(%rbp)
	movq	16(%rbp), %rcx
	movq	%rdi, %rdx
	movq	%r8, -64(%rbp)
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small25alloc_core_small_magazineNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore20flush_class_internal
	movq	-40(%rbp), %rax
	movq	16(%rbp), %rcx
.Ltmp63:
	movl	%edi, %edx
	shll	$7, %edx
	leaq	(%rdx,%rdi,8), %rdx
	movups	1264(%rcx,%rdx), %xmm0
	movups	1280(%rcx,%rdx), %xmm1
	movups	1296(%rcx,%rdx), %xmm2
	movups	1312(%rcx,%rdx), %xmm3
	movq	-64(%rbp), %rcx
	movups	%xmm3, 48(%rcx)
	movups	%xmm2, 32(%rcx)
	movups	%xmm1, 16(%rcx)
	movups	%xmm0, (%rcx)
	orb	$64, %r14b
	movb	%r14b, 5512(%rsi,%rbx)
	movq	-32(%rbp), %rcx
	movq	%rax, 72(%rcx)
	movb	$9, (%rcx)
	jmp	.LBB26_13
.LBB26_12:
.Ltmp54:
	movl	$16, %edx
	movq	%rax, %rcx
	movq	%r9, %r8
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry17heap_core_xthread7routingNtNtNtB6_9heap_core4core8HeapCore15publish_foreign
	nop
.Ltmp55:
	jmp	.LBB26_13
.LBB26_16:
.Ltmp72:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.30(%rip), %rax
	movq	%rax, 40(%rsp)
	movq	$137, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.29(%rip), %r9
	leaq	-72(%rbp), %rdx
	leaq	-8(%rbp), %r8
	xorl	%ecx, %ecx
	callq	_RINvNtCsd9eshbB3hfz_4core9panicking13assert_failedjjEB4_
	nop
.Ltmp73:
	jmp	.LBB26_62
.LBB26_10:
.Ltmp70:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.28(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.27(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.144(%rip), %r9
	leaq	30(%rbp), %r8
	movl	$6, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp71:
	jmp	.LBB26_62
.LBB26_54:
.Ltmp58:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.155(%rip), %r8
	movl	$49, %edx
	movq	%rdi, %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	nop
.Ltmp59:
	jmp	.LBB26_62
.LBB26_61:
.Ltmp64:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.156(%rip), %r8
	movl	$16, %edx
	movq	%rax, %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	nop
.Ltmp65:
.LBB26_62:
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_@IMGREL
	.section	.text,"xr",one_only,_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_,unique,26
	.seh_endproc
	.def	"?dtor$4@?0?_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$4@?0?_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_@4HA":
.seh_proc "?dtor$4@?0?_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_@4HA"
.LBB26_4:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	-48(%rbp), %rax
	movq	(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB26_6
	movq	-56(%rbp), %rax
	movq	24(%rax), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB26_6:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end26:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_,unique,26
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_,unique,16
	.p2align	2, 0x0
$cppxdata$_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_@IMGREL
	.long	0
	.long	0
	.long	5
	.long	$ip2state$_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_@IMGREL
	.long	160
	.long	0
	.long	1
$stateUnwindMap$_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_:
	.long	-1
	.long	"?dtor$4@?0?_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_@4HA"@IMGREL
$ip2state$_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_:
	.long	.Lfunc_begin26@IMGREL
	.long	-1
	.long	.Ltmp52@IMGREL
	.long	0
	.long	.Ltmp57@IMGREL
	.long	-1
	.long	.Ltmp60@IMGREL
	.long	0
	.long	.Ltmp65@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_,unique,26

	.def	_RINvNvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB8_9MapAccesspENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess13next_key_seed12has_next_keyNtNtBa_4read7StrReadECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvNvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB8_9MapAccesspENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess13next_key_seed12has_next_keyNtNtBa_4read7StrReadECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,27
	.p2align	4
_RINvNvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB8_9MapAccesspENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess13next_key_seed12has_next_keyNtNtBa_4read7StrReadECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin27:
.seh_proc _RINvNvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB8_9MapAccesspENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess13next_key_seed12has_next_keyNtNtBa_4read7StrReadECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	subq	$56, %rsp
	.seh_stackalloc 56
	.seh_endprologue
	movq	(%rdx), %rax
	movq	32(%rax), %r11
	movq	40(%rax), %r8
	cmpq	%r11, %r8
	jae	.LBB27_5
	movq	24(%rax), %r9
	incq	%r8
	movq	%r11, %rsi
	negq	%rsi
	movabsq	$4294977024, %r10
	.p2align	4
.LBB27_2:
	movzbl	-1(%r9,%r8), %edi
	cmpq	$32, %rdi
	ja	.LBB27_8
	btq	%rdi, %r10
	jae	.LBB27_8
	movq	%r8, 40(%rax)
	leaq	(%rsi,%r8), %rdi
	incq	%rdi
	incq	%r8
	cmpq	$1, %rdi
	jne	.LBB27_2
.LBB27_5:
	movq	$3, 32(%rsp)
.LBB27_6:
	leaq	32(%rsp), %rdx
	movq	%rcx, %rsi
.LBB27_7:
	movq	%rax, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rsi, %rcx
	movq	%rax, 8(%rsi)
	movb	$1, %al
	jmp	.LBB27_23
.LBB27_8:
	cmpl	$125, %edi
	jne	.LBB27_10
	movb	$0, 1(%rcx)
	jmp	.LBB27_22
.LBB27_10:
	cmpb	$0, 8(%rdx)
	je	.LBB27_13
	movb	$0, 8(%rdx)
	cmpb	$34, %dil
	je	.LBB27_21
	movq	$17, 32(%rsp)
	jmp	.LBB27_6
.LBB27_13:
	cmpb	$44, %dil
	jne	.LBB27_26
	movq	%r8, 40(%rax)
	cmpq	%r11, %r8
	jae	.LBB27_19
	incq	%r8
	movl	$1, %edx
	subq	%r11, %rdx
.LBB27_16:
	movzbl	-1(%r9,%r8), %r11d
	cmpq	$34, %r11
	ja	.LBB27_24
	btq	%r11, %r10
	jae	.LBB27_20
	movq	%r8, 40(%rax)
	leaq	(%rdx,%r8), %r11
	incq	%r11
	incq	%r8
	cmpq	$2, %r11
	jne	.LBB27_16
.LBB27_19:
	movq	$5, 32(%rsp)
	jmp	.LBB27_6
.LBB27_20:
	cmpq	$34, %r11
	jne	.LBB27_24
.LBB27_21:
	movb	$1, 1(%rcx)
.LBB27_22:
	xorl	%eax, %eax
.LBB27_23:
	movb	%al, (%rcx)
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.LBB27_24:
	movq	%rcx, %rsi
	cmpl	$125, %r11d
	jne	.LBB27_27
	movq	$21, 32(%rsp)
	leaq	32(%rsp), %rdx
	jmp	.LBB27_7
.LBB27_26:
	movq	$8, 32(%rsp)
	jmp	.LBB27_6
.LBB27_27:
	movq	$17, 32(%rsp)
	leaq	32(%rsp), %rdx
	jmp	.LBB27_7
.Lfunc_end27:
	.seh_endproc

	.def	_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.p2align	4
_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin28:
.seh_proc _RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$264, %rsp
	.seh_stackalloc 264
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 128(%rbp)
	movq	%rdx, %rdi
	movq	%rcx, %rsi
	movq	32(%rdx), %rbx
	movq	40(%rdx), %rax
	cmpq	%rbx, %rax
	jae	.LBB28_5
	leaq	24(%rdi), %r13
	movq	24(%rdi), %rdx
	addq	$5, %rax
	movq	%rbx, %r9
	negq	%r9
	movabsq	$4294977024, %r15
	.p2align	4
.LBB28_2:
	movzbl	-5(%rdx,%rax), %r8d
	cmpq	$32, %r8
	ja	.LBB28_7
	btq	%r8, %r15
	jae	.LBB28_7
	leaq	-4(%rax), %r8
	movq	%r8, 40(%rdi)
	leaq	(%r9,%rax), %r8
	incq	%r8
	incq	%rax
	cmpq	$5, %r8
	jne	.LBB28_2
.LBB28_5:
	movq	$5, 80(%rbp)
.LBB28_6:
	leaq	80(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB28_93
.LBB28_7:
	leal	-91(%r8), %r9d
	cmpl	$32, %r9d
	ja	.LBB28_34
	leaq	.LJTI28_0(%rip), %r10
	movslq	(%r10,%r9,4), %r9
	addq	%r10, %r9
	jmpq	*%r9
.LBB28_9:
	decb	48(%rdi)
	je	.LBB28_187
	addq	$-4, %rax
	movq	%rax, 40(%rdi)
	movq	$0, 48(%rbp)
	leaq	56(%rbp), %r11
	movq	$8, 56(%rbp)
	movq	$0, 64(%rbp)
	leaq	17(%rbp), %r12
	movb	$1, %r9b
	leaq	80(%rbp), %rcx
	cmpq	%rbx, %rax
	jae	.LBB28_29
.LBB28_11:
	movq	(%r13), %rdx
	leaq	2(%rax), %r8
	.p2align	4
.LBB28_12:
	movzbl	(%rdx,%rax), %r10d
	cmpq	$32, %r10
	ja	.LBB28_15
	btq	%r10, %r15
	jae	.LBB28_15
	incq	%rax
	movq	%rax, 40(%rdi)
	incq	%r8
	cmpq	%rax, %rbx
	jne	.LBB28_12
	jmp	.LBB28_29
.LBB28_15:
	cmpl	$93, %r10d
	je	.LBB28_111
	testb	$1, %r9b
	jne	.LBB28_24
	cmpb	$44, %r10b
	jne	.LBB28_212
	incq	%rax
	movq	%rax, 40(%rdi)
	cmpq	%rbx, %rax
	jae	.LBB28_160
	movl	$1, %eax
	subq	%rbx, %rax
.LBB28_20:
	movzbl	-1(%rdx,%r8), %r9d
	cmpq	$32, %r9
	ja	.LBB28_23
	btq	%r9, %r15
	jae	.LBB28_23
	movq	%r8, 40(%rdi)
	leaq	(%rax,%r8), %r9
	incq	%r9
	incq	%r8
	cmpq	$2, %r9
	jne	.LBB28_20
	jmp	.LBB28_160
.LBB28_23:
	cmpl	$93, %r9d
	je	.LBB28_214
.LBB28_24:
.Ltmp116:
	movq	%rdi, %rdx
	callq	_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp117:
	movzbl	80(%rbp), %eax
	cmpb	$-1, %al
	je	.LBB28_182
	leaq	81(%rbp), %r8
	movl	(%r8), %ecx
	movl	3(%r8), %edx
	movl	%edx, 3(%r12)
	movl	%ecx, (%r12)
	movq	88(%rbp), %rcx
	movups	15(%r8), %xmm0
	movups	%xmm0, 15(%r12)
	movb	%al, 16(%rbp)
	movq	%rcx, 24(%rbp)
	movq	64(%rbp), %r14
	cmpq	48(%rbp), %r14
	jne	.LBB28_28
.Ltmp118:
	leaq	48(%rbp), %rcx
	callq	_RNvMs3_NtCsdkPGTeTFVes_5alloc7raw_vecINtB5_6RawVecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE8grow_oneBQ_
	nop
.Ltmp119:
.LBB28_28:
	movq	56(%rbp), %rax
	movq	%r14, %rcx
	shlq	$5, %rcx
	movups	16(%rbp), %xmm0
	movups	32(%rbp), %xmm1
	movups	%xmm1, 16(%rax,%rcx)
	movups	%xmm0, (%rax,%rcx)
	incq	%r14
	movq	%r14, 64(%rbp)
	movq	32(%rdi), %rbx
	movq	40(%rdi), %rax
	xorl	%r9d, %r9d
	leaq	56(%rbp), %r11
	leaq	80(%rbp), %rcx
	cmpq	%rbx, %rax
	jb	.LBB28_11
.LBB28_29:
	movq	$2, 80(%rbp)
.Ltmp108:
	leaq	80(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp109:
.LBB28_30:
	movq	%rax, %r14
.LBB28_31:
	movq	56(%rbp), %rcx
	movq	64(%rbp), %rax
	movq	%rax, 72(%rbp)
	movq	$0, 120(%rbp)
	movq	%rcx, -56(%rbp)
	.p2align	4
.LBB28_32:
	movq	120(%rbp), %rax
	cmpq	%rax, 72(%rbp)
	je	.LBB28_77
	incq	%rax
	movq	%rax, 120(%rbp)
	leaq	32(%rcx), %r12
.Ltmp120:
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp121:
	movq	%r12, %rcx
	jmp	.LBB28_32
.LBB28_34:
	cmpl	$34, %r8d
	je	.LBB28_72
	cmpl	$45, %r8d
	jne	.LBB28_40
	addq	$-4, %rax
	movq	%rax, 40(%rdi)
	leaq	16(%rbp), %rcx
	movq	%rdi, %rdx
	xorl	%r8d, %r8d
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE13parse_integerCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	16(%rbp), %rcx
	cmpq	$-1, %rcx
	je	.LBB28_90
	movq	24(%rbp), %r14
	testq	%rcx, %rcx
	je	.LBB28_101
	movb	$2, %al
	cmpl	$1, %ecx
	je	.LBB28_39
	jmp	.LBB28_100
.LBB28_40:
	addb	$-48, %r8b
	cmpb	$10, %r8b
	jae	.LBB28_186
	leaq	16(%rbp), %rcx
	movq	%rdi, %rdx
	movb	$1, %r8b
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE13parse_integerCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	16(%rbp), %rcx
	cmpq	$-1, %rcx
	je	.LBB28_90
	movb	$2, %al
	movq	24(%rbp), %r14
	cmpl	$2, %ecx
	je	.LBB28_100
	cmpl	$1, %ecx
	jne	.LBB28_101
.LBB28_39:
	xorl	%ebx, %ebx
	jmp	.LBB28_110
.LBB28_45:
	leaq	-4(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-4(%rdx,%rax), %r9d
	leaq	-3(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpb	$97, %r9b
	jne	.LBB28_184
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-3(%rdx,%rax), %r9d
	leaq	-2(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpb	$108, %r9b
	jne	.LBB28_184
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-2(%rdx,%rax), %r9d
	leaq	-1(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpb	$115, %r9b
	jne	.LBB28_184
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-1(%rdx,%rax), %ecx
	movq	%rax, 40(%rdi)
	cmpb	$101, %cl
	jne	.LBB28_184
	movb	$1, %al
	xorl	%ecx, %ecx
	jmp	.LBB28_206
.LBB28_54:
	leaq	-4(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-4(%rdx,%rax), %r9d
	leaq	-3(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpb	$117, %r9b
	jne	.LBB28_184
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-3(%rdx,%rax), %r9d
	leaq	-2(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpb	$108, %r9b
	jne	.LBB28_184
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-2(%rdx,%rax), %ecx
	decq	%rax
	movq	%rax, 40(%rdi)
	cmpb	$108, %cl
	jne	.LBB28_184
	xorl	%eax, %eax
	jmp	.LBB28_114
.LBB28_61:
	leaq	-4(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-4(%rdx,%rax), %r9d
	leaq	-3(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpb	$114, %r9b
	jne	.LBB28_184
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-3(%rdx,%rax), %r9d
	leaq	-2(%rax), %r8
	movq	%r8, 40(%rdi)
	cmpb	$117, %r9b
	jne	.LBB28_184
	cmpq	%rbx, %r8
	jae	.LBB28_91
	movzbl	-2(%rdx,%rax), %ecx
	decq	%rax
	movq	%rax, 40(%rdi)
	cmpb	$101, %cl
	jne	.LBB28_184
	movb	$1, %cl
	movb	$1, %al
	jmp	.LBB28_206
.LBB28_68:
	decb	48(%rdi)
	je	.LBB28_187
	addq	$-4, %rax
	movq	%rax, 40(%rdi)
	movq	%rdi, -48(%rbp)
	movb	$1, -40(%rbp)
	leaq	80(%rbp), %rcx
	leaq	-48(%rbp), %rdx
	callq	_RINvNvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB8_9MapAccesspENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess13next_key_seed12has_next_keyNtNtBa_4read7StrReadECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	cmpb	$1, 80(%rbp)
	jne	.LBB28_94
	movq	88(%rbp), %r14
.LBB28_71:
	movq	%r14, -16(%rbp)
	movb	$-1, -24(%rbp)
	leaq	24(%rdi), %r13
	jmp	.LBB28_168
.LBB28_72:
	addq	$-4, %rax
	movq	%rax, 40(%rdi)
	movq	$0, 16(%rdi)
	leaq	80(%rbp), %rcx
	movq	%r13, %rdx
	movq	%rdi, %r8
	callq	_RNvXs8_NtCsfZWMcdrZHWk_10serde_json4readNtB5_7StrReadNtB5_4Read9parse_str
	movq	80(%rbp), %rax
	movq	88(%rbp), %rdi
	cmpq	$-1, %rax
	je	.LBB28_103
	movq	96(%rbp), %r15
	cmpl	$1, %eax
	jne	.LBB28_104
	testq	%r15, %r15
	js	.LBB28_219
	je	.LBB28_112
.LBB28_106:
	movl	$1, %ebx
	movl	$1, %edx
	movq	%r15, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB28_220
	movq	%rax, %r14
	movq	%rax, %rcx
	movq	%rdi, %rdx
	movq	%r15, %r8
	callq	memcpy
	jmp	.LBB28_113
.LBB28_77:
	movq	48(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB28_79
	shlq	$5, %rdx
	movl	$8, %r8d
	movq	-56(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB28_79:
	movq	32(%rdi), %rbx
	movq	40(%rdi), %rax
	movb	$-1, %dl
.LBB28_80:
	incb	48(%rdi)
	movb	%dl, 16(%rbp)
	movq	%r14, 24(%rbp)
	movaps	-48(%rbp), %xmm0
	movups	%xmm0, 32(%rbp)
	cmpq	%rbx, %rax
	jae	.LBB28_85
	movq	(%r13), %rdx
	incq	%rax
	movq	%rbx, %r8
	negq	%r8
.LBB28_82:
	movzbl	-1(%rdx,%rax), %r9d
	cmpq	$44, %r9
	ja	.LBB28_199
	btq	%r9, %r15
	jae	.LBB28_193
	movq	%rax, 40(%rdi)
	leaq	(%r8,%rax), %r9
	incq	%r9
	incq	%rax
	cmpq	$1, %r9
	jne	.LBB28_82
.LBB28_85:
	movq	$2, 48(%rbp)
.Ltmp126:
	leaq	48(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp127:
.LBB28_86:
	movq	%rax, %r12
	movups	16(%rbp), %xmm0
	movups	32(%rbp), %xmm1
	movaps	%xmm1, 96(%rbp)
	movaps	%xmm0, 80(%rbp)
	movq	%rax, 112(%rbp)
	cmpb	$-1, 80(%rbp)
	jne	.LBB28_175
	movq	88(%rbp), %rbx
	movq	(%r12), %rax
	testq	%rax, %rax
	movq	%r12, 120(%rbp)
	je	.LBB28_179
	cmpl	$1, %eax
	jne	.LBB28_181
	movq	120(%rbp), %rax
	leaq	8(%rax), %rcx
.Ltmp130:
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp131:
	jmp	.LBB28_181
.LBB28_90:
	movq	24(%rbp), %rax
	jmp	.LBB28_93
.LBB28_91:
	movq	$5, 80(%rbp)
.LBB28_92:
	leaq	80(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB28_93:
	movq	%rax, 8(%rsi)
	movb	$-1, (%rsi)
	jmp	.LBB28_207
.LBB28_94:
	cmpb	$1, 81(%rbp)
	jne	.LBB28_108
	movq	-48(%rbp), %r8
	leaq	24(%r8), %rdx
	incq	40(%r8)
	movq	$0, 16(%r8)
	leaq	80(%rbp), %rcx
	callq	_RNvXs8_NtCsfZWMcdrZHWk_10serde_json4readNtB5_7StrReadNtB5_4Read9parse_str
	movq	80(%rbp), %rax
	movq	88(%rbp), %r14
	cmpq	$-1, %rax
	je	.LBB28_71
	movq	96(%rbp), %r13
	cmpl	$1, %eax
	jne	.LBB28_115
	testq	%r13, %r13
	js	.LBB28_221
	je	.LBB28_119
.LBB28_117:
	movl	$1, %r12d
	movl	$1, %edx
	movq	%r13, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB28_222
	movq	%rax, %rbx
	movq	%rax, %rcx
	movq	%r14, %rdx
	movq	%r13, %r8
	callq	memcpy
	jmp	.LBB28_120
.LBB28_100:
	movq	%r14, %rbx
	shrq	$63, %rbx
	jmp	.LBB28_110
.LBB28_101:
	movabsq	$9223372036854775807, %rax
	andq	%r14, %rax
	movabsq	$9218868437227405311, %rcx
	cmpq	%rcx, %rax
	jle	.LBB28_109
	xorl	%eax, %eax
	movq	$-1, %rbx
	jmp	.LBB28_110
.LBB28_103:
	movq	%rdi, 8(%rsi)
	movb	$-1, (%rsi)
	jmp	.LBB28_207
.LBB28_104:
	testq	%r15, %r15
	js	.LBB28_219
	jne	.LBB28_106
.LBB28_112:
	movl	$1, %r14d
.LBB28_113:
	movb	$3, %al
	movq	%r15, %rbx
	jmp	.LBB28_114
.LBB28_108:
	movb	$5, -24(%rbp)
	movq	$0, -16(%rbp)
	movq	$0, (%rbp)
	jmp	.LBB28_168
.LBB28_109:
	movb	$0, 80(%rbp)
	leaq	80(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movb	$2, %al
	movl	$2, %ebx
.LBB28_110:
.LBB28_114:
	jmp	.LBB28_206
.LBB28_111:
	movq	48(%rbp), %r14
	movups	(%r11), %xmm0
	movaps	%xmm0, -48(%rbp)
	movb	$4, %dl
	jmp	.LBB28_80
.LBB28_115:
	testq	%r13, %r13
	js	.LBB28_221
	jne	.LBB28_117
.LBB28_119:
	movl	$1, %ebx
.LBB28_120:
	movq	$0, 48(%rbp)
	movq	$0, 64(%rbp)
	movq	%r13, 16(%rbp)
	movq	%rbx, 72(%rbp)
	movq	%rbx, 24(%rbp)
	movq	%r13, 120(%rbp)
	movq	%r13, 32(%rbp)
.Ltmp76:
	leaq	-24(%rbp), %rcx
	leaq	-48(%rbp), %rdx
	callq	_RINvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB6_9MapAccessNtNtB8_4read7StrReadENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess15next_value_seedINtNtCsd9eshbB3hfz_4core6marker11PhantomDataNtNtB8_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp77:
	cmpb	$-1, -24(%rbp)
	je	.LBB28_153
.Ltmp78:
	leaq	80(%rbp), %rcx
	leaq	48(%rbp), %rdx
	leaq	16(%rbp), %r8
	leaq	-24(%rbp), %r9
	callq	_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp79:
	cmpb	$-1, 80(%rbp)
	je	.LBB28_125
.Ltmp80:
	leaq	80(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp81:
.LBB28_125:
	leaq	80(%rbp), %r14
.LBB28_126:
.Ltmp82:
	movq	%r14, %rcx
	leaq	-48(%rbp), %rdx
	callq	_RINvNvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB8_9MapAccesspENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess13next_key_seed12has_next_keyNtNtBa_4read7StrReadECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp83:
	cmpb	$1, 80(%rbp)
	je	.LBB28_162
	cmpb	$1, 81(%rbp)
	jne	.LBB28_159
	movq	-48(%rbp), %r13
	leaq	24(%r13), %rdx
	incq	40(%r13)
	movq	$0, 16(%r13)
.Ltmp84:
	movq	%r14, %rcx
	movq	%rdx, %rbx
	movq	%r13, %r8
	callq	_RNvXs8_NtCsfZWMcdrZHWk_10serde_json4readNtB5_7StrReadNtB5_4Read9parse_str
	nop
.Ltmp85:
	movq	80(%rbp), %rax
	movq	88(%rbp), %r12
	cmpq	$-1, %rax
	je	.LBB28_165
	movq	96(%rbp), %rcx
	cmpl	$1, %eax
	movq	%rcx, 120(%rbp)
	jne	.LBB28_135
	testq	%rcx, %rcx
	js	.LBB28_225
	je	.LBB28_139
	movl	$1, %eax
	movq	%rax, 72(%rbp)
	movl	$1, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	jne	.LBB28_138
	jmp	.LBB28_226
.LBB28_135:
	testq	%rcx, %rcx
	js	.LBB28_223
	je	.LBB28_139
	movl	$1, %eax
	movq	%rax, 72(%rbp)
	movl	$1, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB28_224
.LBB28_138:
	movq	%rax, 72(%rbp)
	movq	%rax, %rcx
	movq	%r12, %rdx
	movq	120(%rbp), %r8
	callq	memcpy
	jmp	.LBB28_140
.LBB28_139:
	movl	$1, %eax
	movq	%rax, 72(%rbp)
.LBB28_140:
	movq	32(%r13), %rcx
	movq	40(%r13), %rax
	cmpq	%rcx, %rax
	jae	.LBB28_156
	movq	(%rbx), %rdx
	incq	%rax
	negq	%rcx
.LBB28_142:
	movzbl	-1(%rdx,%rax), %r8d
	cmpq	$58, %r8
	ja	.LBB28_211
	btq	%r8, %r15
	jae	.LBB28_145
	movq	%rax, 40(%r13)
	leaq	(%rcx,%rax), %r8
	incq	%r8
	incq	%rax
	cmpq	$1, %r8
	jne	.LBB28_142
	jmp	.LBB28_156
.LBB28_145:
	cmpq	$58, %r8
	jne	.LBB28_211
	movq	%rax, 40(%r13)
.Ltmp90:
	movq	%r14, %rcx
	movq	%r13, %rdx
	callq	_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp91:
	cmpb	$-1, 80(%rbp)
	je	.LBB28_163
	movups	80(%rbp), %xmm0
	movups	96(%rbp), %xmm1
	movaps	%xmm1, -80(%rbp)
	movaps	%xmm0, -96(%rbp)
	movq	120(%rbp), %rax
	cmpq	$-1, %rax
	je	.LBB28_159
	cmpq	$-2, %rax
	je	.LBB28_164
	movq	%rax, 16(%rbp)
	movq	72(%rbp), %rcx
	movq	%rcx, 24(%rbp)
	movq	%rax, 32(%rbp)
.Ltmp92:
	movq	%r14, %rcx
	leaq	48(%rbp), %rdx
	leaq	16(%rbp), %r8
	leaq	-96(%rbp), %r9
	callq	_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp93:
	cmpb	$-1, 80(%rbp)
	je	.LBB28_126
.Ltmp94:
	movq	%r14, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp95:
	jmp	.LBB28_126
.LBB28_153:
	movq	120(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB28_155
	movl	$1, %r8d
	movq	72(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB28_155:
	leaq	24(%rdi), %r13
	jmp	.LBB28_167
.LBB28_156:
	movq	$3, 16(%rbp)
.Ltmp96:
	leaq	16(%rbp), %rdx
	movq	%r13, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp97:
.LBB28_157:
	movq	%rax, %r12
	movq	120(%rbp), %rdx
	testq	%rdx, %rdx
	leaq	24(%rdi), %r13
	je	.LBB28_166
.LBB28_158:
	movl	$1, %r8d
	movq	72(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	jmp	.LBB28_166
.LBB28_159:
	movups	48(%rbp), %xmm0
	movups	%xmm0, 87(%rbp)
	movq	64(%rbp), %rax
	movq	%rax, 103(%rbp)
	movb	$5, -24(%rbp)
	movups	80(%rbp), %xmm0
	movups	%xmm0, -23(%rbp)
	movq	95(%rbp), %rax
	movq	%rax, -8(%rbp)
	movq	103(%rbp), %rax
	movq	%rax, (%rbp)
	leaq	24(%rdi), %r13
	jmp	.LBB28_168
.LBB28_160:
	movq	$5, 80(%rbp)
.Ltmp114:
	leaq	80(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp115:
	jmp	.LBB28_30
.LBB28_162:
	movq	88(%rbp), %r12
	jmp	.LBB28_165
.LBB28_163:
	movq	88(%rbp), %r12
	movq	120(%rbp), %rdx
	testq	%rdx, %rdx
	leaq	24(%rdi), %r13
	jne	.LBB28_158
	jmp	.LBB28_166
.LBB28_164:
	movq	72(%rbp), %r12
.LBB28_165:
	leaq	24(%rdi), %r13
.LBB28_166:
	movq	%r12, -16(%rbp)
	movb	$-1, -24(%rbp)
.LBB28_167:
	leaq	48(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB28_168:
	incb	48(%rdi)
	movq	32(%rdi), %rcx
	movq	40(%rdi), %rax
	cmpq	%rcx, %rax
	jae	.LBB28_173
	movq	(%r13), %rdx
	incq	%rax
	negq	%rcx
.LBB28_170:
	movzbl	-1(%rdx,%rax), %r8d
	cmpq	$44, %r8
	ja	.LBB28_190
	btq	%r8, %r15
	jae	.LBB28_188
	movq	%rax, 40(%rdi)
	leaq	(%rcx,%rax), %r8
	incq	%r8
	incq	%rax
	cmpq	$1, %r8
	jne	.LBB28_170
.LBB28_173:
	movq	$3, 16(%rbp)
.Ltmp102:
	leaq	16(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp103:
.LBB28_174:
	movq	%rax, %r12
	movups	-24(%rbp), %xmm0
	movups	-8(%rbp), %xmm1
	movaps	%xmm0, 80(%rbp)
	movaps	%xmm1, 96(%rbp)
	movq	%rax, 112(%rbp)
	cmpb	$-1, 80(%rbp)
	je	.LBB28_176
.LBB28_175:
	leaq	80(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movb	$-1, %al
	movq	%r12, %rbx
	jmp	.LBB28_204
.LBB28_176:
	movq	88(%rbp), %rbx
	movq	(%r12), %rax
	testq	%rax, %rax
	movq	%r12, 120(%rbp)
	je	.LBB28_179
	cmpl	$1, %eax
	jne	.LBB28_181
	movq	120(%rbp), %rax
	leaq	8(%rax), %rcx
.Ltmp106:
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp107:
	jmp	.LBB28_181
.LBB28_179:
	movq	16(%r12), %rdx
	testq	%rdx, %rdx
	je	.LBB28_181
	movq	120(%rbp), %rax
	movq	8(%rax), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB28_181:
	movl	$40, %edx
	movl	$8, %r8d
	movq	120(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	jmp	.LBB28_203
.LBB28_182:
	movq	88(%rbp), %r14
	jmp	.LBB28_31
.LBB28_184:
	movq	$9, 80(%rbp)
	jmp	.LBB28_92
.LBB28_186:
	movq	$10, 80(%rbp)
	leaq	80(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rax, %rbx
	jmp	.LBB28_208
.LBB28_187:
	movq	$24, 80(%rbp)
	jmp	.LBB28_6
.LBB28_188:
	cmpq	$44, %r8
	jne	.LBB28_190
	movq	$21, 16(%rbp)
.Ltmp100:
	leaq	16(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp101:
	jmp	.LBB28_174
.LBB28_190:
	cmpl	$125, %r8d
	jne	.LBB28_209
	movq	%rax, 40(%rdi)
	movups	-24(%rbp), %xmm0
	movups	-8(%rbp), %xmm1
	movaps	%xmm0, 80(%rbp)
	movaps	%xmm1, 96(%rbp)
	cmpb	$-1, 80(%rbp)
	je	.LBB28_202
	movzbl	-24(%rbp), %eax
	movzbl	-23(%rbp), %ecx
	movl	-22(%rbp), %edx
	movl	%edx, 8(%rbp)
	movzwl	-18(%rbp), %edx
	movw	%dx, 12(%rbp)
	movq	-16(%rbp), %rbx
	movq	-8(%rbp), %r14
	movq	(%rbp), %r15
	jmp	.LBB28_205
.LBB28_193:
	cmpq	$44, %r9
	jne	.LBB28_199
	movq	%rax, 40(%rdi)
	cmpq	%rbx, %rax
	jae	.LBB28_218
	incq	%rax
	movl	$1, %r8d
	subq	%rbx, %r8
.LBB28_196:
	movzbl	-1(%rdx,%rax), %ecx
	cmpq	$32, %rcx
	ja	.LBB28_216
	btq	%rcx, %r15
	jae	.LBB28_216
	movq	%rax, 40(%rdi)
	leaq	(%r8,%rax), %rcx
	incq	%rcx
	incq	%rax
	cmpq	$2, %rcx
	jne	.LBB28_196
	jmp	.LBB28_218
.LBB28_199:
	cmpl	$93, %r9d
	jne	.LBB28_210
	movq	%rax, 40(%rdi)
	movups	16(%rbp), %xmm0
	movups	32(%rbp), %xmm1
	movaps	%xmm1, 96(%rbp)
	movaps	%xmm0, 80(%rbp)
	movzbl	80(%rbp), %eax
	cmpb	$-1, %al
	je	.LBB28_202
	movzbl	81(%rbp), %ecx
	movl	82(%rbp), %edx
	movl	%edx, 8(%rbp)
	movzwl	86(%rbp), %edx
	movw	%dx, 12(%rbp)
	movq	88(%rbp), %rbx
	movq	96(%rbp), %r14
	movq	104(%rbp), %r15
	jmp	.LBB28_205
.LBB28_202:
	movq	88(%rbp), %rbx
.LBB28_203:
	movb	$-1, %al
.LBB28_204:
.LBB28_205:
	cmpb	$-1, %al
	je	.LBB28_208
.LBB28_206:
	movb	%al, (%rsi)
	movb	%cl, 1(%rsi)
	movl	8(%rbp), %eax
	movl	%eax, 2(%rsi)
	movzwl	12(%rbp), %eax
	movw	%ax, 6(%rsi)
	movq	%rbx, 8(%rsi)
	movq	%r14, 16(%rsi)
	movq	%r15, 24(%rsi)
.LBB28_207:
	.seh_startepilogue
	addq	$264, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB28_208:
	movq	%rbx, %rcx
	movq	%rdi, %rdx
	callq	_RINvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB6_5Error12fix_positionNCNvMs3_NtB8_2deINtB1b_12DeserializerNtNtB8_4read7StrReadE12fix_position0ECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB28_93
.LBB28_209:
	movq	$22, 16(%rbp)
.Ltmp104:
	leaq	16(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp105:
	jmp	.LBB28_174
.LBB28_210:
	movq	$22, 48(%rbp)
.Ltmp128:
	leaq	48(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp129:
	jmp	.LBB28_86
.LBB28_211:
	movq	$6, 16(%rbp)
.Ltmp98:
	leaq	16(%rbp), %rdx
	movq	%r13, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp99:
	jmp	.LBB28_157
.LBB28_212:
	movq	$7, 80(%rbp)
.Ltmp110:
	leaq	80(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp111:
	jmp	.LBB28_30
.LBB28_214:
	movq	$21, 80(%rbp)
.Ltmp112:
	leaq	80(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp113:
	jmp	.LBB28_30
.LBB28_216:
	cmpl	$93, %ecx
	jne	.LBB28_218
	movq	$21, 48(%rbp)
.Ltmp122:
	leaq	48(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp123:
	jmp	.LBB28_86
.LBB28_218:
	movq	$22, 48(%rbp)
.Ltmp124:
	leaq	48(%rbp), %rdx
	movq	%rdi, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp125:
	jmp	.LBB28_86
.LBB28_219:
	xorl	%ebx, %ebx
.LBB28_220:
	movq	%rbx, %rcx
	movq	%r15, %rdx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	ud2
.LBB28_221:
	xorl	%r12d, %r12d
.LBB28_222:
	movq	%r12, %rcx
	movq	%r13, %rdx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	ud2
.LBB28_223:
	movq	$0, 72(%rbp)
.LBB28_224:
.Ltmp86:
	movq	72(%rbp), %rcx
	movq	120(%rbp), %rdx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp87:
	jmp	.LBB28_227
.LBB28_225:
	movq	$0, 72(%rbp)
.LBB28_226:
.Ltmp88:
	movq	72(%rbp), %rcx
	movq	120(%rbp), %rdx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp89:
.LBB28_227:
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$228@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$228@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$228@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_228:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$40, %edx
	movl	$8, %r8d
	movq	120(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$229@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$229@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$229@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_229:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$40, %edx
	movl	$8, %r8d
	movq	120(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$230@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$230@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$230@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_230:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	16(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$231@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$231@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$231@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_231:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$0, 120(%rbp)
	je	.LBB28_233
	movl	$1, %r8d
	movq	72(%rbp), %rcx
	movq	120(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB28_233:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$234@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$234@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$234@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_234:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$0, 120(%rbp)
	je	.LBB28_236
	movl	$1, %r8d
	movq	72(%rbp), %rcx
	movq	120(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB28_236:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$237@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$237@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$237@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_237:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	16(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$238@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$238@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$238@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_238:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	-24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsfZWMcdrZHWk_10serde_json5value5ValueNtNtB11_5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$239@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$239@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$239@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_239:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	48(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc3vec3VecNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$240@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$240@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$240@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_240:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	48(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsfZWMcdrZHWk_10serde_json3map3MapNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtBG_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$241@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$241@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$241@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_241:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	72(%rbp), %rdi
	movq	120(%rbp), %rbx
	cmpq	%rbx, %rdi
	movq	-56(%rbp), %rsi
	je	.LBB28_243
.LBB28_242:
	movq	%rbx, %rcx
	shlq	$5, %rcx
	addq	%rsi, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	incq	%rbx
	cmpq	%rdi, %rbx
	jne	.LBB28_242
.LBB28_243:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.def	"?dtor$244@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$244@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$244@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB28_244:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	48(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB28_246
	shlq	$5, %rdx
	movl	$8, %r8d
	movq	-56(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB28_246:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end28:
	.section	.rdata,"dr",associative,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,29
	.p2align	2, 0x0
.LJTI28_0:
	.long	.LBB28_9-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_45-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_54-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_61-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_40-.LJTI28_0
	.long	.LBB28_68-.LJTI28_0
	.seh_handlerdata
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28
	.seh_endproc
	.section	.xdata,"dr",associative,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,17
	.p2align	2, 0x0
$cppxdata$_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	11
	.long	$stateUnwindMap$_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	33
	.long	$ip2state$_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	256
	.long	0
	.long	1
$stateUnwindMap$_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$239@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	0
	.long	"?dtor$230@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	-1
	.long	"?dtor$244@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	2
	.long	"?dtor$241@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	-1
	.long	"?dtor$237@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	-1
	.long	"?dtor$228@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	-1
	.long	"?dtor$240@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	6
	.long	"?dtor$234@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	6
	.long	"?dtor$231@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	-1
	.long	"?dtor$238@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	-1
	.long	"?dtor$229@?0?_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin28@IMGREL
	.long	-1
	.long	.Ltmp116@IMGREL
	.long	0
	.long	.Ltmp118@IMGREL
	.long	1
	.long	.Ltmp108@IMGREL
	.long	0
	.long	.Ltmp120@IMGREL
	.long	3
	.long	.Ltmp121@IMGREL
	.long	-1
	.long	.Ltmp126@IMGREL
	.long	4
	.long	.Ltmp130@IMGREL
	.long	5
	.long	.Ltmp131@IMGREL
	.long	-1
	.long	.Ltmp76@IMGREL
	.long	8
	.long	.Ltmp78@IMGREL
	.long	6
	.long	.Ltmp85@IMGREL
	.long	-1
	.long	.Ltmp90@IMGREL
	.long	7
	.long	.Ltmp92@IMGREL
	.long	6
	.long	.Ltmp95@IMGREL
	.long	-1
	.long	.Ltmp96@IMGREL
	.long	7
	.long	.Ltmp97@IMGREL
	.long	-1
	.long	.Ltmp114@IMGREL
	.long	0
	.long	.Ltmp115@IMGREL
	.long	-1
	.long	.Ltmp102@IMGREL
	.long	9
	.long	.Ltmp103@IMGREL
	.long	-1
	.long	.Ltmp106@IMGREL
	.long	10
	.long	.Ltmp107@IMGREL
	.long	-1
	.long	.Ltmp100@IMGREL
	.long	9
	.long	.Ltmp101@IMGREL
	.long	-1
	.long	.Ltmp104@IMGREL
	.long	9
	.long	.Ltmp128@IMGREL
	.long	4
	.long	.Ltmp98@IMGREL
	.long	7
	.long	.Ltmp110@IMGREL
	.long	0
	.long	.Ltmp122@IMGREL
	.long	4
	.long	.Ltmp125@IMGREL
	.long	-1
	.long	.Ltmp86@IMGREL
	.long	6
	.long	.Ltmp89@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,28

	.def	_RINvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB6_9MapAccessNtNtB8_4read7StrReadENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess15next_value_seedINtNtCsd9eshbB3hfz_4core6marker11PhantomDataNtNtB8_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RINvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB6_9MapAccessNtNtB8_4read7StrReadENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess15next_value_seedINtNtCsd9eshbB3hfz_4core6marker11PhantomDataNtNtB8_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,30
	.p2align	4
_RINvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB6_9MapAccessNtNtB8_4read7StrReadENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess15next_value_seedINtNtCsd9eshbB3hfz_4core6marker11PhantomDataNtNtB8_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin29:
.seh_proc _RINvXs9_NtCsfZWMcdrZHWk_10serde_json2deINtB6_9MapAccessNtNtB8_4read7StrReadENtNtCsidvd3kPC9MK_10serde_core2de9MapAccess15next_value_seedINtNtCsd9eshbB3hfz_4core6marker11PhantomDataNtNtB8_5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$64, %rsp
	.seh_stackalloc 64
	.seh_endprologue
	movq	(%rdx), %rdx
	movq	32(%rdx), %r8
	movq	40(%rdx), %rax
	cmpq	%r8, %rax
	jae	.LBB29_7
	movq	24(%rdx), %r9
	incq	%rax
	negq	%r8
	movabsq	$4294977024, %r10
.LBB29_2:
	movzbl	-1(%r9,%rax), %r11d
	cmpq	$58, %r11
	ja	.LBB29_5
	btq	%r11, %r10
	jae	.LBB29_4
	movq	%rax, 40(%rdx)
	leaq	(%r8,%rax), %r11
	incq	%r11
	incq	%rax
	cmpq	$1, %r11
	jne	.LBB29_2
.LBB29_7:
	movq	%rcx, %rsi
	movq	$3, 40(%rsp)
.LBB29_8:
	leaq	40(%rsp), %rax
	movq	%rdx, %rcx
	movq	%rax, %rdx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rax, 8(%rsi)
	movb	$-1, (%rsi)
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rsi
	.seh_endepilogue
	retq
.LBB29_4:
	cmpq	$58, %r11
	jne	.LBB29_5
	movq	%rax, 40(%rdx)
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rsi
	.seh_endepilogue
	jmp	_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB29_5:
	movq	%rcx, %rsi
	movq	$6, 40(%rsp)
	jmp	.LBB29_8
.Lfunc_end29:
	.seh_endproc

	.def	_RNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtB8_4Once15call_once_forceNCINvMNtBa_9once_lockINtB1b_8OnceLockTjjjbEE10initializeNCINvB1a_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0B2p_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtB8_4Once15call_once_forceNCINvMNtBa_9once_lockINtB1b_8OnceLockTjjjbEE10initializeNCINvB1a_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0B2p_,unique,31
	.p2align	4
_RNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtB8_4Once15call_once_forceNCINvMNtBa_9once_lockINtB1b_8OnceLockTjjjbEE10initializeNCINvB1a_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0B2p_:
.Lfunc_begin30:
.seh_proc _RNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtB8_4Once15call_once_forceNCINvMNtBa_9once_lockINtB1b_8OnceLockTjjjbEE10initializeNCINvB1a_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0B2p_
	pushq	%r15
	.seh_pushreg %r15
	pushq	%r14
	.seh_pushreg %r14
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
	subq	$128, %rsp
	.seh_stackalloc 128
	.seh_endprologue
	movq	(%rcx), %rax
	movq	(%rax), %r14
	movq	$0, (%rax)
	testq	%r14, %r14
	je	.LBB30_10
	movq	$0, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.17(%rip), %rdx
	leaq	56(%rsp), %rcx
	movl	$8, %r8d
	movl	$8, %r9d
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step
	cmpl	$1, 56(%rsp)
	je	.LBB30_2
	movq	80(%rsp), %r15
	movq	72(%rsp), %rdi
	movups	64(%rsp), %xmm0
	movaps	%xmm0, 96(%rsp)
	movq	%r15, 112(%rsp)
	movq	$0, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.17(%rip), %rdx
	leaq	56(%rsp), %rsi
	movl	$8, %r8d
	movl	$9, %r9d
	movq	%rsi, %rcx
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step
	cmpl	$1, 56(%rsp)
	je	.LBB30_5
	movq	72(%rsp), %rax
	subq	%rdi, %rax
	movq	%rax, 88(%rsp)
	cmpq	%r15, %rax
	jne	.LBB30_11
	leaq	-1(%r15), %rax
	cmpq	$512, %rax
	jae	.LBB30_12
	movq	$0, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.17(%rip), %rdx
	leaq	56(%rsp), %rcx
	movl	$8, %r8d
	movl	$64, %r9d
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step
	cmpl	$1, 56(%rsp)
	je	.LBB30_13
	movq	72(%rsp), %rsi
	addq	64(%rsp), %rsi
	movq	%r15, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.17(%rip), %rdi
	leaq	56(%rsp), %rbx
	movl	$8, %r8d
	movq	%rbx, %rcx
	movq	%rdi, %rdx
	movq	%rsi, %r9
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search
	movq	56(%rsp), %r12
	movzbl	80(%rsp), %ebp
	movq	%r15, 32(%rsp)
	movl	$8, %r8d
	movq	%rbx, %rcx
	movq	%rdi, %rdx
	movq	%rsi, %r9
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search
	movq	56(%rsp), %rax
	orb	80(%rsp), %bpl
	cmpq	%r12, %rax
	cmovaq	%rax, %r12
	movq	%r15, 32(%rsp)
	movl	$8, %r8d
	movq	%rbx, %rcx
	movq	%rdi, %rdx
	movq	%rsi, %r9
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search
	movq	56(%rsp), %rax
	orb	80(%rsp), %bpl
	cmpq	%r12, %rax
	cmovaq	%rax, %r12
	movq	%r15, (%r14)
	movq	%r12, 8(%r14)
	movq	%rsi, 16(%r14)
	movb	%bpl, 24(%r14)
	.seh_startepilogue
	addq	$128, %rsp
	popq	%rbx
	popq	%rbp
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r14
	popq	%r15
	.seh_endepilogue
	retq
.LBB30_10:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.15(%rip), %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13unwrap_failed
	ud2
.LBB30_2:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.122(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.123(%rip), %r8
	movl	$61, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	ud2
.LBB30_5:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.120(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.121(%rip), %r8
	movl	$61, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	ud2
.LBB30_11:
	leaq	112(%rsp), %rax
	leaq	88(%rsp), %rdx
	movq	%rdx, 56(%rsp)
	leaq	_RNvXsi_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impjNtB9_7Display3fmt(%rip), %rcx
	movq	%rcx, 64(%rsp)
	leaq	112(%rsp), %r8
	movq	%rax, 72(%rsp)
	movq	%rcx, 80(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.117(%rip), %rax
	movq	%rax, 40(%rsp)
	movq	%rsi, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.116(%rip), %r9
	xorl	%ecx, %ecx
	callq	_RINvNtCsd9eshbB3hfz_4core9panicking13assert_failedjjEB4_
	ud2
.LBB30_12:
	leaq	88(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	_RNvXsi_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impjNtB9_7Display3fmt(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.118(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.119(%rip), %r8
	leaq	56(%rsp), %rdx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	ud2
.LBB30_13:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.18(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.20(%rip), %r8
	movl	$65, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	ud2
.Lfunc_end30:
	.seh_endproc

	.def	_RNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0Cs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0Cs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,32
	.p2align	4
_RNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0Cs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin31:
.seh_proc _RNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0Cs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	movq	(%rcx), %rcx
	callq	_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	xorl	%eax, %eax
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	retq
.Lfunc_end31:
	.seh_endproc

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

	.def	_RNSNvYNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtBd_4Once15call_once_forceNCINvMNtBf_9once_lockINtB1g_8OnceLockTjjjbEE10initializeNCINvB1f_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceTRNtBd_9OnceStateEE9call_once6vtableB2u_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNSNvYNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtBd_4Once15call_once_forceNCINvMNtBf_9once_lockINtB1g_8OnceLockTjjjbEE10initializeNCINvB1f_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceTRNtBd_9OnceStateEE9call_once6vtableB2u_,unique,34
	.p2align	4
_RNSNvYNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtBd_4Once15call_once_forceNCINvMNtBf_9once_lockINtB1g_8OnceLockTjjjbEE10initializeNCINvB1f_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceTRNtBd_9OnceStateEE9call_once6vtableB2u_:
.Lfunc_begin33:
.seh_proc _RNSNvYNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtBd_4Once15call_once_forceNCINvMNtBf_9once_lockINtB1g_8OnceLockTjjjbEE10initializeNCINvB1f_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceTRNtBd_9OnceStateEE9call_once6vtableB2u_
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	movq	(%rcx), %rax
	movq	%rax, 32(%rsp)
	leaq	32(%rsp), %rcx
	callq	_RNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtB8_4Once15call_once_forceNCINvMNtBa_9once_lockINtB1b_8OnceLockTjjjbEE10initializeNCINvB1a_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0B2p_
	nop
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	retq
.Lfunc_end33:
	.seh_endproc

	.def	_RNSNvYNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNSNvYNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,35
	.p2align	4
_RNSNvYNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin34:
.seh_proc _RNSNvYNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	movq	(%rcx), %rcx
	callq	_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	xorl	%eax, %eax
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	retq
.Lfunc_end34:
	.seh_endproc

	.def	_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_,unique,36
	.p2align	4
_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_:
.Lfunc_begin35:
.seh_proc _RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$344, %rsp
	.seh_stackalloc 344
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 200(%rbp)
	movq	48(%rcx), %rax
	movq	%rax, (%rbp)
	movups	16(%rcx), %xmm0
	movups	32(%rcx), %xmm1
	movaps	%xmm1, -16(%rbp)
	movaps	%xmm0, -32(%rbp)
	movups	56(%rcx), %xmm2
	movups	72(%rcx), %xmm3
	movups	%xmm3, 24(%rbp)
	movups	%xmm2, 8(%rbp)
	movq	%rcx, 176(%rbp)
	movups	(%rcx), %xmm2
	movaps	%xmm2, 48(%rbp)
	movaps	%xmm0, 64(%rbp)
	movaps	%xmm1, 80(%rbp)
	movq	(%rbp), %rax
	movq	%rax, 96(%rbp)
	movq	8(%rbp), %rax
	movq	%rax, 104(%rbp)
	movaps	16(%rbp), %xmm0
	movaps	%xmm0, 112(%rbp)
	movq	32(%rbp), %rax
	movq	%rax, 128(%rbp)
	leaq	104(%rbp), %rcx
	movb	$1, 199(%rbp)
.Ltmp132:
	callq	_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNCNCINvNtNtB6_6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_000uEB1Z_
	nop
.Ltmp133:
	movq	96(%rbp), %rax
	movq	%rax, -48(%rbp)
	movaps	48(%rbp), %xmm0
	movaps	64(%rbp), %xmm1
	movaps	80(%rbp), %xmm2
	movaps	%xmm2, -64(%rbp)
	movaps	%xmm1, -80(%rbp)
	movaps	%xmm0, -96(%rbp)
	movb	$0, 199(%rbp)
.Ltmp134:
	leaq	-96(%rbp), %rcx
	callq	_RINvNtNtCsaZOKLwlFm27_3std3sys9backtrace28___rust_begin_short_backtraceNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEB1b_
	nop
.Ltmp135:
	movq	$0, 184(%rbp)
.LBB35_3:
$ehgcr_35_3:
	movq	176(%rbp), %rax
	movq	88(%rax), %rsi
	addq	$88, %rax
	movq	%rax, 176(%rbp)
	cmpq	$0, 24(%rsi)
	je	.LBB35_9
	movq	32(%rsi), %rax
	movq	%rax, 152(%rbp)
	testq	%rax, %rax
	je	.LBB35_9
	movq	%rsi, 144(%rbp)
	movq	40(%rsi), %rax
	movq	%rax, 168(%rbp)
	movq	(%rax), %rax
	testq	%rax, %rax
	je	.LBB35_7
.Ltmp136:
	movq	152(%rbp), %rcx
	callq	*%rax
	nop
.Ltmp137:
.LBB35_7:
	movq	168(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	movq	144(%rbp), %rsi
	je	.LBB35_9
	movq	16(%rax), %r8
	movq	152(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB35_9:
	movq	$1, 24(%rsi)
	movq	184(%rbp), %rax
	movq	%rax, 32(%rsi)
	movq	160(%rbp), %rax
	movq	%rax, 40(%rsi)
	movq	%rsi, 136(%rbp)
	lock		decq	(%rsi)
	jne	.LBB35_11
	#MEMBARRIER
	movb	$0, 198(%rbp)
.Ltmp138:
	leaq	136(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEE9drop_slowBN_
	nop
.Ltmp139:
.LBB35_11:
	.seh_startepilogue
	addq	$344, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@IMGREL
	.section	.text,"xr",one_only,_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_,unique,36
	.seh_endproc
	.def	"?dtor$12@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$12@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA":
.seh_proc "?dtor$12@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"
.LBB35_12:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpb	$0, 199(%rbp)
	je	.LBB35_13
	movq	64(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB35_13
	movq	72(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB35_13:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_,unique,36
	.seh_endproc
	.def	"?catch$16@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?catch$16@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA":
.seh_proc "?catch$16@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"
	.seh_handler __CxxFrameHandler3, @unwind, @except
.LBB35_16:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	208(%rbp), %rcx
	callq	_RNvNvNtCsaZOKLwlFm27_3std9panicking12catch_unwind7cleanup
	movq	%rax, 184(%rbp)
	movq	%rdx, 160(%rbp)
	leaq	.LBB35_3(%rip), %rax
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@IMGREL
	.section	.text,"xr",one_only,_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_,unique,36
	.seh_endproc
	.def	"?catch$17@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?catch$17@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA":
.seh_proc "?catch$17@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"
	.seh_handler __CxxFrameHandler3, @unwind, @except
.LBB35_17:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	xorl	%ecx, %ecx
	callq	_RNvNvNtCsaZOKLwlFm27_3std9panicking12catch_unwind7cleanup
	movq	%rax, 184(%rbp)
	movq	%rdx, 160(%rbp)
	leaq	.LBB35_3(%rip), %rax
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@IMGREL
	.section	.text,"xr",one_only,_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_,unique,36
	.seh_endproc
	.def	"?dtor$18@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$18@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA":
.seh_proc "?dtor$18@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"
.LBB35_18:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	168(%rbp), %rax
	movq	8(%rax), %rdx
	testq	%rdx, %rdx
	je	.LBB35_20
	movq	168(%rbp), %rax
	movq	16(%rax), %r8
	movq	152(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB35_20:
	movq	144(%rbp), %rcx
	movq	$1, 24(%rcx)
	movq	184(%rbp), %rax
	movq	%rax, 32(%rcx)
	movq	160(%rbp), %rax
	movq	%rax, 40(%rcx)
	movb	$1, 198(%rbp)
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_,unique,36
	.seh_endproc
	.def	"?dtor$21@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$21@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA":
.seh_proc "?dtor$21@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"
.LBB35_21:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpb	$0, 198(%rbp)
	je	.LBB35_24
	movq	176(%rbp), %rax
	movq	(%rax), %rax
	lock		decq	(%rax)
	jne	.LBB35_24
	#MEMBARRIER
	movq	176(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEE9drop_slowBN_
	nop
.LBB35_24:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end35:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_,unique,36
	.seh_endproc
	.section	.xdata,"dr",associative,_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_,unique,18
	.p2align	2, 0x0
$cppxdata$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_:
	.long	429065506
	.long	5
	.long	$stateUnwindMap$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@IMGREL
	.long	1
	.long	$tryMap$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@IMGREL
	.long	8
	.long	$ip2state$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@IMGREL
	.long	328
	.long	0
	.long	1
$stateUnwindMap$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_:
	.long	-1
	.long	0
	.long	0
	.long	"?dtor$12@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"@IMGREL
	.long	-1
	.long	0
	.long	-1
	.long	"?dtor$21@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"@IMGREL
	.long	3
	.long	"?dtor$18@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"@IMGREL
$tryMap$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_:
	.long	0
	.long	1
	.long	2
	.long	2
	.long	$handlerMap$0$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@IMGREL
$handlerMap$0$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_:
	.long	8
	.long	_RNvCs9hJ03s5DiqP_7___rustc22___rust_panic_type_info@IMGREL
	.long	336
	.long	"?catch$16@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"@IMGREL
	.long	72
	.long	64
	.long	0
	.long	0
	.long	"?catch$17@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"@IMGREL
	.long	72
$ip2state$_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_:
	.long	.Lfunc_begin35@IMGREL
	.long	-1
	.long	.Ltmp132@IMGREL
	.long	1
	.long	.Ltmp136@IMGREL
	.long	4
	.long	.Ltmp137@IMGREL
	.long	-1
	.long	.Ltmp138@IMGREL
	.long	3
	.long	.Ltmp139@IMGREL
	.long	-1
	.long	"?catch$16@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"@IMGREL
	.long	2
	.long	"?catch$17@?0?_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_@4HA"@IMGREL
	.long	2
	.section	.text,"xr",one_only,_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_,unique,36

	.def	_RNSNvYNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableBa_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNSNvYNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableBa_,unique,37
	.p2align	4
_RNSNvYNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableBa_:
.Lfunc_begin36:
.seh_proc _RNSNvYNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableBa_
	subq	$56, %rsp
	.seh_stackalloc 56
	.seh_endprologue
	movups	(%rcx), %xmm0
	movaps	%xmm0, 32(%rsp)
	leaq	32(%rsp), %rcx
	callq	_RNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00B5_
	nop
	.seh_startepilogue
	addq	$56, %rsp
	.seh_endepilogue
	retq
.Lfunc_end36:
	.seh_endproc

	.def	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search,unique,38
	.p2align	4
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search:
.Lfunc_begin37:
.seh_proc _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search
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
	subq	$184, %rsp
	.seh_stackalloc 184
	.seh_endprologue
	movq	%r9, %rsi
	movq	%r8, %rdi
	movq	%rdx, %rbx
	movq	%rcx, %r14
	movq	288(%rsp), %r12
	movq	%r9, 136(%rsp)
	cmpq	$1, %r12
	adcq	$0, %r12
	leaq	(,%r12,8), %rbp
	subq	%rbp, %rsi
	addq	$-4096, %rsi
	movq	%rsi, 144(%rsp)
	movq	%rsi, 40(%rsp)
	movq	$1, 32(%rsp)
	leaq	152(%rsp), %rcx
	xorl	%r9d, %r9d
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step
	cmpl	$1, 152(%rsp)
	je	.LBB37_18
	movq	%r14, 128(%rsp)
	movq	176(%rsp), %rax
	movq	%rax, 112(%rsp)
	movq	%rsi, %rax
	shrq	%rax
	movq	%rsi, %r14
	subq	%rbp, %r14
	cmovbeq	%rax, %r14
	movups	160(%rsp), %xmm0
	movaps	%xmm0, 96(%rsp)
	testq	%r14, %r14
	je	.LBB37_2
	movq	%r14, 40(%rsp)
	movq	$1, 32(%rsp)
	leaq	64(%rsp), %rcx
	movq	%rbx, %rdx
	movq	%rdi, %r8
	xorl	%r9d, %r9d
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step
	cmpb	$0, 64(%rsp)
	je	.LBB37_12
	movq	%r14, %r13
	movl	$0, 60(%rsp)
	jmp	.LBB37_4
.LBB37_2:
	movb	$1, %al
	movl	%eax, 60(%rsp)
	jmp	.LBB37_3
.LBB37_12:
	movb	$1, %al
	movl	%eax, 60(%rsp)
	leaq	72(%rsp), %r15
	movq	%r14, %r13
	.p2align	4
.LBB37_13:
	movq	16(%r15), %rax
	movq	%rax, 112(%rsp)
	movups	(%r15), %xmm0
	movaps	%xmm0, 96(%rsp)
	addq	%rbp, %rbp
	movq	%r14, %rax
	shrq	%rax
	subq	%rbp, %r13
	cmovbeq	%rax, %r13
	testq	%r13, %r13
	je	.LBB37_14
	movq	%r13, 40(%rsp)
	movq	$1, 32(%rsp)
	leaq	64(%rsp), %rcx
	movq	%rbx, %rdx
	movq	%rdi, %r8
	xorl	%r9d, %r9d
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step
	cmpl	$1, 64(%rsp)
	movq	%r14, %rsi
	movq	%r13, %r14
	jne	.LBB37_13
	movl	$0, 60(%rsp)
	jmp	.LBB37_4
.LBB37_14:
	movq	%r14, %rsi
.LBB37_3:
	xorl	%r13d, %r13d
.LBB37_4:
	addq	%r12, %r12
	movq	%rsi, %r14
	subq	%r13, %r14
	cmpq	%r12, %r14
	jbe	.LBB37_17
	leaq	72(%rsp), %rbp
	leaq	64(%rsp), %r15
	jmp	.LBB37_6
	.p2align	4
.LBB37_7:
	movq	%r14, %r13
	movq	%rsi, %r14
	subq	%r13, %r14
	cmpq	%r12, %r14
	jbe	.LBB37_17
.LBB37_6:
	shrq	%r14
	addq	%r13, %r14
	movq	%r14, 40(%rsp)
	movq	$1, 32(%rsp)
	movq	%r15, %rcx
	movq	%rbx, %rdx
	movq	%rdi, %r8
	xorl	%r9d, %r9d
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step
	cmpb	$0, 64(%rsp)
	jne	.LBB37_7
	movq	16(%rbp), %rax
	movq	%rax, 112(%rsp)
	movups	(%rbp), %xmm0
	movaps	%xmm0, 96(%rsp)
	movq	%r14, %rsi
	subq	%r13, %r14
	cmpq	%r12, %r14
	ja	.LBB37_6
.LBB37_17:
	movq	112(%rsp), %rax
	movq	128(%rsp), %rcx
	movq	%rax, 16(%rcx)
	movaps	96(%rsp), %xmm0
	movups	%xmm0, (%rcx)
	movl	60(%rsp), %eax
	movb	%al, 24(%rcx)
	.seh_startepilogue
	addq	$184, %rsp
	popq	%rbx
	popq	%rbp
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	.seh_endepilogue
	retq
.LBB37_18:
	leaq	144(%rsp), %rax
	movq	%rax, 64(%rsp)
	leaq	_RNvXsi_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impjNtB9_7Display3fmt(%rip), %rax
	movq	%rax, 72(%rsp)
	leaq	136(%rsp), %rcx
	movq	%rcx, 80(%rsp)
	movq	%rax, 88(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.59(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.60(%rip), %r8
	leaq	64(%rsp), %rdx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	ud2
.Lfunc_end37:
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

	.def	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4main;
	.scl	2;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4main,unique,41
	.globl	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4main
	.p2align	4
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4main:
.Lfunc_begin40:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.71(%rip), %rcx
	movl	$4, %edx
	jmp	_RNvCs6COpcv5Z6wJ_4test16test_main_static
.Lfunc_end40:

	.def	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.p2align	4
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step:
.Lfunc_begin41:
.seh_proc _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$568, %rsp
	.seh_stackalloc 568
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 432(%rbp)
	movq	%r9, %rdi
	movq	%r8, %r14
	movq	%rdx, %rbx
	movq	%rcx, 256(%rbp)
	leaq	-24(%rbp), %rcx
	callq	_RNvNtNtNtCsaZOKLwlFm27_3std3sys5paths7windows11current_exe.llvm.14433937248639318539
	movq	-24(%rbp), %rsi
	cmpq	$-1, %rsi
	je	.LBB41_189
	movq	-16(%rbp), %r13
	movq	-8(%rbp), %r15
	testq	%r15, %r15
	je	.LBB41_4
	movl	$1, %edx
	movq	%r15, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB41_205
	movq	%rax, %r12
	movq	%rax, %rcx
	movq	%r13, %rdx
	movq	%r15, %r8
	callq	memcpy
	jmp	.LBB41_5
.LBB41_4:
	movl	$1, %r12d
.LBB41_5:
	movq	%r15, -24(%rbp)
	movq	%r12, -16(%rbp)
	movq	%r15, -8(%rbp)
	movb	$0, (%rbp)
	movq	$0, 8(%rbp)
	movq	$8, 16(%rbp)
	movq	$0, 24(%rbp)
	movq	$-1, 32(%rbp)
	movl	$-1, 64(%rbp)
	movl	$-1, 80(%rbp)
	movl	$-1, 96(%rbp)
	movq	$0, 112(%rbp)
	movq	$0, 128(%rbp)
	movw	$0, 136(%rbp)
	movw	$0, 144(%rbp)
	movq	$0, 148(%rbp)
	movw	$513, 156(%rbp)
	testq	%rsi, %rsi
	je	.LBB41_7
	movl	$1, %r8d
	movq	%r13, %rcx
	movq	%rsi, %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB41_7:
	movl	$7, %ecx
	movl	$1, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB41_201
	leaq	8(%rbp), %r15
	movl	$1952670072, 3(%rax)
	movq	%rax, 424(%rbp)
	movl	$2019896621, (%rax)
.Ltmp142:
	movq	%r15, %rcx
	callq	_RNvMs3_NtCsdkPGTeTFVes_5alloc7raw_vecINtB5_6RawVecNtNtNtNtCsaZOKLwlFm27_3std3sys4args7windows3ArgE8grow_oneBU_
	nop
.Ltmp143:
	movq	16(%rbp), %rax
	movq	$0, (%rax)
	movq	$7, 8(%rax)
	movq	424(%rbp), %rcx
	movq	%rcx, 16(%rax)
	movq	$7, 24(%rax)
	movb	$0, 32(%rax)
	movq	$1, 24(%rbp)
	movl	$17, %ecx
	movl	$1, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB41_202
	movups	anon.22844a9aef6a14b72687ebf27443efb5.66(%rip), %xmm0
	movups	%xmm0, (%rax)
	movq	%rax, 424(%rbp)
	movb	$100, 16(%rax)
	cmpq	$1, 8(%rbp)
	jne	.LBB41_12
.Ltmp144:
	movq	%r15, %rcx
	callq	_RNvMs3_NtCsdkPGTeTFVes_5alloc7raw_vecINtB5_6RawVecNtNtNtNtCsaZOKLwlFm27_3std3sys4args7windows3ArgE8grow_oneBU_
	nop
.Ltmp145:
.LBB41_12:
	movq	16(%rbp), %rax
	movq	$0, 40(%rax)
	movq	$17, 48(%rax)
	movq	424(%rbp), %rcx
	movq	%rcx, 56(%rax)
	movq	$17, 64(%rax)
	movb	$0, 72(%rax)
	movq	$2, 24(%rbp)
	movl	$9, %ecx
	movl	$1, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB41_203
	movabsq	$7310027665147768109, %rcx
	movq	%rcx, (%rax)
	movq	%rax, 424(%rbp)
	movb	$100, 8(%rax)
	cmpq	$2, 8(%rbp)
	jne	.LBB41_15
.Ltmp146:
	movq	%r15, %rcx
	callq	_RNvMs3_NtCsdkPGTeTFVes_5alloc7raw_vecINtB5_6RawVecNtNtNtNtCsaZOKLwlFm27_3std3sys4args7windows3ArgE8grow_oneBU_
	nop
.Ltmp147:
.LBB41_15:
	movq	16(%rbp), %rsi
	movq	$0, 80(%rsi)
	movq	$9, 88(%rsi)
	movq	424(%rbp), %rax
	movq	%rax, 96(%rsi)
	movq	$9, 104(%rsi)
	movb	$0, 112(%rsi)
	movq	$3, 24(%rbp)
	movl	$11, %ecx
	movl	$1, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB41_204
	movabsq	$8390313185490382125, %rcx
	movq	%rcx, (%rax)
	movl	$1701999988, 7(%rax)
	cmpq	$3, 8(%rbp)
	jne	.LBB41_19
.Ltmp148:
	movq	%rax, 424(%rbp)
	movq	%r15, %rcx
	callq	_RNvMs3_NtCsdkPGTeTFVes_5alloc7raw_vecINtB5_6RawVecNtNtNtNtCsaZOKLwlFm27_3std3sys4args7windows3ArgE8grow_oneBU_
	nop
.Ltmp149:
	movq	16(%rbp), %rsi
	movq	424(%rbp), %rax
.LBB41_19:
	leaq	112(%rbp), %r15
	movq	$0, 120(%rsi)
	movq	$11, 128(%rsi)
	movq	%rax, 136(%rsi)
	movq	$11, 144(%rsi)
	movb	$0, 152(%rsi)
	movq	$4, 24(%rbp)
.Ltmp150:
	movq	%r14, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.77(%rip), %rdx
	movl	$20, %r8d
	movq	%r15, %rcx
	movq	%rbx, %r9
	callq	_RNvMs_NtNtNtCsaZOKLwlFm27_3std3sys7process3envNtB4_10CommandEnv3set
	nop
.Ltmp151:
.Ltmp152:
	leaq	320(%rbp), %rdx
	movl	$20, %r8d
	movq	%rdi, %rcx
	callq	_RNvMsk_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impj10__fmt_inner.llvm.3738276955999817974
	nop
.Ltmp153:
	movq	%rax, %rbx
	leaq	-20(%rax), %rsi
	negq	%rsi
	movl	$1, %edi
	cmpq	$20, %rax
	je	.LBB41_24
	movl	$1, %edx
	movq	%rsi, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB41_206
	movq	%rax, %rdi
	leaq	(%rbx,%rbp), %rdx
	addq	$320, %rdx
	movq	%rax, %rcx
	movq	%rsi, %r8
	callq	memcpy
	nop
.LBB41_24:
.Ltmp156:
	movq	%rbx, 424(%rbp)
	movq	%rsi, 416(%rbp)
	movq	%rsi, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.78(%rip), %rdx
	movl	$13, %r8d
	movq	%r15, %rcx
	movq	%rdi, 408(%rbp)
	movq	%rdi, %r9
	callq	_RNvMs_NtNtNtCsaZOKLwlFm27_3std3sys7process3envNtB4_10CommandEnv3set
	nop
.Ltmp157:
	movq	544(%rbp), %rsi
	cmpq	$20, 424(%rbp)
	je	.LBB41_27
	movl	$1, %r8d
	movq	408(%rbp), %rcx
	movq	416(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB41_27:
	cmpq	$1, %rsi
	jne	.LBB41_35
.Ltmp158:
	movq	552(%rbp), %rcx
	leaq	320(%rbp), %rdx
	movl	$20, %r8d
	callq	_RNvMsk_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impj10__fmt_inner.llvm.3738276955999817974
	nop
.Ltmp159:
	movq	%rax, %rbx
	leaq	-20(%rax), %rsi
	negq	%rsi
	movl	$1, %edi
	cmpq	$20, %rax
	je	.LBB41_32
	movl	$1, %edx
	movq	%rsi, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB41_207
	movq	%rax, %rdi
	leaq	(%rbx,%rbp), %rdx
	addq	$320, %rdx
	movq	%rax, %rcx
	movq	%rsi, %r8
	callq	memcpy
	nop
.LBB41_32:
.Ltmp162:
	movq	%rbx, 424(%rbp)
	movq	%rsi, 416(%rbp)
	movq	%rsi, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.79(%rip), %rdx
	movl	$16, %r8d
	movq	%r15, %rcx
	movq	%rdi, 408(%rbp)
	movq	%rdi, %r9
	callq	_RNvMs_NtNtNtCsaZOKLwlFm27_3std3sys7process3envNtB4_10CommandEnv3set
	nop
.Ltmp163:
	cmpq	$20, 424(%rbp)
	je	.LBB41_35
	movl	$1, %r8d
	movq	408(%rbp), %rcx
	movq	416(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB41_35:
.Ltmp164:
	leaq	320(%rbp), %rcx
	leaq	-24(%rbp), %rdx
	callq	_RNvMsk_NtCsaZOKLwlFm27_3std7processNtB5_7Command6output
	nop
.Ltmp165:
	cmpq	$-1, 320(%rbp)
	je	.LBB41_190
	movq	368(%rbp), %rax
	movq	%rax, 208(%rbp)
	movups	320(%rbp), %xmm0
	movups	336(%rbp), %xmm1
	movups	352(%rbp), %xmm2
	movaps	%xmm2, 192(%rbp)
	movaps	%xmm1, 176(%rbp)
	movaps	%xmm0, 160(%rbp)
	testl	%eax, %eax
	jne	.LBB41_78
	movq	168(%rbp), %rdx
	movq	176(%rbp), %r8
.Ltmp166:
	leaq	-48(%rbp), %rcx
	callq	_RNvMNtCsdkPGTeTFVes_5alloc6stringNtB2_6String15from_utf8_lossy
	nop
.Ltmp167:
	movq	-48(%rbp), %rax
	movq	%rax, 264(%rbp)
	movq	-40(%rbp), %r12
	movq	-32(%rbp), %rax
	leaq	8(%r12), %rcx
	movq	%rcx, 416(%rbp)
	xorl	%r10d, %r10d
	movabsq	$-723401728380766731, %rdx
	movabsq	$72340172838076673, %r8
	movabsq	$-9187201950435737472, %r9
	xorl	%r11d, %r11d
	movq	%r12, 424(%rbp)
	jmp	.LBB41_41
	.p2align	4
.LBB41_40:
	movq	%rdi, %r11
	testb	%sil, %sil
	jne	.LBB41_195
.LBB41_41:
	cmpq	%r10, %rax
	jae	.LBB41_44
	movq	%r10, %rdi
	jmp	.LBB41_66
	.p2align	4
.LBB41_43:
	movq	%rdi, %r10
	cmpq	%rax, %rdi
	ja	.LBB41_66
.LBB41_44:
	movq	%rax, %rdi
	subq	%r10, %rdi
	leaq	(%r12,%r10), %rsi
	cmpq	$16, %rdi
	jae	.LBB41_49
	cmpq	%r10, %rax
	je	.LBB41_65
	xorl	%r14d, %r14d
	.p2align	4
.LBB41_47:
	cmpb	$10, (%rsi,%r14)
	je	.LBB41_61
	incq	%r14
	cmpq	%r14, %rdi
	jne	.LBB41_47
	jmp	.LBB41_65
	.p2align	4
.LBB41_49:
	leaq	7(%rsi), %rbx
	andq	$-8, %rbx
	subq	%rsi, %rbx
	jne	.LBB41_54
	leaq	-16(%rdi), %r14
	xorl	%ebx, %ebx
.LBB41_51:
	movq	416(%rbp), %rcx
	leaq	(%rcx,%r10), %r15
	.p2align	4
.LBB41_52:
	movq	-8(%r15,%rbx), %rcx
	movq	(%r15,%rbx), %r13
	movq	%rcx, %r12
	xorq	%rdx, %r12
	addq	%r8, %r12
	orq	%rcx, %r12
	movq	%r13, %rcx
	xorq	%rdx, %rcx
	addq	%r8, %rcx
	orq	%r13, %rcx
	andq	%r12, %rcx
	notq	%rcx
	testq	%r9, %rcx
	jne	.LBB41_58
	addq	$16, %rbx
	cmpq	%r14, %rbx
	jbe	.LBB41_52
	jmp	.LBB41_58
	.p2align	4
.LBB41_54:
	xorl	%r14d, %r14d
	.p2align	4
.LBB41_55:
	cmpb	$10, (%rsi,%r14)
	je	.LBB41_61
	incq	%r14
	cmpq	%r14, %rbx
	jne	.LBB41_55
	leaq	-16(%rdi), %r14
	cmpq	%r14, %rbx
	jbe	.LBB41_51
	.p2align	4
.LBB41_58:
	movq	%rbx, %r14
	cmpq	%rbx, %rdi
	movq	424(%rbp), %r12
	je	.LBB41_65
	.p2align	4
.LBB41_59:
	cmpb	$10, (%rsi,%r14)
	je	.LBB41_61
	incq	%r14
	cmpq	%r14, %rdi
	jne	.LBB41_59
	jmp	.LBB41_65
	.p2align	4
.LBB41_61:
	leaq	(%r14,%r10), %rdi
	incq	%rdi
	addq	%r14, %r10
	cmpq	%rax, %r10
	jae	.LBB41_43
	cmpb	$10, (%rsi,%r14)
	jne	.LBB41_43
	xorl	%esi, %esi
	movq	%rdi, %r10
	movq	%rdi, %rbx
	jmp	.LBB41_67
	.p2align	4
.LBB41_65:
	movq	%rax, %rdi
.LBB41_66:
	movb	$1, %sil
	movq	%rdi, %r10
	movq	%r11, %rdi
	movq	%rax, %rbx
	cmpq	%r11, %rax
	je	.LBB41_195
.LBB41_67:
	leaq	(%r12,%r11), %r14
	subq	%r11, %rbx
	je	.LBB41_73
	leaq	-1(%rbx), %r11
	cmpb	$10, (%r14,%r11)
	jne	.LBB41_73
	addq	$-2, %rbx
	testq	%r11, %r11
	je	.LBB41_71
	cmpb	$13, (%r14,%rbx)
	movl	$0, %r15d
	cmoveq	%r14, %r15
	jmp	.LBB41_72
.LBB41_71:
	xorl	%r15d, %r15d
.LBB41_72:
	testq	%r15, %r15
	cmoveq	%r11, %rbx
	cmovneq	%r15, %r14
.LBB41_73:
	testq	%r14, %r14
	je	.LBB41_195
	testq	%rbx, %rbx
	je	.LBB41_40
	movzbl	(%r14), %r15d
	cmpb	$123, %r15b
	sete	%cl
	orb	%cl, %sil
	movq	%rdi, %r11
	je	.LBB41_41
	cmpb	$123, %r15b
	jne	.LBB41_195
	movq	%r14, 344(%rbp)
	movq	%rbx, 352(%rbp)
	movq	$0, 360(%rbp)
	movq	$0, 320(%rbp)
	movq	$1, 328(%rbp)
	movq	$0, 336(%rbp)
	movb	$-128, 368(%rbp)
.Ltmp168:
	leaq	272(%rbp), %rcx
	leaq	320(%rbp), %rdx
	callq	_RINvXNtNtCsfZWMcdrZHWk_10serde_json5value2deNtB5_5ValueNtNtCsidvd3kPC9MK_10serde_core2de11Deserialize11deserializeQINtNtB7_2de12DeserializerNtNtB7_4read7StrReadEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp169:
	cmpb	$-1, 272(%rbp)
	je	.LBB41_198
	movups	272(%rbp), %xmm0
	movups	288(%rbp), %xmm1
	movaps	%xmm1, 240(%rbp)
	movaps	%xmm0, 224(%rbp)
	movq	352(%rbp), %rax
	movq	360(%rbp), %rcx
	cmpq	%rax, %rcx
	jae	.LBB41_88
	movq	344(%rbp), %rdx
	negq	%rax
	incq	%rcx
	movabsq	$4294977024, %r8
	.p2align	4
.LBB41_85:
	movzbl	-1(%rdx,%rcx), %r9d
	cmpq	$32, %r9
	ja	.LBB41_196
	btq	%r9, %r8
	jae	.LBB41_196
	movq	%rcx, 360(%rbp)
	leaq	(%rax,%rcx), %r9
	incq	%r9
	incq	%rcx
	cmpq	$1, %r9
	jne	.LBB41_85
.LBB41_88:
	movq	320(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB41_90
	movq	328(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB41_90:
	movups	272(%rbp), %xmm0
	movups	288(%rbp), %xmm1
	movaps	%xmm1, 336(%rbp)
	movaps	%xmm0, 320(%rbp)
	movzbl	320(%rbp), %edx
	cmpb	$5, %dl
	movb	%dl, 416(%rbp)
	jne	.LBB41_101
	movq	328(%rbp), %rcx
	testq	%rcx, %rcx
	je	.LBB41_101
	movq	336(%rbp), %rax
	movq	%rax, 408(%rbp)
	movl	$10, %r15d
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.37(%rip), %rbx
.LBB41_93:
	leaq	360(%rcx), %rax
	movzwl	626(%rcx), %edx
	movq	%rcx, 392(%rbp)
	leaq	-32(%rcx), %r14
	movq	%rdx, 400(%rbp)
	leal	(,%rdx,8), %ecx
	leaq	(%rcx,%rcx,2), %r13
	movq	$-1, %r12
	.p2align	4
.LBB41_94:
	testq	%r13, %r13
	je	.LBB41_97
	leaq	24(%rax), %rdi
	movq	8(%rax), %rdx
	movq	16(%rax), %r8
	movq	%r8, %rsi
	subq	$10, %rsi
	cmovaeq	%r15, %r8
	negq	%rsi
	movq	%rbx, %rcx
	callq	memcmp
	cltq
	testl	%eax, %eax
	cmovneq	%rax, %rsi
	testq	%rsi, %rsi
	sets	%al
	setg	%cl
	subb	%al, %cl
	addq	$32, %r14
	incq	%r12
	addq	$-24, %r13
	movq	%rdi, %rax
	cmpb	$1, %cl
	je	.LBB41_94
	movzbl	%cl, %eax
	testl	%eax, %eax
	je	.LBB41_103
	jmp	.LBB41_98
.LBB41_97:
	movq	400(%rbp), %r12
.LBB41_98:
	movq	408(%rbp), %rax
	subq	$1, %rax
	jb	.LBB41_102
	movq	%rax, 408(%rbp)
	movq	392(%rbp), %rcx
	movq	632(%rcx,%r12,8), %rcx
	jmp	.LBB41_93
.LBB41_78:
	cmpl	$-1073741571, %eax
	jne	.LBB41_191
	movq	256(%rbp), %rax
	movq	$1, (%rax)
	jmp	.LBB41_177
.LBB41_101:
	xorl	%r14d, %r14d
	jmp	.LBB41_104
.LBB41_102:
	xorl	%r14d, %r14d
.LBB41_103:
	movzbl	416(%rbp), %edx
.LBB41_104:
	testq	%r14, %r14
	leaq	_RNvNvXs3_NtNtCsfZWMcdrZHWk_10serde_json5value5indexNtB9_5ValueINtNtNtCsd9eshbB3hfz_4core3ops5index5IndexpE5index4NULL(%rip), %rax
	cmoveq	%rax, %r14
	cmpb	$2, (%r14)
	jne	.LBB41_184
	cmpq	$0, 8(%r14)
	jne	.LBB41_184
	movq	16(%r14), %rcx
	cmpb	$5, %dl
	movq	%rcx, 408(%rbp)
	jne	.LBB41_117
	movq	328(%rbp), %r8
	testq	%r8, %r8
	je	.LBB41_117
	movq	336(%rbp), %rax
	movq	%rax, 392(%rbp)
	movl	$9, %r13d
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.38(%rip), %rbx
.LBB41_109:
	leaq	360(%r8), %rax
	movzwl	626(%r8), %ecx
	movq	%r8, 400(%rbp)
	leaq	-32(%r8), %r14
	movq	%rcx, 384(%rbp)
	leal	(,%rcx,8), %ecx
	leaq	(%rcx,%rcx,2), %r15
	movq	$-1, %r12
	.p2align	4
.LBB41_110:
	testq	%r15, %r15
	je	.LBB41_113
	leaq	24(%rax), %rdi
	movq	8(%rax), %rdx
	movq	16(%rax), %r8
	movq	%r8, %rsi
	subq	$9, %rsi
	cmovaeq	%r13, %r8
	negq	%rsi
	movq	%rbx, %rcx
	callq	memcmp
	cltq
	testl	%eax, %eax
	cmovneq	%rax, %rsi
	testq	%rsi, %rsi
	sets	%al
	setg	%cl
	subb	%al, %cl
	addq	$32, %r14
	incq	%r12
	addq	$-24, %r15
	movq	%rdi, %rax
	cmpb	$1, %cl
	je	.LBB41_110
	movzbl	%cl, %eax
	testl	%eax, %eax
	je	.LBB41_119
	jmp	.LBB41_114
.LBB41_113:
	movq	384(%rbp), %r12
.LBB41_114:
	movq	392(%rbp), %rax
	subq	$1, %rax
	jb	.LBB41_118
	movq	%rax, 392(%rbp)
	movq	400(%rbp), %r8
	movq	632(%r8,%r12,8), %r8
	jmp	.LBB41_109
.LBB41_117:
	xorl	%r14d, %r14d
	testq	%r14, %r14
	cmoveq	%rax, %r14
	cmpb	$2, (%r14)
	je	.LBB41_121
	jmp	.LBB41_185
.LBB41_118:
	xorl	%r14d, %r14d
.LBB41_119:
	movq	408(%rbp), %rcx
	leaq	_RNvNvXs3_NtNtCsfZWMcdrZHWk_10serde_json5value5indexNtB9_5ValueINtNtNtCsd9eshbB3hfz_4core3ops5index5IndexpE5index4NULL(%rip), %rax
	movzbl	416(%rbp), %edx
	testq	%r14, %r14
	cmoveq	%rax, %r14
	cmpb	$2, (%r14)
	jne	.LBB41_185
.LBB41_121:
	cmpq	$0, 8(%r14)
	jne	.LBB41_185
	movq	16(%r14), %rbx
	cmpb	$5, %dl
	movq	%rbx, 392(%rbp)
	jne	.LBB41_133
	movq	328(%rbp), %r8
	testq	%r8, %r8
	je	.LBB41_133
	movq	336(%rbp), %rax
	movq	%rax, 400(%rbp)
	movl	$9, %r12d
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.39(%rip), %rbx
.LBB41_125:
	leaq	360(%r8), %rax
	movzwl	626(%r8), %ecx
	movq	%r8, 384(%rbp)
	leaq	-32(%r8), %r14
	movq	%rcx, 312(%rbp)
	leal	(,%rcx,8), %ecx
	leaq	(%rcx,%rcx,2), %r15
	movq	$-1, %r13
	.p2align	4
.LBB41_126:
	testq	%r15, %r15
	je	.LBB41_129
	leaq	24(%rax), %rdi
	movq	8(%rax), %rdx
	movq	16(%rax), %r8
	movq	%r8, %rsi
	subq	$9, %rsi
	cmovaeq	%r12, %r8
	negq	%rsi
	movq	%rbx, %rcx
	callq	memcmp
	cltq
	testl	%eax, %eax
	cmovneq	%rax, %rsi
	testq	%rsi, %rsi
	sets	%al
	setg	%cl
	subb	%al, %cl
	addq	$32, %r14
	incq	%r13
	addq	$-24, %r15
	movq	%rdi, %rax
	cmpb	$1, %cl
	je	.LBB41_126
	movzbl	%cl, %eax
	testl	%eax, %eax
	je	.LBB41_135
	jmp	.LBB41_130
.LBB41_129:
	movq	312(%rbp), %r13
.LBB41_130:
	movq	400(%rbp), %rax
	subq	$1, %rax
	jb	.LBB41_134
	movq	%rax, 400(%rbp)
	movq	384(%rbp), %r8
	movq	632(%r8,%r13,8), %r8
	jmp	.LBB41_125
.LBB41_133:
	xorl	%r14d, %r14d
	testq	%r14, %r14
	cmoveq	%rax, %r14
	cmpb	$2, (%r14)
	je	.LBB41_137
	jmp	.LBB41_186
.LBB41_134:
	xorl	%r14d, %r14d
.LBB41_135:
	movq	408(%rbp), %rcx
	leaq	_RNvNvXs3_NtNtCsfZWMcdrZHWk_10serde_json5value5indexNtB9_5ValueINtNtNtCsd9eshbB3hfz_4core3ops5index5IndexpE5index4NULL(%rip), %rax
	movzbl	416(%rbp), %edx
	movq	392(%rbp), %rbx
	testq	%r14, %r14
	cmoveq	%rax, %r14
	cmpb	$2, (%r14)
	jne	.LBB41_186
.LBB41_137:
	cmpq	$0, 8(%r14)
	jne	.LBB41_186
	movq	16(%r14), %r8
	cmpb	$5, %dl
	jne	.LBB41_149
	movq	328(%rbp), %r9
	testq	%r9, %r9
	je	.LBB41_149
	movq	%r8, 400(%rbp)
	movq	336(%rbp), %rax
	movq	%rax, 384(%rbp)
	movl	$10, %r12d
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.40(%rip), %rbx
.LBB41_141:
	leaq	360(%r9), %rax
	movzwl	626(%r9), %ecx
	movq	%r9, 312(%rbp)
	leaq	-32(%r9), %rdi
	movq	%rcx, 304(%rbp)
	leal	(,%rcx,8), %ecx
	leaq	(%rcx,%rcx,2), %r15
	movq	$-1, %r13
	.p2align	4
.LBB41_142:
	testq	%r15, %r15
	je	.LBB41_145
	leaq	24(%rax), %r14
	movq	8(%rax), %rdx
	movq	16(%rax), %r8
	movq	%r8, %rsi
	subq	$10, %rsi
	cmovaeq	%r12, %r8
	negq	%rsi
	movq	%rbx, %rcx
	callq	memcmp
	cltq
	testl	%eax, %eax
	cmovneq	%rax, %rsi
	testq	%rsi, %rsi
	sets	%al
	setg	%cl
	subb	%al, %cl
	addq	$32, %rdi
	incq	%r13
	addq	$-24, %r15
	movq	%r14, %rax
	cmpb	$1, %cl
	je	.LBB41_142
	movzbl	%cl, %eax
	testl	%eax, %eax
	je	.LBB41_151
	jmp	.LBB41_146
.LBB41_145:
	movq	304(%rbp), %r13
.LBB41_146:
	movq	384(%rbp), %rax
	subq	$1, %rax
	jb	.LBB41_150
	movq	%rax, 384(%rbp)
	movq	312(%rbp), %r9
	movq	632(%r9,%r13,8), %r9
	jmp	.LBB41_141
.LBB41_149:
	xorl	%edi, %edi
	testq	%rdi, %rdi
	cmoveq	%rax, %rdi
	cmpb	$2, (%rdi)
	je	.LBB41_153
	jmp	.LBB41_187
.LBB41_150:
	xorl	%edi, %edi
.LBB41_151:
	movq	408(%rbp), %rcx
	leaq	_RNvNvXs3_NtNtCsfZWMcdrZHWk_10serde_json5value5indexNtB9_5ValueINtNtNtCsd9eshbB3hfz_4core3ops5index5IndexpE5index4NULL(%rip), %rax
	movzbl	416(%rbp), %edx
	movq	392(%rbp), %rbx
	movq	400(%rbp), %r8
	testq	%rdi, %rdi
	cmoveq	%rax, %rdi
	cmpb	$2, (%rdi)
	jne	.LBB41_187
.LBB41_153:
	cmpq	$0, 8(%rdi)
	jne	.LBB41_187
	movq	16(%rdi), %r9
	cmpb	$5, %dl
	jne	.LBB41_167
	movq	328(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB41_167
	movq	%r9, 312(%rbp)
	movq	%r8, 400(%rbp)
	movq	336(%rbp), %rax
	movq	%rax, 416(%rbp)
	movl	$13, %edi
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.41(%rip), %rbx
.LBB41_157:
	leaq	360(%rdx), %rax
	movzwl	626(%rdx), %ecx
	movq	%rdx, 384(%rbp)
	leaq	-32(%rdx), %r13
	movq	%rcx, 304(%rbp)
	leal	(,%rcx,8), %ecx
	leaq	(%rcx,%rcx,2), %r15
	movq	$-1, %rsi
	.p2align	4
.LBB41_158:
	testq	%r15, %r15
	je	.LBB41_162
	leaq	24(%rax), %r14
	movq	8(%rax), %rdx
	movq	16(%rax), %r8
	movq	%r8, %r12
	subq	$13, %r12
	cmovaeq	%rdi, %r8
	negq	%r12
	movq	%rbx, %rcx
	callq	memcmp
	cltq
	testl	%eax, %eax
	cmovneq	%rax, %r12
	testq	%r12, %r12
	sets	%al
	setg	%cl
	subb	%al, %cl
	addq	$32, %r13
	incq	%rsi
	addq	$-24, %r15
	movq	%r14, %rax
	cmpb	$1, %cl
	je	.LBB41_158
	movzbl	%cl, %eax
	testl	%eax, %eax
	movq	312(%rbp), %r9
	je	.LBB41_166
	movq	400(%rbp), %r8
	jmp	.LBB41_163
.LBB41_162:
	movq	304(%rbp), %rsi
	movq	400(%rbp), %r8
	movq	312(%rbp), %r9
.LBB41_163:
	movq	416(%rbp), %rax
	subq	$1, %rax
	jb	.LBB41_182
	movq	%rax, 416(%rbp)
	movq	384(%rbp), %rdx
	movq	632(%rdx,%rsi,8), %rdx
	jmp	.LBB41_157
.LBB41_167:
	xorl	%r13d, %r13d
	testq	%r13, %r13
	cmovneq	%r13, %rax
	cmpb	$2, (%rax)
	je	.LBB41_168
	jmp	.LBB41_183
.LBB41_166:
	movq	408(%rbp), %rcx
	leaq	_RNvNvXs3_NtNtCsfZWMcdrZHWk_10serde_json5value5indexNtB9_5ValueINtNtNtCsd9eshbB3hfz_4core3ops5index5IndexpE5index4NULL(%rip), %rax
	movq	392(%rbp), %rbx
	movq	400(%rbp), %r8
	testq	%r13, %r13
	cmovneq	%r13, %rax
	cmpb	$2, (%rax)
	jne	.LBB41_183
.LBB41_168:
	cmpq	$0, 8(%rax)
	jne	.LBB41_183
	movq	16(%rax), %rsi
	movq	%rsi, 272(%rbp)
	movq	%r9, %rax
	subq	%r8, %rax
	movq	%rax, 224(%rbp)
	cmpq	$2097152, %rax
	jne	.LBB41_193
	movq	%rcx, %rdi
	subq	%rbx, %rdi
	jbe	.LBB41_188
	subq	%r8, %rbx
	jbe	.LBB41_188
	cmpq	%r9, %rcx
	jae	.LBB41_188
	leaq	-1(%rsi), %rax
	cmpq	$512, %rax
	jae	.LBB41_194
.Ltmp186:
	leaq	320(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp187:
	movq	256(%rbp), %rax
	movq	%rbx, 8(%rax)
	movq	%rdi, 16(%rax)
	movq	%rsi, 24(%rax)
	movq	$0, (%rax)
	movq	264(%rbp), %rdx
	testq	%rdx, %rdx
	jle	.LBB41_177
	movl	$1, %r8d
	movq	424(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB41_177:
	movq	160(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB41_179
	movq	168(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB41_179:
	movq	184(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB41_181
	movq	192(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB41_181:
	leaq	-24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$568, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB41_182:
	xorl	%r13d, %r13d
	movq	408(%rbp), %rcx
	leaq	_RNvNvXs3_NtNtCsfZWMcdrZHWk_10serde_json5value5indexNtB9_5ValueINtNtNtCsd9eshbB3hfz_4core3ops5index5IndexpE5index4NULL(%rip), %rax
	movq	392(%rbp), %rbx
	testq	%r13, %r13
	cmovneq	%r13, %rax
	cmpb	$2, (%rax)
	je	.LBB41_168
.LBB41_183:
.Ltmp178:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.49(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.50(%rip), %r8
	movl	$6, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13expect_failed
	nop
.Ltmp179:
	jmp	.LBB41_208
.LBB41_184:
.Ltmp170:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.57(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.58(%rip), %r8
	movl	$5, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13expect_failed
	nop
.Ltmp171:
	jmp	.LBB41_208
.LBB41_185:
.Ltmp172:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.55(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.56(%rip), %r8
	movl	$4, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13expect_failed
	nop
.Ltmp173:
	jmp	.LBB41_208
.LBB41_186:
.Ltmp174:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.53(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.54(%rip), %r8
	movl	$3, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13expect_failed
	nop
.Ltmp175:
	jmp	.LBB41_208
.LBB41_187:
.Ltmp176:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.51(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.52(%rip), %r8
	movl	$4, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13expect_failed
	nop
.Ltmp177:
	jmp	.LBB41_208
.LBB41_188:
.Ltmp182:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.43(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.44(%rip), %r8
	movl	$33, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	nop
.Ltmp183:
	jmp	.LBB41_208
.LBB41_189:
	movq	-16(%rbp), %rax
	movq	%rax, 320(%rbp)
.Ltmp210:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.73(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.72(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.141(%rip), %r9
	leaq	320(%rbp), %r8
	movl	$11, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp211:
	jmp	.LBB41_208
.LBB41_190:
	movq	328(%rbp), %rax
	movq	%rax, 160(%rbp)
.Ltmp200:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.81(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.80(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.141(%rip), %r9
	leaq	160(%rbp), %r8
	movl	$11, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp201:
	jmp	.LBB41_208
.LBB41_191:
	movl	$1, 272(%rbp)
	movl	%eax, 276(%rbp)
	movq	192(%rbp), %rdx
	movq	200(%rbp), %r8
.Ltmp196:
	leaq	224(%rbp), %rsi
	movq	%rsi, %rcx
	callq	_RNvMNtCsdkPGTeTFVes_5alloc6stringNtB2_6String15from_utf8_lossy
	nop
.Ltmp197:
	leaq	272(%rbp), %rax
	movq	%rax, 320(%rbp)
	leaq	_RNvXsR_NtCsd9eshbB3hfz_4core6optionINtB5_6OptionlENtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack(%rip), %rax
	movq	%rax, 328(%rbp)
	movq	%rsi, 336(%rbp)
	leaq	_RNvXsb_NtCsdkPGTeTFVes_5alloc6borrowINtB5_3CoweENtNtCsd9eshbB3hfz_4core3fmt7Display3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack(%rip), %rax
	movq	%rax, 344(%rbp)
.Ltmp198:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.108(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.109(%rip), %r8
	leaq	320(%rbp), %rdx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	nop
.Ltmp199:
	jmp	.LBB41_208
.LBB41_193:
.Ltmp180:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.48(%rip), %rax
	movq	%rax, 40(%rsp)
	movq	$53, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.42(%rip), %r8
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.47(%rip), %r9
	leaq	224(%rbp), %rdx
	xorl	%ecx, %ecx
	callq	_RINvNtCsd9eshbB3hfz_4core9panicking13assert_failedjjEB4_
	nop
.Ltmp181:
	jmp	.LBB41_208
.LBB41_194:
	leaq	272(%rbp), %rax
	movq	%rax, 224(%rbp)
	leaq	_RNvXsi_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impjNtB9_7Display3fmt(%rip), %rax
	movq	%rax, 232(%rbp)
.Ltmp184:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.45(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.46(%rip), %r8
	leaq	224(%rbp), %rdx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	nop
.Ltmp185:
	jmp	.LBB41_208
.LBB41_195:
.Ltmp194:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.33(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.34(%rip), %r8
	movl	$17, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13expect_failed
	nop
.Ltmp195:
	jmp	.LBB41_208
.LBB41_196:
	movq	$22, -72(%rbp)
.Ltmp188:
	leaq	320(%rbp), %rcx
	leaq	-72(%rbp), %rdx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp189:
	movq	%rax, 280(%rbp)
.Ltmp190:
	leaq	224(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp191:
.LBB41_198:
	movq	320(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB41_200
	movq	328(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB41_200:
	movq	280(%rbp), %rax
	movq	%rax, 320(%rbp)
.Ltmp192:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.36(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.35(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.143(%rip), %r9
	leaq	320(%rbp), %r8
	movl	$16, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp193:
	jmp	.LBB41_208
.LBB41_201:
.Ltmp208:
	movl	$1, %ecx
	movl	$7, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp209:
	jmp	.LBB41_208
.LBB41_202:
.Ltmp206:
	movl	$1, %ecx
	movl	$17, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp207:
	jmp	.LBB41_208
.LBB41_203:
.Ltmp204:
	movl	$1, %ecx
	movl	$9, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp205:
	jmp	.LBB41_208
.LBB41_204:
.Ltmp202:
	movl	$1, %ecx
	movl	$11, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp203:
	jmp	.LBB41_208
.LBB41_205:
.Ltmp140:
	movq	%r13, 416(%rbp)
	movq	%rsi, 424(%rbp)
	movl	$1, %ecx
	movq	%r15, %rdx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp141:
	jmp	.LBB41_208
.LBB41_206:
.Ltmp154:
	movl	$1, %ecx
	movq	%rsi, %rdx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp155:
	jmp	.LBB41_208
.LBB41_207:
.Ltmp160:
	movl	$1, %ecx
	movq	%rsi, %rdx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp161:
.LBB41_208:
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@IMGREL
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$209@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$209@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$209@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_209:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$20, 424(%rbp)
	je	.LBB41_211
	movl	$1, %r8d
	movq	408(%rbp), %rcx
	movq	416(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB41_211:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$212@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$212@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$212@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_212:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$11, %edx
	movl	$1, %r8d
	movq	424(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$213@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$213@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$213@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_213:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$9, %edx
	movl	$1, %r8d
	movq	424(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$214@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$214@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$214@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_214:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$17, %edx
	movl	$1, %r8d
	movq	424(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$215@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$215@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$215@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_215:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$0, 424(%rbp)
	je	.LBB41_217
	movl	$1, %r8d
	movq	416(%rbp), %rcx
	movq	424(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB41_217:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$218@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$218@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$218@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_218:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	272(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$219@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$219@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$219@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_219:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	320(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$220@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$220@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$220@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_220:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$20, 424(%rbp)
	je	.LBB41_222
	movl	$1, %r8d
	movq	408(%rbp), %rcx
	movq	416(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB41_222:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$223@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$223@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$223@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_223:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$7, %edx
	movl	$1, %r8d
	movq	424(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$224@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$224@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$224@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_224:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	320(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB41_226
	movq	328(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB41_226:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$227@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$227@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$227@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_227:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	224(%rbp), %rdx
	testq	%rdx, %rdx
	jle	.LBB41_229
	movq	232(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB41_229:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$230@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$230@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$230@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_230:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	160(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$231@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$231@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$231@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_231:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	320(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$232@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$232@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$232@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_232:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	320(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$233@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$233@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$233@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_233:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	264(%rbp), %rdx
	testq	%rdx, %rdx
	jle	.LBB41_235
	movl	$1, %r8d
	movq	424(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB41_235:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$236@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$236@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$236@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_236:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	160(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process6OutputECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.def	"?dtor$237@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$237@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA":
.seh_proc "?dtor$237@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"
.LBB41_237:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	-24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end41:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,19
	.p2align	2, 0x0
$cppxdata$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step:
	.long	429065506
	.long	17
	.long	$stateUnwindMap$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@IMGREL
	.long	0
	.long	0
	.long	37
	.long	$ip2state$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@IMGREL
	.long	560
	.long	0
	.long	1
$stateUnwindMap$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step:
	.long	-1
	.long	"?dtor$231@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	-1
	.long	"?dtor$215@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	-1
	.long	"?dtor$237@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	2
	.long	"?dtor$236@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	3
	.long	"?dtor$227@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	3
	.long	"?dtor$233@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	5
	.long	"?dtor$232@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	5
	.long	"?dtor$219@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	5
	.long	"?dtor$224@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	8
	.long	"?dtor$218@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	2
	.long	"?dtor$230@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	2
	.long	"?dtor$209@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	2
	.long	"?dtor$220@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	2
	.long	"?dtor$212@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	2
	.long	"?dtor$213@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	2
	.long	"?dtor$214@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
	.long	2
	.long	"?dtor$223@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step@4HA"@IMGREL
$ip2state$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step:
	.long	.Lfunc_begin41@IMGREL
	.long	-1
	.long	.Ltmp142@IMGREL
	.long	16
	.long	.Ltmp143@IMGREL
	.long	-1
	.long	.Ltmp144@IMGREL
	.long	15
	.long	.Ltmp145@IMGREL
	.long	-1
	.long	.Ltmp146@IMGREL
	.long	14
	.long	.Ltmp147@IMGREL
	.long	-1
	.long	.Ltmp148@IMGREL
	.long	13
	.long	.Ltmp150@IMGREL
	.long	2
	.long	.Ltmp153@IMGREL
	.long	-1
	.long	.Ltmp156@IMGREL
	.long	12
	.long	.Ltmp157@IMGREL
	.long	-1
	.long	.Ltmp158@IMGREL
	.long	2
	.long	.Ltmp159@IMGREL
	.long	-1
	.long	.Ltmp162@IMGREL
	.long	11
	.long	.Ltmp163@IMGREL
	.long	-1
	.long	.Ltmp164@IMGREL
	.long	2
	.long	.Ltmp166@IMGREL
	.long	3
	.long	.Ltmp168@IMGREL
	.long	8
	.long	.Ltmp169@IMGREL
	.long	-1
	.long	.Ltmp186@IMGREL
	.long	5
	.long	.Ltmp187@IMGREL
	.long	-1
	.long	.Ltmp178@IMGREL
	.long	6
	.long	.Ltmp210@IMGREL
	.long	0
	.long	.Ltmp200@IMGREL
	.long	10
	.long	.Ltmp196@IMGREL
	.long	3
	.long	.Ltmp198@IMGREL
	.long	4
	.long	.Ltmp180@IMGREL
	.long	6
	.long	.Ltmp194@IMGREL
	.long	5
	.long	.Ltmp188@IMGREL
	.long	9
	.long	.Ltmp190@IMGREL
	.long	8
	.long	.Ltmp191@IMGREL
	.long	-1
	.long	.Ltmp192@IMGREL
	.long	7
	.long	.Ltmp208@IMGREL
	.long	2
	.long	.Ltmp140@IMGREL
	.long	1
	.long	.Ltmp154@IMGREL
	.long	2
	.long	.Ltmp161@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step,unique,42

	.def	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43
	.p2align	4
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure:
.Lfunc_begin42:
.seh_proc _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$472, %rsp
	.seh_stackalloc 472
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 336(%rbp)
	movq	%rcx, 312(%rbp)
	movq	%rdx, 320(%rbp)
	movl	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL+32(%rip), %eax
	testl	%eax, %eax
	jne	.LBB42_115
.LBB42_1:
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL(%rip), %rax
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL+8(%rip), %rcx
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL+16(%rip), %rsi
	movzbl	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL+24(%rip), %ebx
	movq	%rax, 296(%rbp)
	movq	%rcx, -40(%rbp)
	movq	$0, 328(%rbp)
	movq	312(%rbp), %rdx
	movq	320(%rbp), %r8
	movq	%rax, 32(%rsp)
	leaq	-8(%rbp), %rdi
	movq	%rdi, %rcx
	movq	%rsi, %r9
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search
	movq	-8(%rbp), %rax
	movzbl	16(%rbp), %r14d
	movq	328(%rbp), %rcx
	cmpq	%rcx, %rax
	cmovaq	%rax, %rcx
	movq	%rcx, 328(%rbp)
	movq	312(%rbp), %rdx
	movq	320(%rbp), %r8
	movq	296(%rbp), %rax
	movq	%rax, 32(%rsp)
	movq	%rdi, %rcx
	movq	%rsi, %r9
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search
	movq	-8(%rbp), %rax
	orb	16(%rbp), %r14b
	movq	328(%rbp), %rcx
	cmpq	%rcx, %rax
	cmovaq	%rax, %rcx
	movq	%rcx, 328(%rbp)
	movq	312(%rbp), %rdx
	movq	320(%rbp), %r8
	movq	296(%rbp), %rax
	movq	%rax, 32(%rsp)
	movq	%rdi, %rcx
	movq	%rsi, %r9
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack15boundary_search
	movq	-8(%rbp), %rcx
	orb	16(%rbp), %r14b
	movq	328(%rbp), %rax
	cmpq	%rax, %rcx
	cmovaq	%rcx, %rax
	movq	%rax, 328(%rbp)
	testb	$1, %bl
	je	.LBB42_3
	addq	296(%rbp), %rax
	movq	%rax, %rcx
	jmp	.LBB42_4
.LBB42_3:
	xorl	%ecx, %ecx
	subq	-40(%rbp), %rax
	cmovaeq	%rax, %rcx
	addq	296(%rbp), %rcx
.LBB42_4:
	movq	%rcx, 288(%rbp)
	movq	$18432, 280(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.83(%rip), %rdx
	leaq	-8(%rbp), %rcx
	movl	$19, %r8d
	callq	_RNvNtCsaZOKLwlFm27_3std3env7__var_os
	movq	-8(%rbp), %rdx
	cmpq	$-1, %rdx
	je	.LBB42_16
	movq	(%rbp), %rcx
	cmpq	$1, 8(%rbp)
	jne	.LBB42_14
	movzbl	(%rcx), %esi
	testq	%rdx, %rdx
	je	.LBB42_8
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB42_8:
	cmpb	$49, %sil
	jne	.LBB42_16
	movq	288(%rbp), %rax
	cmpq	280(%rbp), %rax
	ja	.LBB42_117
	cmpq	$5, 320(%rbp)
	jne	.LBB42_13
	movq	312(%rbp), %rax
	movl	$1818324339, %ecx
	xorl	(%rax), %ecx
	movzbl	4(%rax), %edx
	xorl	$108, %edx
	orl	%ecx, %edx
	je	.LBB42_109
	movl	$1735549292, %ecx
	xorl	(%rax), %ecx
	movzbl	4(%rax), %eax
	xorl	$101, %eax
	orl	%ecx, %eax
	je	.LBB42_110
.LBB42_13:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.90(%rip), %rcx
	movl	$79, %edx
	jmp	.LBB42_112
.LBB42_14:
	testq	%rdx, %rdx
	je	.LBB42_16
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB42_16:
	xorl	%esi, %esi
.LBB42_17:
	movq	328(%rbp), %rax
	movq	312(%rbp), %rdx
	movq	320(%rbp), %r8
	movq	%rax, 40(%rsp)
	movq	$1, 32(%rsp)
	leaq	-8(%rbp), %rcx
	xorl	%r9d, %r9d
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4step
	cmpb	$0, -8(%rbp)
	jne	.LBB42_116
	movq	(%rbp), %rdx
	movq	%rdx, -64(%rbp)
	movq	328(%rbp), %r8
	movq	296(%rbp), %rax
	addq	%r8, %rax
	cmpq	%rax, %rdx
	setb	%cl
	cmpq	%r8, %rdx
	jb	.LBB42_113
	testb	%cl, %cl
	je	.LBB42_114
.LBB42_20:
	cmpq	$5, 320(%rbp)
	jne	.LBB42_23
	movq	312(%rbp), %rax
	movl	$1818324339, %ecx
	xorl	(%rax), %ecx
	movzbl	4(%rax), %edx
	xorl	$108, %edx
	orl	%ecx, %edx
	je	.LBB42_24
	movl	$1735549292, %ecx
	xorl	(%rax), %ecx
	movzbl	4(%rax), %eax
	xorl	$101, %eax
	orl	%ecx, %eax
	je	.LBB42_25
.LBB42_23:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.95(%rip), %rcx
	movl	$73, %edx
	jmp	.LBB42_27
.LBB42_24:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.97(%rip), %rcx
	jmp	.LBB42_26
.LBB42_25:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.96(%rip), %rcx
.LBB42_26:
	movl	$59, %edx
.LBB42_27:
	callq	_RNvNtNtCsaZOKLwlFm27_3std2io5stdio6__print
	movq	280(%rbp), %rax
	subq	288(%rbp), %rax
	movq	%rax, 256(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.99(%rip), %rax
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.98(%rip), %rcx
	testb	%r14b, %r14b
	movq	%rcx, %rdx
	cmovneq	%rax, %rdx
	movzbl	%r14b, %r8d
	movl	$6, %r9d
	movl	$6, %r10d
	subq	%r8, %r10
	movq	%rdx, -32(%rbp)
	testb	$1, %bl
	cmovneq	%rax, %rcx
	movq	%r10, -24(%rbp)
	movzbl	%bl, %eax
	subq	%rax, %r9
	movq	%rcx, 184(%rbp)
	movq	%r9, 192(%rbp)
	movl	%esi, -80(%rbp)
	leaq	312(%rbp), %rax
	movq	%rax, -8(%rbp)
	leaq	_RNvXs1i_NtCsd9eshbB3hfz_4core3fmtReNtB6_7Display3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack(%rip), %r14
	movq	%r14, (%rbp)
	leaq	288(%rbp), %rax
	movq	%rax, 8(%rbp)
	leaq	_RNvXsi_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impjNtB9_7Display3fmt(%rip), %rax
	movq	%rax, 16(%rbp)
	leaq	280(%rbp), %rcx
	movq	%rcx, 24(%rbp)
	movq	%rax, 32(%rbp)
	leaq	256(%rbp), %r13
	movq	%r13, 40(%rbp)
	leaq	_RNvXse_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impxNtB9_7Display3fmt(%rip), %rcx
	movq	%rcx, 48(%rbp)
	leaq	328(%rbp), %rcx
	movq	%rcx, 56(%rbp)
	movq	%rax, 64(%rbp)
	leaq	-40(%rbp), %rcx
	movq	%rcx, 72(%rbp)
	movq	%rax, 80(%rbp)
	leaq	296(%rbp), %rcx
	movq	%rcx, 88(%rbp)
	movq	%rax, 96(%rbp)
	leaq	-32(%rbp), %rax
	movq	%rax, 104(%rbp)
	movq	%r14, 112(%rbp)
	leaq	184(%rbp), %rax
	movq	%rax, 120(%rbp)
	movq	%r14, 128(%rbp)
	leaq	-80(%rbp), %rdi
	movq	%rdi, 136(%rbp)
	leaq	_RNvXs9_NtNtNtCsd9eshbB3hfz_4core3fmt3num3implNtB9_7Display3fmt(%rip), %rax
	movq	%rax, 144(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.100(%rip), %rcx
	leaq	-8(%rbp), %rdx
	callq	_RNvNtNtCsaZOKLwlFm27_3std2io5stdio6__print
	movl	$5, %ecx
	movl	$1, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB42_118
	movb	$99, 4(%rax)
	movl	$1953723762, (%rax)
	movq	$5, -8(%rbp)
	movq	%rax, (%rbp)
	movq	$5, 8(%rbp)
	movb	$0, 16(%rbp)
	movq	$0, 24(%rbp)
	movq	$8, 32(%rbp)
	movq	$0, 40(%rbp)
	movq	$0, 128(%rbp)
	movq	$0, 144(%rbp)
	movw	$0, 152(%rbp)
	movq	$-1, 48(%rbp)
	movq	$0, 164(%rbp)
	movw	$0, 160(%rbp)
	movl	$-1, 80(%rbp)
	movl	$-1, 96(%rbp)
	movl	$-1, 112(%rbp)
	movb	$2, 173(%rbp)
	movb	$1, 172(%rbp)
	movl	$2, %ecx
	movl	$1, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB42_119
	leaq	24(%rbp), %rcx
	movq	%rax, 304(%rbp)
	movw	$22061, (%rax)
.Ltmp212:
	callq	_RNvMs3_NtCsdkPGTeTFVes_5alloc7raw_vecINtB5_6RawVecNtNtNtNtCsaZOKLwlFm27_3std3sys4args7windows3ArgE8grow_oneBU_
	nop
.Ltmp213:
	movq	32(%rbp), %rax
	movq	$0, (%rax)
	movq	$2, 8(%rax)
	movq	304(%rbp), %rcx
	movq	%rcx, 16(%rax)
	movq	$2, 24(%rax)
	movb	$0, 32(%rax)
	movq	$1, 40(%rbp)
.Ltmp214:
	leaq	184(%rbp), %rbx
	leaq	-8(%rbp), %rdx
	movq	%rbx, %rcx
	callq	_RNvMsk_NtCsaZOKLwlFm27_3std7processNtB5_7Command6output
	nop
.Ltmp215:
	movq	184(%rbp), %rax
	movq	%rax, 304(%rbp)
	cmpq	$-1, %rax
	sete	%al
	movl	232(%rbp), %ecx
	movl	%ecx, 180(%rbp)
	testl	%ecx, %ecx
	setne	%cl
	orb	%al, %cl
	cmpb	$1, %cl
	jne	.LBB42_38
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.103(%rip), %rax
	movq	%rax, -80(%rbp)
	movq	$4, -72(%rbp)
	movq	%rdi, 256(%rbp)
	movq	%r14, 264(%rbp)
.Ltmp220:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.104(%rip), %rdx
	leaq	-32(%rbp), %rcx
	leaq	256(%rbp), %r8
	callq	_RNvNvNtCsdkPGTeTFVes_5alloc3fmt6format12format_inner
	nop
.Ltmp221:
	movups	-32(%rbp), %xmm0
	movaps	%xmm0, 256(%rbp)
	movq	-16(%rbp), %rax
	movq	%rax, 272(%rbp)
	movq	304(%rbp), %rdx
	cmpq	$-1, %rdx
	je	.LBB42_63
	testq	%rdx, %rdx
	je	.LBB42_36
	movq	192(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB42_36:
	movq	208(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB42_101
	movq	216(%rbp), %rcx
	movl	$1, %r8d
	jmp	.LBB42_100
.LBB42_38:
	movq	192(%rbp), %rsi
	movq	200(%rbp), %r8
	movq	208(%rbp), %rbx
	movq	216(%rbp), %rdi
.Ltmp216:
	leaq	-32(%rbp), %rcx
	movq	%rsi, %rdx
	callq	_RNvMNtCsdkPGTeTFVes_5alloc6stringNtB2_6String15from_utf8_lossy
	nop
.Ltmp217:
	movq	-32(%rbp), %rax
	movq	%rax, 240(%rbp)
	movq	-24(%rbp), %r8
	movq	-16(%rbp), %rcx
	leaq	(%r8,%rcx), %rax
	testq	%rcx, %rcx
	movq	%rdi, -56(%rbp)
	movq	%rsi, -48(%rbp)
	movq	%r8, 248(%rbp)
	je	.LBB42_64
	leaq	_RNvNtNtNtCsd9eshbB3hfz_4core7unicode12unicode_data11white_space14WHITESPACE_MAP(%rip), %rdx
	xorl	%r12d, %r12d
	movq	%r8, %rcx
	jmp	.LBB42_44
.LBB42_41:
	movzbl	%r8b, %r8d
	movzbl	(%r8,%rdx), %r8d
	shrb	%r8b
.LBB42_42:
	testb	$1, %r8b
	je	.LBB42_65
	.p2align	4
.LBB42_43:
	cmpq	%rax, %rcx
	je	.LBB42_62
.LBB42_44:
	movq	%rcx, %r9
	movq	%r12, %r15
	movzbl	(%rcx), %ecx
	movzbl	%cl, %r8d
	testb	%r8b, %r8b
	js	.LBB42_46
	leaq	1(%r9), %rcx
	jmp	.LBB42_51
	.p2align	4
.LBB42_46:
	movl	%r8d, %r10d
	andl	$31, %r10d
	movzbl	1(%r9), %r11d
	andl	$63, %r11d
	cmpb	$-33, %r8b
	jbe	.LBB42_49
	movzbl	2(%r9), %r8d
	shll	$6, %r11d
	andl	$63, %r8d
	orl	%r11d, %r8d
	cmpb	$-16, %cl
	jb	.LBB42_50
	leaq	4(%r9), %rcx
	movzbl	3(%r9), %r11d
	andl	$7, %r10d
	shll	$18, %r10d
	shll	$6, %r8d
	andl	$63, %r11d
	orl	%r8d, %r11d
	orl	%r10d, %r11d
	movl	%r11d, %r8d
	jmp	.LBB42_51
.LBB42_49:
	leaq	2(%r9), %rcx
	shll	$6, %r10d
	orl	%r11d, %r10d
	movl	%r10d, %r8d
	jmp	.LBB42_51
.LBB42_50:
	leaq	3(%r9), %rcx
	shll	$12, %r10d
	orl	%r10d, %r8d
	.p2align	4
.LBB42_51:
	movq	%rcx, %r12
	subq	%r9, %r12
	addq	%r15, %r12
	leal	-9(%r8), %r9d
	cmpl	$5, %r9d
	jb	.LBB42_43
	cmpl	$32, %r8d
	je	.LBB42_43
	cmpl	$133, %r8d
	jb	.LBB42_65
	movl	%r8d, %r9d
	shrl	$8, %r9d
	cmpl	$31, %r9d
	jg	.LBB42_58
	testl	%r9d, %r9d
	je	.LBB42_61
	cmpl	$22, %r9d
	jne	.LBB42_65
	cmpl	$5760, %r8d
	sete	%r8b
	jmp	.LBB42_42
.LBB42_58:
	cmpl	$32, %r9d
	je	.LBB42_41
	cmpl	$48, %r9d
	jne	.LBB42_65
	cmpl	$12288, %r8d
	sete	%r8b
	jmp	.LBB42_42
.LBB42_61:
	movzbl	%r8b, %r8d
	movzbl	(%r8,%rdx), %r8d
	jmp	.LBB42_42
.LBB42_62:
	movq	%r14, %rsi
	movq	%rbx, %r14
	movq	%r13, %rdi
	jmp	.LBB42_91
.LBB42_63:
	leaq	192(%rbp), %rcx
.Ltmp222:
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp223:
	jmp	.LBB42_101
.LBB42_64:
	xorl	%r12d, %r12d
	movq	%r8, %rcx
	xorl	%r15d, %r15d
.LBB42_65:
	cmpq	%rax, %rcx
	jne	.LBB42_70
.LBB42_89:
	movq	%r14, %rsi
	subq	%r15, %r12
	js	.LBB42_107
.LBB42_90:
	movq	%rbx, %r14
	movq	%r13, %rdi
	jne	.LBB42_92
.LBB42_91:
	movl	$1, %r13d
	xorl	%r12d, %r12d
	jmp	.LBB42_94
.LBB42_67:
	movzbl	%r9b, %r9d
	leaq	_RNvNtNtNtCsd9eshbB3hfz_4core7unicode12unicode_data11white_space14WHITESPACE_MAP(%rip), %rdx
	movzbl	(%r9,%rdx), %r9d
	shrb	%r9b
.LBB42_68:
	testb	$1, %r9b
	je	.LBB42_106
	.p2align	4
.LBB42_69:
	cmpq	%rax, %rcx
	je	.LBB42_89
.LBB42_70:
	movq	%rax, %r8
	movsbl	-1(%rax), %r9d
	testl	%r9d, %r9d
	js	.LBB42_72
	leaq	-1(%r8), %rax
	leal	-9(%r9), %r10d
	cmpl	$5, %r10d
	jb	.LBB42_69
	jmp	.LBB42_79
	.p2align	4
.LBB42_72:
	movzbl	-2(%r8), %r10d
	cmpb	$-64, %r10b
	jge	.LBB42_75
	movzbl	-3(%r8), %r11d
	cmpb	$-64, %r11b
	jge	.LBB42_76
	leaq	-4(%r8), %rax
	movq	%rbx, %rdx
	movzbl	-4(%r8), %ebx
	andl	$7, %ebx
	shll	$6, %ebx
	andl	$63, %r11d
	orl	%ebx, %r11d
	movq	%rdx, %rbx
	jmp	.LBB42_77
.LBB42_75:
	leaq	-2(%r8), %rax
	andl	$31, %r10d
	jmp	.LBB42_78
.LBB42_76:
	leaq	-3(%r8), %rax
	andl	$15, %r11d
.LBB42_77:
	shll	$6, %r11d
	andl	$63, %r10d
	orl	%r11d, %r10d
.LBB42_78:
	shll	$6, %r10d
	andb	$63, %r9b
	movzbl	%r9b, %r9d
	orl	%r10d, %r9d
	leal	-9(%r9), %r10d
	cmpl	$5, %r10d
	jb	.LBB42_69
.LBB42_79:
	cmpl	$32, %r9d
	je	.LBB42_69
	cmpl	$133, %r9d
	jb	.LBB42_106
	movl	%r9d, %r10d
	shrl	$8, %r10d
	cmpl	$31, %r10d
	jg	.LBB42_85
	testl	%r10d, %r10d
	je	.LBB42_88
	cmpl	$22, %r10d
	jne	.LBB42_106
	cmpl	$5760, %r9d
	sete	%r9b
	jmp	.LBB42_68
.LBB42_85:
	cmpl	$32, %r10d
	je	.LBB42_67
	cmpl	$48, %r10d
	jne	.LBB42_106
	cmpl	$12288, %r9d
	sete	%r9b
	jmp	.LBB42_68
.LBB42_88:
	movzbl	%r9b, %r9d
	leaq	_RNvNtNtNtCsd9eshbB3hfz_4core7unicode12unicode_data11white_space14WHITESPACE_MAP(%rip), %rdx
	movzbl	(%r9,%rdx), %r9d
	jmp	.LBB42_68
.LBB42_92:
	movl	$1, %ebx
	movl	$1, %edx
	movq	%r12, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB42_108
	movq	%rax, %r13
	addq	248(%rbp), %r15
	movq	%rax, %rcx
	movq	%r15, %rdx
	movq	%r12, %r8
	callq	memcpy
.LBB42_94:
	movq	%r12, 256(%rbp)
	movq	%r13, 264(%rbp)
	movq	%r12, 272(%rbp)
	movq	240(%rbp), %rdx
	testq	%rdx, %rdx
	leaq	184(%rbp), %rbx
	movq	304(%rbp), %r15
	jle	.LBB42_96
	movl	$1, %r8d
	movq	248(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB42_96:
	testq	%r15, %r15
	movq	%rdi, %r13
	je	.LBB42_98
	movl	$1, %r8d
	movq	-48(%rbp), %rcx
	movq	%r15, %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB42_98:
	movq	%r14, %rdx
	testq	%r14, %r14
	movq	%rsi, %r14
	je	.LBB42_101
	movl	$1, %r8d
	movq	-56(%rbp), %rcx
.LBB42_100:
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB42_101:
.Ltmp224:
	leaq	-8(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
.Ltmp225:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.85(%rip), %rax
	movq	%rax, 184(%rbp)
	movq	$7, 192(%rbp)
	movq	%r13, -8(%rbp)
	leaq	_RNvXsq_NtCsdkPGTeTFVes_5alloc6stringNtB5_6StringNtNtCsd9eshbB3hfz_4core3fmt7Display3fmt(%rip), %rax
	movq	%rax, (%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.84(%rip), %rax
	movq	%rax, 8(%rbp)
	movq	%r14, 16(%rbp)
	movq	%rbx, 24(%rbp)
	movq	%r14, 32(%rbp)
.Ltmp226:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.105(%rip), %rcx
	leaq	-8(%rbp), %rdx
	callq	_RNvNtNtCsaZOKLwlFm27_3std2io5stdio6__print
	nop
.Ltmp227:
	movq	256(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB42_105
	movq	264(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB42_105:
	.seh_startepilogue
	addq	$472, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB42_106:
	movq	%r14, %rsi
	subq	%rcx, %r12
	addq	%r8, %r12
	subq	%r15, %r12
	jns	.LBB42_90
.LBB42_107:
	xorl	%ebx, %ebx
.LBB42_108:
.Ltmp218:
	movq	%rbx, %rcx
	movq	%r12, %rdx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp219:
	jmp	.LBB42_120
.LBB42_109:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.92(%rip), %rcx
	jmp	.LBB42_111
.LBB42_110:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.91(%rip), %rcx
.LBB42_111:
	movl	$65, %edx
.LBB42_112:
	callq	_RNvNtNtCsaZOKLwlFm27_3std2io5stdio6__print
	movl	$1, %esi
	jmp	.LBB42_17
.LBB42_113:
	xorl	%ecx, %ecx
	testb	%cl, %cl
	jne	.LBB42_20
.LBB42_114:
	movq	%rax, 184(%rbp)
	leaq	-64(%rbp), %rax
	movq	%rax, -8(%rbp)
	leaq	_RNvXsi_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impjNtB9_7Display3fmt(%rip), %rax
	movq	%rax, (%rbp)
	leaq	328(%rbp), %rcx
	movq	%rcx, 8(%rbp)
	movq	%rax, 16(%rbp)
	leaq	184(%rbp), %rcx
	movq	%rcx, 24(%rbp)
	movq	%rax, 32(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.93(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.94(%rip), %r8
	leaq	-8(%rbp), %rdx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	ud2
.LBB42_115:
	callq	_RINvMNtNtCsaZOKLwlFm27_3std4sync9once_lockINtB3_8OnceLockTjjjbEE10initializeNCINvB2_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zEB1B_
	jmp	.LBB42_1
.LBB42_116:
	leaq	328(%rbp), %rax
	movq	%rax, 184(%rbp)
	leaq	_RNvXsi_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impjNtB9_7Display3fmt(%rip), %rax
	movq	%rax, 192(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.106(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.107(%rip), %r8
	leaq	184(%rbp), %rdx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	ud2
.LBB42_117:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.85(%rip), %rax
	movq	%rax, 184(%rbp)
	movq	$7, 192(%rbp)
	leaq	288(%rbp), %rax
	movq	%rax, -8(%rbp)
	leaq	_RNvXsi_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impjNtB9_7Display3fmt(%rip), %rax
	movq	%rax, (%rbp)
	leaq	280(%rbp), %rcx
	movq	%rcx, 8(%rbp)
	movq	%rax, 16(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.84(%rip), %rax
	movq	%rax, 24(%rbp)
	leaq	_RNvXs1i_NtCsd9eshbB3hfz_4core3fmtReNtB6_7Display3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack(%rip), %rax
	movq	%rax, 32(%rbp)
	leaq	184(%rbp), %rcx
	movq	%rcx, 40(%rbp)
	movq	%rax, 48(%rbp)
	leaq	312(%rbp), %rcx
	movq	%rcx, 56(%rbp)
	movq	%rax, 64(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.86(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.87(%rip), %r8
	leaq	-8(%rbp), %rdx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	ud2
.LBB42_118:
	movl	$1, %ecx
	movl	$5, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	ud2
.LBB42_119:
.Ltmp228:
	movl	$1, %ecx
	movl	$2, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp229:
.LBB42_120:
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@IMGREL
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43
	.seh_endproc
	.def	"?dtor$121@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$121@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA":
.seh_proc "?dtor$121@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"
.LBB42_121:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	-8(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43
	.seh_endproc
	.def	"?dtor$122@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$122@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA":
.seh_proc "?dtor$122@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"
.LBB42_122:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$0, 240(%rbp)
	jle	.LBB42_124
	movl	$1, %r8d
	movq	248(%rbp), %rcx
	movq	240(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB42_124:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43
	.seh_endproc
	.def	"?dtor$125@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$125@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA":
.seh_proc "?dtor$125@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"
.LBB42_125:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	184(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process6OutputECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43
	.seh_endproc
	.def	"?dtor$126@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$126@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA":
.seh_proc "?dtor$126@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"
.LBB42_126:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$2, %edx
	movl	$1, %r8d
	movq	304(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43
	.seh_endproc
	.def	"?dtor$127@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$127@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA":
.seh_proc "?dtor$127@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"
.LBB42_127:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$-1, 304(%rbp)
	jne	.LBB42_129
	leaq	184(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtB4_6result6ResultNtNtCsaZOKLwlFm27_3std7process6OutputNtNtNtB11_2io5error5ErrorEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB42_129:
	cmpl	$0, 180(%rbp)
	je	.LBB42_131
	leaq	184(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process6OutputECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB42_131:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43
	.seh_endproc
	.def	"?dtor$132@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$132@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA":
.seh_proc "?dtor$132@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"
.LBB42_132:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	256(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB42_134
	movq	264(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB42_134:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43
	.seh_endproc
	.def	"?dtor$135@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$135@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA":
.seh_proc "?dtor$135@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"
.LBB42_135:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	-8(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std7process7CommandECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end42:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,20
	.p2align	2, 0x0
$cppxdata$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure:
	.long	429065506
	.long	7
	.long	$stateUnwindMap$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@IMGREL
	.long	0
	.long	0
	.long	14
	.long	$ip2state$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@IMGREL
	.long	464
	.long	0
	.long	1
$stateUnwindMap$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure:
	.long	-1
	.long	"?dtor$135@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"@IMGREL
	.long	0
	.long	"?dtor$127@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"@IMGREL
	.long	1
	.long	"?dtor$125@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"@IMGREL
	.long	2
	.long	"?dtor$122@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"@IMGREL
	.long	0
	.long	"?dtor$126@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"@IMGREL
	.long	-1
	.long	"?dtor$132@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"@IMGREL
	.long	5
	.long	"?dtor$121@?0?_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure@4HA"@IMGREL
$ip2state$_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure:
	.long	.Lfunc_begin42@IMGREL
	.long	-1
	.long	.Ltmp212@IMGREL
	.long	4
	.long	.Ltmp214@IMGREL
	.long	0
	.long	.Ltmp220@IMGREL
	.long	1
	.long	.Ltmp221@IMGREL
	.long	-1
	.long	.Ltmp216@IMGREL
	.long	2
	.long	.Ltmp222@IMGREL
	.long	6
	.long	.Ltmp223@IMGREL
	.long	-1
	.long	.Ltmp224@IMGREL
	.long	5
	.long	.Ltmp227@IMGREL
	.long	-1
	.long	.Ltmp218@IMGREL
	.long	3
	.long	.Ltmp219@IMGREL
	.long	-1
	.long	.Ltmp228@IMGREL
	.long	0
	.long	.Ltmp229@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure,unique,43

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

	.def	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc;
	.scl	2;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc,unique,46
	.globl	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	.p2align	4
_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc:
.Lfunc_begin45:
.seh_proc _RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	subq	$136, %rsp
	.seh_stackalloc 136
	.seh_endprologue
	movq	%rcx, %rsi
	movl	_tls_index(%rip), %eax
	movq	%gs:88, %rcx
	movq	(%rcx,%rax,8), %rax
	movq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rax), %rcx
	leaq	-1(%rcx), %rax
	cmpq	$-2, %rax
	jae	.LBB45_7
.LBB45_1:
	cmpq	$17, %rsi
	movl	$16, %eax
	cmovaeq	%rsi, %rax
	cmpq	$17, %rdx
	jae	.LBB45_9
	decq	%rax
	cmpq	$258751, %rax
	ja	.LBB45_17
	shrq	$4, %rax
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes10SIZE2CLASS(%rip), %r8
	movzbl	(%rax,%r8), %eax
	cmpq	$49, %rax
	jae	.LBB45_28
.LBB45_4:
	movl	%eax, %r8d
	shll	$7, %r8d
	leaq	(%r8,%rax,8), %r9
	movzbl	1192(%rcx,%r9), %r8d
	testq	%r8, %r8
	je	.LBB45_23
	leaq	(%rcx,%r9), %rax
	addq	$1192, %rax
	decq	%r8
	movb	%r8b, (%rax)
	andl	$15, %r8d
	movq	8(%rax,%r8,8), %rax
	movq	%rax, %rcx
	andq	$-4194304, %rcx
	movl	%eax, %edx
	shrl	$4, %edx
	andl	$262143, %edx
	andb	$63, 5512(%rcx,%rdx)
.LBB45_6:
	.seh_startepilogue
	addq	$136, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.LBB45_7:
	testq	%rcx, %rcx
	je	.LBB45_22
.LBB45_8:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665(%rip), %rcx
	movq	%rsi, %r8
	.seh_startepilogue
	addq	$136, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc18alloc_via_fallback
.LBB45_9:
	cmpq	%rdx, %rax
	cmovbeq	%rdx, %rax
	decq	%rax
	cmpq	$258751, %rax
	ja	.LBB45_17
	shrq	$4, %rax
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes2SC(%rip), %r8
	movzbl	404(%rax,%r8), %r10d
	cmpb	$48, %r10b
	ja	.LBB45_17
	leaq	-1(%rdx), %r9
	.p2align	4
.LBB45_12:
	movzbl	%r10b, %eax
	movq	(%r8,%rax,8), %r10
	testq	%r9, %r10
	je	.LBB45_4
	orq	%r9, %r10
	cmpq	$258751, %r10
	ja	.LBB45_18
	shrq	$4, %r10
	movzbl	404(%r10,%r8), %r10d
	cmpb	$49, %r10b
	jb	.LBB45_12
	jmp	.LBB45_18
.LBB45_17:
.LBB45_18:
	movq	%rax, 32(%rsp)
	movq	%rdx, %rdi
	movq	%rsi, %r8
	xorl	%r9d, %r9d
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core5alloc3hotNtNtB6_4core8HeapCore28alloc_large_without_magazine
	testq	%rax, %rax
	jne	.LBB45_6
.LBB45_19:
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	je	.LBB45_24
	cmpq	$1, %rax
	je	.LBB45_21
.LBB45_25:
	callq	*%rax
	testb	%al, %al
	jne	.LBB45_27
.LBB45_21:
	xorl	%eax, %eax
	.seh_startepilogue
	addq	$136, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.LBB45_22:
	xorps	%xmm0, %xmm0
	movaps	%xmm0, 112(%rsp)
	movaps	%xmm0, 96(%rsp)
	movaps	%xmm0, 80(%rsp)
	movaps	%xmm0, 64(%rsp)
	movaps	%xmm0, 48(%rsp)
	movq	$0, 128(%rsp)
	leaq	48(%rsp), %rcx
	movq	%rdx, %rdi
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap28bind_slow_tagged_with_config
	movq	%rdx, %rcx
	movq	%rdi, %rdx
	cmpq	$1, %rax
	jne	.LBB45_1
	jmp	.LBB45_8
.LBB45_23:
	movq	%rdx, %rdi
	movq	%rax, %rdx
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core5alloc3hotNtNtB6_4core8HeapCore20refill_magazine_slow
	testq	%rax, %rax
	jne	.LBB45_6
	jmp	.LBB45_19
.LBB45_24:
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit7resolve
	cmpq	$1, %rax
	je	.LBB45_21
	jmp	.LBB45_25
.LBB45_27:
	movq	%rdi, %rcx
	movq	%rsi, %rdx
	xorl	%r8d, %r8d
	.seh_startepilogue
	addq	$136, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc17exit_escape_alloc
.LBB45_28:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665(%rip), %r8
	movl	$49, %edx
	movq	%rax, %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	ud2
.Lfunc_end45:
	.seh_endproc

	.def	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc;
	.scl	2;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc,unique,47
	.globl	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	.p2align	4
_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc:
.Lfunc_begin46:
.seh_proc _RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
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
	subq	$56, %rsp
	.seh_stackalloc 56
	.seh_endprologue
	testq	%rcx, %rcx
	je	.LBB46_92
	movq	%rdx, %r9
	movl	_tls_index(%rip), %eax
	movq	%gs:88, %rdx
	movq	(%rdx,%rax,8), %rax
	movq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rax), %rsi
	leaq	-1(%rsi), %rax
	cmpq	$-2, %rax
	jae	.LBB46_9
	movq	%rcx, %rax
	andq	$-4194304, %rax
	cmpq	%rax, 304(%rsi)
	jne	.LBB46_4
	movq	312(%rsi), %rdi
	jmp	.LBB46_15
.LBB46_4:
	movq	%rax, %r10
	shrq	$22, %r10
	movl	%r10d, %r11d
	andl	$15, %r11d
	shll	$4, %r11d
	leaq	(%rsi,%r11), %rdx
	addq	$48, %rdx
	cmpq	%rax, 48(%rsi,%r11)
	jne	.LBB46_5
	movq	8(%rdx), %rdi
	jmp	.LBB46_15
.LBB46_5:
	movq	%rax, %r11
	shrq	$35, %r11
	movq	%rax, %rdi
	shrq	$48, %rdi
	xorl	%edi, %r11d
	xorl	%r11d, %r10d
	andl	$8191, %r10d
	movq	320(%rsi), %r11
	movq	%r10, %rbx
	.p2align	4
.LBB46_6:
	movq	(%r11,%rbx,8), %rdi
	testq	%rdi, %rdi
	je	.LBB46_9
	movq	%rdi, %r14
	andq	$-4194304, %r14
	cmpq	%rax, %r14
	je	.LBB46_10
	incl	%ebx
	andl	$8191, %ebx
	cmpq	%r10, %rbx
	jne	.LBB46_6
	jmp	.LBB46_9
.LBB46_10:
	movq	%rdi, %r10
	andq	$4194303, %r10
	je	.LBB46_13
	leaq	-1(%r10), %r11
	movl	344(%rsi), %edi
	cmpq	%rdi, %r11
	jae	.LBB46_9
	movq	32(%rsi), %r11
	movq	-8(%r11,%r10,8), %rdi
	testq	%rdi, %rdi
	je	.LBB46_9
.LBB46_13:
	movq	%rax, (%rdx)
	movq	%rdi, 8(%rdx)
	cmpq	%rax, %rdi
	jne	.LBB46_15
	movq	%rax, 304(%rsi)
	movq	%rdi, 312(%rsi)
.LBB46_15:
	movq	%rcx, %r15
	subq	%rdi, %r15
	movzbl	28(%rdi), %eax
	movl	$4194304, %edx
	cmpl	$2, %eax
	jne	.LBB46_17
	movq	56(%rdi), %rdx
.LBB46_17:
	cmpq	%rdx, %r15
	jae	.LBB46_9
	leaq	(%rdi,%r15), %rbx
	cmpq	$17, %r9
	movl	$16, %edx
	cmovaeq	%r9, %rdx
	cmpq	$17, %r8
	jae	.LBB46_19
	leaq	-1(%rdx), %r10
	cmpq	$258751, %r10
	ja	.LBB46_25
	shrq	$4, %r10
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes10SIZE2CLASS(%rip), %rdx
	movzbl	(%r10,%rdx), %r14d
	cmpq	$49, %r14
	jae	.LBB46_84
.LBB46_30:
	movl	%r14d, %edx
	shll	$7, %edx
	leaq	(%rdx,%r14,8), %r10
	movzbl	1192(%rsi,%r10), %edx
	testb	%al, %al
	je	.LBB46_31
	movzbl	%al, %eax
	cmpl	$2, %eax
	je	.LBB46_85
	cmpl	$1, %eax
	jne	.LBB46_86
	movl	$270336, %ecx
	movl	%r15d, %eax
	cmpq	%rcx, %rax
	jb	.LBB46_86
	jmp	.LBB46_74
.LBB46_19:
	cmpq	%r8, %rdx
	movq	%r8, %r11
	cmovaq	%rdx, %r11
	decq	%r11
	cmpq	$258751, %r11
	ja	.LBB46_25
	shrq	$4, %r11
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes2SC(%rip), %r10
	movzbl	404(%r11,%r10), %ebp
	cmpb	$48, %bpl
	ja	.LBB46_25
	leaq	-1(%r8), %r11
	.p2align	4
.LBB46_22:
	movzbl	%bpl, %r14d
	movq	(%r10,%r14,8), %r12
	testq	%r11, %r12
	je	.LBB46_30
	orq	%r11, %r12
	cmpq	$258751, %r12
	ja	.LBB46_25
	shrq	$4, %r12
	movzbl	404(%r12,%r10), %ebp
	cmpb	$49, %bpl
	jb	.LBB46_22
.LBB46_25:
	cmpl	$2, %eax
	jae	.LBB46_26
	cmpq	$17, %r8
	jae	.LBB46_33
	decq	%rdx
	cmpq	$258751, %rdx
	ja	.LBB46_34
	shrq	$4, %rdx
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes10SIZE2CLASS(%rip), %rcx
	movzbl	(%rdx,%rcx), %edx
	movb	$1, %cl
	jmp	.LBB46_42
.LBB46_26:
	jne	.LBB46_92
	movq	%rsi, %rcx
	movq	%rbx, %rdx
	movq	%rdi, %r8
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rbp
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	.seh_endepilogue
	jmp	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core10alloc_core3mem8mem_implNtNtB6_15alloc_core_impl9AllocCore21dealloc_large_at_base
.LBB46_33:
	cmpq	%r8, %rdx
	cmovbeq	%r8, %rdx
	decq	%rdx
	cmpq	$258751, %rdx
	ja	.LBB46_34
	shrq	$4, %rdx
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes2SC(%rip), %r9
	movzbl	404(%rdx,%r9), %r10d
	cmpb	$48, %r10b
	ja	.LBB46_34
	decq	%r8
	.p2align	4
.LBB46_37:
	movzbl	%r10b, %edx
	movq	(%r9,%rdx,8), %r10
	testq	%r8, %r10
	sete	%cl
	je	.LBB46_42
	orq	%r8, %r10
	cmpq	$258751, %r10
	ja	.LBB46_42
	shrq	$4, %r10
	movzbl	404(%r10,%r9), %r10d
	cmpb	$49, %r10b
	jb	.LBB46_37
	jmp	.LBB46_42
.LBB46_34:
	xorl	%ecx, %ecx
.LBB46_42:
	cmpl	$1, %eax
	je	.LBB46_91
	testl	%eax, %eax
	jne	.LBB46_92
	testb	%cl, %cl
	je	.LBB46_92
	movl	%r15d, %eax
	cmpq	$389120, %rax
	jb	.LBB46_92
	cmpq	(%rdi), %rax
	jae	.LBB46_92
	testb	$15, %r15b
	jne	.LBB46_92
	movl	%r15d, %ecx
	shrl	$4, %ecx
	leal	1(%rdx), %r8d
	cmpb	%r8b, 5512(%rdi,%rcx)
	jne	.LBB46_92
	cmpq	$48, %rdx
	jbe	.LBB46_50
	movq	$0, (%rdi,%rax)
	orb	$-128, %r8b
	movb	%r8b, 5512(%rdi,%rcx)
	jmp	.LBB46_54
.LBB46_91:
	testb	%cl, %cl
	je	.LBB46_92
	movl	%r15d, %eax
	cmpq	$270336, %rax
	jb	.LBB46_92
	cmpq	(%rdi), %rax
	jae	.LBB46_92
	testb	$15, %r15b
	jne	.LBB46_92
	movl	%r15d, %ecx
	shrl	$4, %ecx
	movzbl	5512(%rdi,%rcx), %r8d
	leal	1(%rdx), %r9d
	cmpb	%r9b, %r8b
	jne	.LBB46_92
	cmpq	$48, %rdx
	jbe	.LBB46_67
	movq	$0, (%rdi,%rax)
	orb	$-128, %r8b
	movb	%r8b, 5512(%rdi,%rcx)
.LBB46_54:
	movl	20(%rdi), %eax
	testl	%eax, %eax
	je	.LBB46_69
	decl	%eax
	movl	%eax, 20(%rdi)
	jmp	.LBB46_56
.LBB46_9:
	movq	%r8, %rdx
	movq	%r9, %r8
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rbp
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	.seh_endepilogue
	jmp	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry17heap_core_xthread7routingNtNtNtB6_9heap_core4core8HeapCore15publish_foreign
.LBB46_85:
	movq	48(%rdi), %rax
	addq	%rdi, %rax
	movq	8(%rdi), %rdx
	setb	%dl
	cmpq	%rcx, %rax
	setne	%al
	orb	%dl, %al
	jne	.LBB46_86
	movq	%rdi, 32(%rsp)
	movq	%rsi, %rcx
	movq	%rbx, %rdx
	callq	_RNvNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core4free16dealloc_own_base19route_to_large_free
	jmp	.LBB46_92
.LBB46_31:
	movl	$389120, %ecx
	movl	%r15d, %eax
	cmpq	%rcx, %rax
	jae	.LBB46_74
.LBB46_86:
	lock		incq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core10alloc_core8counters18LOCAL_REJECT_FREES(%rip)
.LBB46_92:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rbp
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	.seh_endepilogue
	retq
.LBB46_74:
	testb	$15, %r15b
	jne	.LBB46_92
	shrl	$4, %r15d
	movzbl	5512(%rdi,%r15), %ebp
	leal	1(%r14), %ecx
	cmpb	%cl, %bpl
	jne	.LBB46_81
	cmpq	(%rdi), %rax
	jae	.LBB46_86
	leaq	(%rsi,%r10), %r12
	addq	$1192, %r12
	movzbl	%dl, %r13d
	leaq	_RNvNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core5state6tcache13FREE_PARK_CAP(%rip), %rax
	cmpb	(%r14,%rax), %r13b
	jae	.LBB46_78
	orb	$64, %bpl
	movb	%bpl, 5512(%rdi,%r15)
	andl	$15, %r13d
	movq	%rbx, 8(%r12,%r13,8)
	incb	%dl
	movb	%dl, (%r12)
	jmp	.LBB46_92
.LBB46_81:
	testb	%bpl, %bpl
	je	.LBB46_83
	cmpb	$64, %bpl
	jae	.LBB46_86
	jmp	.LBB46_92
.LBB46_83:
	cmpq	(%rdi), %rax
	jae	.LBB46_86
	jmp	.LBB46_92
.LBB46_78:
	leal	-26(%r14), %eax
	cmpl	$23, %eax
	jae	.LBB46_89
	leaq	-1(%r13), %rcx
	cmpq	$16, %rcx
	jae	.LBB46_90
	movq	(%r12,%r13,8), %rax
	movq	%rax, 48(%rsp)
	leaq	48(%rsp), %r8
	movl	$1, %r9d
	movq	%rsi, %rcx
	movq	%r14, %rdx
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small25alloc_core_small_magazineNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore20flush_class_internal
	movq	%rbx, (%r12,%r13,8)
	orb	$64, %bpl
	movb	%bpl, 5512(%rdi,%r15)
	jmp	.LBB46_92
.LBB46_50:
	movl	5120(%rdi,%rdx,4), %r9d
	leaq	(%rdi,%r9), %r10
	xorl	%r11d, %r11d
	movl	$4294967295, %ebx
	cmpq	%rbx, %r9
	cmovneq	%r10, %r11
	movq	%r11, (%rdi,%rax)
	movl	%r15d, 5120(%rdi,%rdx,4)
	orb	$-128, %r8b
	movb	%r8b, 5512(%rdi,%rcx)
	jmp	.LBB46_51
.LBB46_67:
	movl	5120(%rdi,%rdx,4), %r9d
	leaq	(%rdi,%r9), %r10
	xorl	%r11d, %r11d
	movl	$4294967295, %ebx
	cmpq	%rbx, %r9
	cmovneq	%r10, %r11
	movq	%r11, (%rdi,%rax)
	movl	%r15d, 5120(%rdi,%rdx,4)
	orb	$-128, %r8b
	movb	%r8b, 5512(%rdi,%rcx)
.LBB46_51:
	movl	20(%rdi), %eax
	testl	%eax, %eax
	je	.LBB46_69
	decl	%eax
	movl	%eax, 20(%rdi)
	cmpl	$-1, %r9d
	jne	.LBB46_57
.LBB46_56:
	movl	112(%rdi), %r8d
	movq	1000(%rsi), %rcx
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small16alloc_core_small9directoryNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore16publish_nonempty
	movl	20(%rdi), %eax
.LBB46_57:
	testl	%eax, %eax
	jne	.LBB46_92
	cmpq	936(%rsi), %rdi
	je	.LBB46_92
	cmpl	$0, 24(%rdi)
	jne	.LBB46_92
	cmpb	$1, 28(%rdi)
	jne	.LBB46_92
	movq	%rsi, %rcx
	movq	%rdi, %rdx
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rbp
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	.seh_endepilogue
	jmp	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small21alloc_core_small_pool26alloc_core_small_pool_implNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore29release_or_pool_empty_segment
.LBB46_89:
	leaq	8(%r12), %r8
	movl	$8, %r9d
	movq	%rsi, %rcx
	movq	%r14, %rdx
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small25alloc_core_small_magazineNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore20flush_class_internal
	movl	%r14d, %eax
	shll	$7, %eax
	leaq	(%rax,%r14,8), %rax
	movups	1264(%rsi,%rax), %xmm0
	movups	1280(%rsi,%rax), %xmm1
	movups	1296(%rsi,%rax), %xmm2
	movups	1312(%rsi,%rax), %xmm3
	movups	%xmm3, 56(%r12)
	movups	%xmm2, 40(%r12)
	movups	%xmm1, 24(%r12)
	movups	%xmm0, 8(%r12)
	orb	$64, %bpl
	movb	%bpl, 5512(%rdi,%r15)
	movq	%rbx, 72(%r12)
	movb	$9, (%r12)
	jmp	.LBB46_92
.LBB46_69:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.150(%rip), %rcx
	movl	$58, %edx
	callq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform5fatal5fatal
	ud2
.LBB46_84:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.155(%rip), %r8
	movl	$49, %edx
	movq	%r14, %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	ud2
.LBB46_90:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.156(%rip), %r8
	movl	$16, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	ud2
.Lfunc_end46:
	.seh_endproc

	.def	_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc;
	.scl	2;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc,unique,48
	.globl	_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc
	.p2align	4
_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc:
.Lfunc_begin47:
.seh_proc _RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$168, %rsp
	.seh_stackalloc 168
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 32(%rbp)
	testq	%rcx, %rcx
	je	.LBB47_46
	movq	%r9, %rsi
	movq	%r8, %rbx
	movq	%rdx, %rdi
	movq	%rcx, %r14
	movl	_tls_index(%rip), %eax
	movq	%gs:88, %rcx
	movq	(%rcx,%rax,8), %rax
	movq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rax), %rcx
	leaq	-1(%rcx), %rax
	cmpq	$-2, %rax
	jae	.LBB47_2
.LBB47_5:
	movq	%rsi, 32(%rsp)
	movq	%r14, %rdx
	movq	%rbx, %r8
	movq	%rdi, %r9
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core4free7reallocNtNtB6_4core8HeapCore7realloc
	testq	%rax, %rax
	jne	.LBB47_47
	jmp	.LBB47_42
.LBB47_2:
	testq	%rcx, %rcx
	je	.LBB47_3
.LBB47_6:
	xorps	%xmm0, %xmm0
	movaps	%xmm0, -16(%rbp)
	movaps	%xmm0, -32(%rbp)
	movaps	%xmm0, -48(%rbp)
	movaps	%xmm0, -64(%rbp)
	movaps	%xmm0, -80(%rbp)
	movq	$0, (%rbp)
	leaq	-80(%rbp), %rcx
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback13heap_ptr_impl.llvm.2560762130558453100
	testq	%rax, %rax
	je	.LBB47_42
	movq	%rax, -88(%rbp)
	movq	%rbx, 24(%rbp)
	movq	%rdi, 16(%rbp)
	movq	%rsi, 8(%rbp)
	movb	$1, %r12b
	xorl	%eax, %eax
	lock		cmpxchgb	%r12b, _RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip)
	jne	.LBB47_8
.LBB47_21:
	lock		incq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback17LOCK_ACQUISITIONS.llvm.2560762130558453100(%rip)
	movzbl	-80(%rbp), %eax
	movq	-88(%rbp), %rcx
	cmpl	$1, 800(%rcx)
	jne	.LBB47_36
	testb	$1, %al
	movq	8(%rbp), %rsi
	movq	16(%rbp), %rdi
	movq	24(%rbp), %rbx
	je	.LBB47_37
	movq	-72(%rbp), %rax
	cmpq	%rax, 808(%rcx)
	je	.LBB47_24
	jmp	.LBB47_37
.LBB47_8:
	movl	$-1, %r13d
	movq	__imp_SwitchToThread(%rip), %rsi
	xorl	%edi, %edi
	xorl	%ebx, %ebx
	xorl	%r15d, %r15d
	jmp	.LBB47_10
	.p2align	4
.LBB47_12:
	incl	%ebx
	testb	$63, %bl
	je	.LBB47_13
.LBB47_17:
	callq	*%rsi
.LBB47_10:
	movzbl	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip), %eax
	testb	%al, %al
	je	.LBB47_9
	cmpl	$64, %edi
	jae	.LBB47_12
	cmpl	$6, %r15d
	movl	$6, %ecx
	cmovbl	%r15d, %ecx
	movl	$1, %eax
	.p2align	4
.LBB47_19:
	pause
	movl	%eax, %edx
	shrl	%cl, %edx
	incl	%eax
	testl	%edx, %edx
	je	.LBB47_19
	incl	%edi
	incl	%r15d
	cmovel	%r13d, %r15d
	jmp	.LBB47_10
.LBB47_13:
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	je	.LBB47_14
	cmpq	$1, %rax
	je	.LBB47_17
.LBB47_16:
	callq	*%rax
	testb	%al, %al
	je	.LBB47_17
	jmp	.LBB47_41
.LBB47_9:
	xorl	%eax, %eax
	lock		cmpxchgb	%r12b, _RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip)
	jne	.LBB47_10
	jmp	.LBB47_21
.LBB47_14:
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit7resolve
	cmpq	$1, %rax
	je	.LBB47_17
	jmp	.LBB47_16
.LBB47_36:
	testb	$1, %al
	movq	8(%rbp), %rsi
	movq	16(%rbp), %rdi
	movq	24(%rbp), %rbx
	jne	.LBB47_37
.LBB47_24:
	cmpb	$0, -8(%rbp)
	movl	$10, %eax
	je	.LBB47_26
	movl	-4(%rbp), %eax
.LBB47_26:
	cmpl	$100, %eax
	movl	$100, %r8d
	movl	$100, %edx
	cmovbl	%eax, %edx
	imull	$100, %edx, %edx
	testl	%eax, %eax
	cmovel	%r8d, %edx
	testb	$1, -64(%rbp)
	movl	$268435456, %eax
	je	.LBB47_28
	movq	-56(%rbp), %rax
.LBB47_28:
	testb	$1, -16(%rbp)
	movl	$1000, %r9d
	cmovnel	-12(%rbp), %r9d
	cmpl	%edx, 840(%rcx)
	jne	.LBB47_37
	imulq	$274877907, %r9, %rdx
	shrq	$38, %rdx
	movl	%edx, %r8d
	cmpq	%r8, 816(%rcx)
	jne	.LBB47_37
	imull	$1000, %edx, %edx
	subl	%edx, %r9d
	imull	$1000000, %r9d, %edx
	cmpl	%edx, 824(%rcx)
	jne	.LBB47_37
	cmpq	%rax, 832(%rcx)
	jne	.LBB47_37
	movq	-24(%rbp), %r8
	testb	$1, -48(%rbp)
	movl	$4, %eax
	movl	$4, %edx
	je	.LBB47_34
	movq	-40(%rbp), %rdx
.LBB47_34:
	shrq	$22, %r8
	testb	$1, -32(%rbp)
	cmovneq	%r8, %rax
	cmpq	%rdx, %rax
	cmovaeq	%rdx, %rax
	movl	$4294967295, %edx
	cmpq	%rdx, %rax
	cmovaeq	%rdx, %rax
	cmpl	%eax, 1016(%rcx)
	je	.LBB47_38
.LBB47_37:
	lock		incq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback16CONFIG_CONFLICTS(%rip)
.LBB47_38:
.Ltmp230:
	movq	%rsi, 32(%rsp)
	movq	%r14, %rdx
	movq	%rbx, %r8
	movq	%rdi, %r9
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core4free7reallocNtNtB6_4core8HeapCore7realloc
	nop
.Ltmp231:
	movb	$0, _RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip)
	testq	%rax, %rax
	jne	.LBB47_47
.LBB47_42:
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	je	.LBB47_43
.LBB47_44:
	cmpq	$1, %rax
	je	.LBB47_46
.LBB47_45:
	callq	*%rax
	testb	%al, %al
	jne	.LBB47_48
.LBB47_46:
	xorl	%eax, %eax
.LBB47_47:
	.seh_startepilogue
	addq	$168, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB47_41:
	lock		incq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback17EXIT_LOCK_GIVEUPS.llvm.2560762130558453100(%rip)
	movq	8(%rbp), %rsi
	movq	16(%rbp), %rdi
	movq	24(%rbp), %rbx
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	jne	.LBB47_44
.LBB47_43:
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit7resolve
	cmpq	$1, %rax
	jne	.LBB47_45
	jmp	.LBB47_46
.LBB47_3:
	xorps	%xmm0, %xmm0
	movaps	%xmm0, -16(%rbp)
	movaps	%xmm0, -32(%rbp)
	movaps	%xmm0, -48(%rbp)
	movaps	%xmm0, -64(%rbp)
	movaps	%xmm0, -80(%rbp)
	movq	$0, (%rbp)
	leaq	-80(%rbp), %rcx
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap28bind_slow_tagged_with_config
	cmpq	$1, %rax
	je	.LBB47_6
	movq	%rdx, %rcx
	jmp	.LBB47_5
.LBB47_48:
	movq	%r14, %rcx
	movq	%rbx, %rdx
	movq	%rdi, %r8
	movq	%rsi, %r9
	.seh_startepilogue
	addq	$168, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc19exit_escape_realloc
	.seh_handlerdata
	.long	$cppxdata$_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc@IMGREL
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc,unique,48
	.seh_endproc
	.def	"?dtor$35@?0?_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$35@?0?_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc@4HA":
.seh_proc "?dtor$35@?0?_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc@4HA"
.LBB47_35:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movb	$0, _RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip)
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end47:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc,unique,48
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc,unique,21
	.p2align	2, 0x0
$cppxdata$_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc@IMGREL
	.long	160
	.long	0
	.long	1
$stateUnwindMap$_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc:
	.long	-1
	.long	"?dtor$35@?0?_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc@4HA"@IMGREL
$ip2state$_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc:
	.long	.Lfunc_begin47@IMGREL
	.long	-1
	.long	.Ltmp230@IMGREL
	.long	0
	.long	.Ltmp231@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc14___rust_realloc,unique,48

	.def	_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed;
	.scl	2;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed,unique,49
	.globl	_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed
	.p2align	4
_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed:
.Lfunc_begin48:
.seh_proc _RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$152, %rsp
	.seh_stackalloc 152
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 16(%rbp)
	movq	%rdx, %r15
	movq	%rcx, %rdi
	movl	_tls_index(%rip), %eax
	movq	%gs:88, %rcx
	movq	(%rcx,%rax,8), %rax
	movq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rax), %r14
	leaq	-1(%r14), %rax
	cmpq	$-2, %rax
	jae	.LBB48_1
.LBB48_4:
	cmpq	$17, %rdi
	movl	$16, %ebx
	cmovaeq	%rdi, %rbx
	cmpq	$17, %r15
	jae	.LBB48_5
	leaq	-1(%rbx), %rax
	cmpq	$258751, %rax
	ja	.LBB48_11
	shrq	$4, %rax
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes10SIZE2CLASS(%rip), %rcx
	movzbl	(%rax,%rcx), %edx
	cmpq	$48, %rdx
	ja	.LBB48_79
.LBB48_75:
	movl	%edx, %eax
	shll	$7, %eax
	leaq	(%rax,%rdx,8), %rcx
	movzbl	1192(%r14,%rcx), %eax
	testq	%rax, %rax
	je	.LBB48_77
	addq	%r14, %rcx
	addq	$1192, %rcx
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
	jmp	.LBB48_78
.LBB48_1:
	testq	%r14, %r14
	je	.LBB48_2
.LBB48_12:
	xorps	%xmm0, %xmm0
	movaps	%xmm0, -32(%rbp)
	movaps	%xmm0, -48(%rbp)
	movaps	%xmm0, -64(%rbp)
	movaps	%xmm0, -80(%rbp)
	movaps	%xmm0, -96(%rbp)
	movq	$0, -16(%rbp)
	leaq	-96(%rbp), %rcx
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback13heap_ptr_impl.llvm.2560762130558453100
	testq	%rax, %rax
	je	.LBB48_56
	movq	%rax, %rbx
	movq	%r15, 8(%rbp)
	movq	%rdi, (%rbp)
	movb	$1, %r14b
	xorl	%eax, %eax
	lock		cmpxchgb	%r14b, _RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip)
	jne	.LBB48_14
.LBB48_27:
	lock		incq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback17LOCK_ACQUISITIONS.llvm.2560762130558453100(%rip)
	movzbl	-96(%rbp), %eax
	cmpl	$1, 800(%rbx)
	jne	.LBB48_42
	testb	$1, %al
	movq	(%rbp), %rdi
	movq	8(%rbp), %r15
	je	.LBB48_43
	movq	-88(%rbp), %rax
	cmpq	%rax, 808(%rbx)
	je	.LBB48_30
	jmp	.LBB48_43
.LBB48_5:
	cmpq	%r15, %rbx
	movq	%r15, %rcx
	cmovaq	%rbx, %rcx
	decq	%rcx
	cmpq	$258751, %rcx
	ja	.LBB48_11
	shrq	$4, %rcx
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes2SC(%rip), %rax
	movzbl	404(%rcx,%rax), %edx
	cmpb	$48, %dl
	ja	.LBB48_11
	leaq	-1(%r15), %rcx
	.p2align	4
.LBB48_8:
	movzbl	%dl, %edx
	movq	(%rax,%rdx,8), %r8
	testq	%rcx, %r8
	je	.LBB48_75
	orq	%rcx, %r8
	cmpq	$258751, %r8
	ja	.LBB48_11
	shrq	$4, %r8
	movzbl	404(%r8,%rax), %edx
	cmpb	$49, %dl
	jb	.LBB48_8
.LBB48_11:
	movq	%r14, %rcx
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry17heap_core_xthread13sidecar_drainNtNtNtB6_9heap_core4core8HeapCore39drain_large_sidecar_ingress_hot_bounded
	movq	%r14, %rcx
	movq	%rbx, %rdx
	movq	%r15, %r8
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5large16alloc_core_largeNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore11alloc_large
	testq	%rax, %rax
	sete	%cl
	orb	%dl, %cl
	testb	$1, %cl
	jne	.LBB48_55
.LBB48_78:
	movq	%rax, %rcx
	xorl	%edx, %edx
	movq	%rbx, %r8
	.seh_startepilogue
	addq	$152, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	jmp	memset
.LBB48_14:
	movl	$-1, %r15d
	movq	__imp_SwitchToThread(%rip), %r12
	xorl	%r13d, %r13d
	xorl	%esi, %esi
	xorl	%edi, %edi
	jmp	.LBB48_16
	.p2align	4
.LBB48_18:
	incl	%esi
	testb	$63, %sil
	je	.LBB48_19
.LBB48_23:
	callq	*%r12
.LBB48_16:
	movzbl	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip), %eax
	testb	%al, %al
	je	.LBB48_15
	cmpl	$64, %r13d
	jae	.LBB48_18
	cmpl	$6, %edi
	movl	$6, %ecx
	cmovbl	%edi, %ecx
	movl	$1, %eax
	.p2align	4
.LBB48_25:
	pause
	movl	%eax, %edx
	shrl	%cl, %edx
	incl	%eax
	testl	%edx, %edx
	je	.LBB48_25
	incl	%r13d
	incl	%edi
	cmovel	%r15d, %edi
	jmp	.LBB48_16
.LBB48_19:
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	je	.LBB48_20
	cmpq	$1, %rax
	je	.LBB48_23
.LBB48_22:
	callq	*%rax
	testb	%al, %al
	je	.LBB48_23
	jmp	.LBB48_72
.LBB48_15:
	xorl	%eax, %eax
	lock		cmpxchgb	%r14b, _RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip)
	jne	.LBB48_16
	jmp	.LBB48_27
.LBB48_20:
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit7resolve
	cmpq	$1, %rax
	je	.LBB48_23
	jmp	.LBB48_22
.LBB48_42:
	testb	$1, %al
	movq	(%rbp), %rdi
	movq	8(%rbp), %r15
	jne	.LBB48_43
.LBB48_30:
	cmpb	$0, -24(%rbp)
	movl	$10, %eax
	je	.LBB48_32
	movl	-20(%rbp), %eax
.LBB48_32:
	cmpl	$100, %eax
	movl	$100, %ecx
	movl	$100, %edx
	cmovbl	%eax, %edx
	imull	$100, %edx, %edx
	testl	%eax, %eax
	cmovel	%ecx, %edx
	testb	$1, -80(%rbp)
	movl	$268435456, %eax
	je	.LBB48_34
	movq	-72(%rbp), %rax
.LBB48_34:
	testb	$1, -32(%rbp)
	movl	$1000, %ecx
	cmovnel	-28(%rbp), %ecx
	cmpl	%edx, 840(%rbx)
	jne	.LBB48_43
	imulq	$274877907, %rcx, %rdx
	shrq	$38, %rdx
	movl	%edx, %r8d
	cmpq	%r8, 816(%rbx)
	jne	.LBB48_43
	imull	$1000, %edx, %edx
	subl	%edx, %ecx
	imull	$1000000, %ecx, %ecx
	cmpl	%ecx, 824(%rbx)
	jne	.LBB48_43
	cmpq	%rax, 832(%rbx)
	jne	.LBB48_43
	movq	-40(%rbp), %rcx
	testb	$1, -64(%rbp)
	movl	$4, %eax
	movl	$4, %edx
	je	.LBB48_40
	movq	-56(%rbp), %rdx
.LBB48_40:
	shrq	$22, %rcx
	testb	$1, -48(%rbp)
	cmovneq	%rcx, %rax
	cmpq	%rdx, %rax
	cmovaeq	%rdx, %rax
	movl	$4294967295, %ecx
	cmpq	%rcx, %rax
	cmovaeq	%rcx, %rax
	cmpl	%eax, 1016(%rbx)
	je	.LBB48_44
.LBB48_43:
	lock		incq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback16CONFIG_CONFLICTS(%rip)
.LBB48_44:
	cmpq	$17, %rdi
	movl	$16, %r14d
	cmovaeq	%rdi, %r14
	cmpq	$17, %r15
	jae	.LBB48_45
	leaq	-1(%r14), %rax
	cmpq	$258751, %rax
	ja	.LBB48_51
	shrq	$4, %rax
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes10SIZE2CLASS(%rip), %rcx
	movzbl	(%rax,%rcx), %edx
	cmpq	$48, %rdx
	ja	.LBB48_67
.LBB48_64:
	movl	%edx, %eax
	shll	$7, %eax
	leaq	(%rax,%rdx,8), %rcx
	movzbl	1192(%rbx,%rcx), %eax
	testq	%rax, %rax
	je	.LBB48_69
	addq	%rbx, %rcx
	addq	$1192, %rcx
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
	jmp	.LBB48_66
.LBB48_45:
	cmpq	%r15, %r14
	movq	%r15, %rcx
	cmovaq	%r14, %rcx
	decq	%rcx
	cmpq	$258751, %rcx
	ja	.LBB48_51
	shrq	$4, %rcx
	leaq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform12size_classes2SC(%rip), %rax
	movzbl	404(%rcx,%rax), %edx
	cmpb	$48, %dl
	ja	.LBB48_51
	leaq	-1(%r15), %rcx
	.p2align	4
.LBB48_48:
	movzbl	%dl, %edx
	movq	(%rax,%rdx,8), %r8
	testq	%rcx, %r8
	je	.LBB48_64
	orq	%rcx, %r8
	cmpq	$258751, %r8
	ja	.LBB48_51
	shrq	$4, %r8
	movzbl	404(%r8,%rax), %edx
	cmpb	$49, %dl
	jb	.LBB48_48
.LBB48_51:
.Ltmp236:
	movq	%rbx, %rcx
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry17heap_core_xthread13sidecar_drainNtNtNtB6_9heap_core4core8HeapCore39drain_large_sidecar_ingress_hot_bounded
	nop
.Ltmp237:
.Ltmp238:
	movq	%rbx, %rcx
	movq	%r14, %rdx
	movq	%r15, %r8
	callq	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5large16alloc_core_largeNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore11alloc_large
	nop
.Ltmp239:
	testq	%rax, %rax
	sete	%cl
	orb	%dl, %cl
	testb	$1, %cl
	jne	.LBB48_54
.LBB48_66:
	movq	%rax, %rcx
	xorl	%edx, %edx
	movq	%r14, %r8
	movq	%rax, %rsi
	callq	memset
	movq	%rsi, %rax
.LBB48_54:
	movb	$0, _RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip)
.LBB48_55:
	testq	%rax, %rax
	jne	.LBB48_61
.LBB48_56:
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	je	.LBB48_57
	cmpq	$1, %rax
	je	.LBB48_60
.LBB48_59:
	callq	*%rax
	testb	%al, %al
	jne	.LBB48_80
.LBB48_60:
	xorl	%eax, %eax
.LBB48_61:
	.seh_startepilogue
	addq	$152, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB48_72:
	lock		incq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback17EXIT_LOCK_GIVEUPS.llvm.2560762130558453100(%rip)
	movq	(%rbp), %rdi
	movq	8(%rbp), %r15
	jmp	.LBB48_56
.LBB48_2:
	xorps	%xmm0, %xmm0
	movaps	%xmm0, -32(%rbp)
	movaps	%xmm0, -48(%rbp)
	movaps	%xmm0, -64(%rbp)
	movaps	%xmm0, -80(%rbp)
	movaps	%xmm0, -96(%rbp)
	movq	$0, -16(%rbp)
	leaq	-96(%rbp), %rcx
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap28bind_slow_tagged_with_config
	cmpq	$1, %rax
	je	.LBB48_12
	movq	%rdx, %r14
	jmp	.LBB48_4
.LBB48_57:
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit7resolve
	cmpq	$1, %rax
	jne	.LBB48_59
	jmp	.LBB48_60
.LBB48_80:
	movq	%r15, %rcx
	movq	%rdi, %rdx
	movb	$1, %r8b
	.seh_startepilogue
	addq	$152, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	jmp	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc17exit_escape_alloc
.LBB48_77:
	movq	%r14, %rcx
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core5alloc3hotNtNtB6_4core8HeapCore20refill_magazine_slow
	testq	%rax, %rax
	jne	.LBB48_78
	jmp	.LBB48_56
.LBB48_69:
.Ltmp234:
	movq	%rbx, %rcx
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core5alloc3hotNtNtB6_4core8HeapCore20refill_magazine_slow
	nop
.Ltmp235:
	testq	%rax, %rax
	jne	.LBB48_66
	xorl	%eax, %eax
	jmp	.LBB48_54
.LBB48_79:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665(%rip), %r8
	movq	%rdx, %rcx
	movl	$49, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	ud2
.LBB48_67:
.Ltmp232:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665(%rip), %r8
	movq	%rdx, %rcx
	movl	$49, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	nop
.Ltmp233:
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed@IMGREL
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed,unique,49
	.seh_endproc
	.def	"?dtor$41@?0?_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$41@?0?_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed@4HA":
.seh_proc "?dtor$41@?0?_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed@4HA"
.LBB48_41:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movb	$0, _RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8fallback4LOCK.llvm.2560762130558453100(%rip)
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end48:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed,unique,49
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed,unique,22
	.p2align	2, 0x0
$cppxdata$_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed@IMGREL
	.long	0
	.long	0
	.long	7
	.long	$ip2state$_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed@IMGREL
	.long	144
	.long	0
	.long	1
$stateUnwindMap$_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed:
	.long	-1
	.long	"?dtor$41@?0?_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed@4HA"@IMGREL
$ip2state$_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed:
	.long	.Lfunc_begin48@IMGREL
	.long	-1
	.long	.Ltmp236@IMGREL
	.long	0
	.long	.Ltmp239@IMGREL
	.long	-1
	.long	.Ltmp234@IMGREL
	.long	0
	.long	.Ltmp235@IMGREL
	.long	-1
	.long	.Ltmp232@IMGREL
	.long	0
	.long	.Ltmp233@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvCs9hJ03s5DiqP_7___rustc19___rust_alloc_zeroed,unique,49

	.def	_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small25alloc_core_small_magazineNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore20flush_class_internal;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small25alloc_core_small_magazineNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore20flush_class_internal,unique,50
	.p2align	4
_RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small25alloc_core_small_magazineNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore20flush_class_internal:
.Lfunc_begin49:
.seh_proc _RNvMNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small25alloc_core_small_magazineNtNtNtB6_10alloc_core15alloc_core_impl9AllocCore20flush_class_internal
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
	subq	$248, %rsp
	.seh_stackalloc 248
	.seh_endprologue
	xorps	%xmm0, %xmm0
	movaps	%xmm0, 224(%rsp)
	movaps	%xmm0, 208(%rsp)
	movaps	%xmm0, 192(%rsp)
	movaps	%xmm0, 176(%rsp)
	movaps	%xmm0, 160(%rsp)
	movaps	%xmm0, 144(%rsp)
	movaps	%xmm0, 128(%rsp)
	movaps	%xmm0, 112(%rsp)
	movq	%rcx, 48(%rsp)
	leaq	48(%rcx), %r15
	leal	1(%rdx), %eax
	movl	%eax, 84(%rsp)
	movq	%rdx, 64(%rsp)
	leal	-127(%rdx), %eax
	movl	%eax, 80(%rsp)
	xorl	%esi, %esi
	movl	$389120, %edx
	xorl	%r12d, %r12d
.LBB49_28:
	leaq	(,%r12,8), %rbx
	negq	%rbx
	movq	%r12, %rdi
	negq	%rdi
	movq	%r12, %r10
	.p2align	4
.LBB49_1:
	movq	(%r8,%r10,8), %r13
	testq	%r13, %r13
	jne	.LBB49_4
	incq	%r10
	addq	$-8, %rbx
	decq	%rdi
	cmpq	%r9, %r10
	jb	.LBB49_1
	jmp	.LBB49_3
	.p2align	4
.LBB49_4:
	movq	%r8, %r11
	subq	%rbx, %r11
	andq	$-4194304, %r13
	leaq	1(%r10), %rax
	cmpq	%rax, %r9
	cmovaq	%r9, %rax
	movq	%r10, %r12
	.p2align	4
.LBB49_5:
	movq	%r12, %r14
	incq	%r12
	cmpq	%r9, %r12
	jae	.LBB49_6
	movq	8(%r8,%r14,8), %rcx
	testq	%rcx, %rcx
	je	.LBB49_7
	andq	$-4194304, %rcx
	cmpq	%r13, %rcx
	je	.LBB49_5
	jmp	.LBB49_7
.LBB49_6:
	movq	%rax, %r12
.LBB49_7:
	cmpq	$17, %rsi
	jae	.LBB49_8
	leaq	(,%rsi,8), %rax
	xorl	%ecx, %ecx
	.p2align	4
.LBB49_12:
	cmpq	%rcx, %rax
	je	.LBB49_15
	cmpq	%r13, 112(%rsp,%rcx)
	leaq	8(%rcx), %rcx
	jne	.LBB49_12
	movq	%rsi, %rax
	jmp	.LBB49_27
	.p2align	4
.LBB49_15:
	movq	48(%rsp), %rax
	cmpq	%r13, 304(%rax)
	movq	%r15, 40(%rsp)
	jne	.LBB49_17
	movq	48(%rsp), %rax
	movq	312(%rax), %rbp
	jmp	.LBB49_30
.LBB49_17:
	movq	%r13, %rcx
	shrq	$22, %rcx
	movl	%ecx, %eax
	andl	$15, %eax
	shll	$4, %eax
	cmpq	%r13, (%r15,%rax)
	jne	.LBB49_18
	addq	%r15, %rax
	movq	8(%rax), %rbp
.LBB49_30:
	cmpq	%r9, %r14
	jae	.LBB49_52
	cmpq	%r10, %r12
	jb	.LBB49_52
	movq	(%rbp), %rax
	cmpb	$0, 28(%rbp)
	movl	$270336, %r14d
	cmoveq	%rdx, %r14
	subq	%r14, %rax
	movl	$0, %r15d
	cmovaeq	%rax, %r15
	cmpq	%r10, %r12
	jne	.LBB49_33
.LBB49_26:
	movq	%rsi, %rax
	movq	40(%rsp), %r15
	jmp	.LBB49_27
.LBB49_33:
	movq	64(%rsp), %rcx
	movl	5120(%rbp,%rcx,4), %ecx
	addq	%r12, %rdi
	leaq	(%r8,%rdi,8), %r10
	subq	%rbx, %r10
	leaq	(%rcx,%rbp), %rax
	movq	$0, 88(%rsp)
	movq	%rcx, 96(%rsp)
	cmpl	$-1, %ecx
	movl	$0, %ecx
	cmovneq	%rax, %rcx
	xorl	%edi, %edi
.LBB49_34:
	movq	%rcx, 104(%rsp)
	movl	%edi, 60(%rsp)
	jmp	.LBB49_35
	.p2align	4
.LBB49_42:
	addq	$8, %r11
	cmpq	%r10, %r11
	je	.LBB49_43
.LBB49_35:
	movq	(%r11), %rdx
	subq	%rbp, %rdx
	movl	%edx, %edi
	movq	%rdi, %rax
	subq	%r14, %rax
	cmpq	%r15, %rax
	jae	.LBB49_42
	movl	%edx, %ecx
	shrl	$4, %ecx
	movzbl	5512(%rbp,%rcx), %ebx
	movl	%ebx, %eax
	andb	$63, %al
	cmpb	84(%rsp), %al
	setne	%al
	cmpb	$-64, %bl
	setl	%bl
	orb	%al, %bl
	jne	.LBB49_42
	movq	104(%rsp), %rax
	movq	%rax, (%rbp,%rdi)
	leaq	(%rdi,%rbp), %rax
	movl	80(%rsp), %edi
	movb	%dil, 5512(%rbp,%rcx)
	movl	60(%rsp), %edi
	incl	%edi
	addq	$8, %r11
	movb	$1, %cl
	movq	%rcx, 88(%rsp)
	movq	%rax, %rcx
	movl	%edx, 76(%rsp)
	cmpq	%r10, %r11
	jne	.LBB49_34
	jmp	.LBB49_38
.LBB49_43:
	movl	76(%rsp), %eax
	movl	%eax, %edx
	testb	$1, 88(%rsp)
	movl	60(%rsp), %edi
	je	.LBB49_44
.LBB49_38:
	movq	64(%rsp), %rcx
	movl	%edx, 5120(%rbp,%rcx,4)
	cmpl	$-1, 96(%rsp)
	jne	.LBB49_44
	movq	48(%rsp), %rcx
	movq	1000(%rcx), %r11
	testq	%r11, %r11
	je	.LBB49_44
	movl	112(%rbp), %ecx
	movl	%ecx, %r10d
	shrl	$6, %r10d
	cmpq	$4095, %rcx
	ja	.LBB49_53
	movl	$1, %eax
	shlq	%cl, %rax
	movq	64(%rsp), %rcx
	shlq	$9, %rcx
	addq	%rcx, %r11
	orq	%rax, (%r11,%r10,8)
.LBB49_44:
	movl	20(%rbp), %ecx
	movl	%ecx, %edx
	subl	%edi, %edx
	jb	.LBB49_54
	movl	%edx, 20(%rbp)
	testl	%edi, %edi
	je	.LBB49_20
	cmpl	%edi, %ecx
	movl	$389120, %edx
	jne	.LBB49_26
	movq	48(%rsp), %rcx
	cmpq	936(%rcx), %rbp
	je	.LBB49_26
	cmpl	$0, 24(%rbp)
	jne	.LBB49_26
	cmpb	$1, 28(%rbp)
	jne	.LBB49_26
	movq	48(%rsp), %rcx
	movq	%rbp, %rdx
	movq	%r9, %rdi
	movq	%r8, %rbx
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small21alloc_core_small_pool26alloc_core_small_pool_implNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore29release_or_pool_empty_segment
	movl	$389120, %edx
	movq	%rbx, %r8
	movq	%rdi, %r9
	movl	$16, %eax
	cmpq	$16, %rsi
	movq	40(%rsp), %r15
	je	.LBB49_27
	movq	%r13, 112(%rsp,%rsi,8)
	incq	%rsi
	movq	%rsi, %rax
	jmp	.LBB49_27
.LBB49_18:
	movq	%r13, %rax
	shrq	$35, %rax
	movq	%r13, %rdx
	shrq	$48, %rdx
	xorl	%edx, %eax
	xorl	%eax, %ecx
	andl	$8191, %ecx
	movq	48(%rsp), %rax
	movq	320(%rax), %rdx
	movq	%rcx, %r15
	.p2align	4
.LBB49_19:
	movq	(%rdx,%r15,8), %rbp
	testq	%rbp, %rbp
	je	.LBB49_20
	movq	%rbp, %rax
	andq	$-4194304, %rax
	cmpq	%r13, %rax
	je	.LBB49_23
	incl	%r15d
	andl	$8191, %r15d
	cmpq	%rcx, %r15
	jne	.LBB49_19
	jmp	.LBB49_20
.LBB49_23:
	movq	%rbp, %rcx
	andq	$4194303, %rcx
	movl	$389120, %edx
	je	.LBB49_30
	leaq	-1(%rcx), %rdx
	movq	48(%rsp), %rax
	movl	344(%rax), %eax
	cmpq	%rax, %rdx
	jae	.LBB49_20
	movq	48(%rsp), %rax
	movq	32(%rax), %rax
	movq	-8(%rax,%rcx,8), %rbp
	testq	%rbp, %rbp
	movl	$389120, %edx
	jne	.LBB49_30
	jmp	.LBB49_26
.LBB49_20:
	movq	%rsi, %rax
	movq	40(%rsp), %r15
	movl	$389120, %edx
	.p2align	4
.LBB49_27:
	movq	%rax, %rsi
	cmpq	%r9, %r12
	jb	.LBB49_28
.LBB49_3:
	.seh_startepilogue
	addq	$248, %rsp
	popq	%rbx
	popq	%rbp
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	.seh_endepilogue
	retq
.LBB49_8:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.149(%rip), %r9
	movl	$16, %r8d
	xorl	%ecx, %ecx
	movq	%rsi, %rdx
	callq	_RNvNtNtCsd9eshbB3hfz_4core5slice5index16slice_index_fail
	ud2
.LBB49_52:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.148(%rip), %rax
	movq	%r10, %rcx
	movq	%r12, %rdx
	movq	%r9, %r8
	movq	%rax, %r9
	callq	_RNvNtNtCsd9eshbB3hfz_4core5slice5index16slice_index_fail
	ud2
.LBB49_54:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.150(%rip), %rcx
	movl	$58, %edx
	callq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core8platform5fatal5fatal
	ud2
.LBB49_53:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.152(%rip), %r8
	movl	$64, %edx
	movq	%r10, %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	ud2
.Lfunc_end49:
	.seh_endproc

	.def	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small16alloc_core_small9directoryNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore16publish_nonempty;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small16alloc_core_small9directoryNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore16publish_nonempty,unique,51
	.p2align	4
_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small16alloc_core_small9directoryNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore16publish_nonempty:
.Lfunc_begin50:
.seh_proc _RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small16alloc_core_small9directoryNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore16publish_nonempty
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	testq	%rcx, %rcx
	je	.LBB50_4
	movq	%rcx, %rax
	movl	$1, %r9d
	movl	%r8d, %ecx
	shlq	%cl, %r9
	cmpq	$49, %rdx
	jae	.LBB50_6
	movq	%r8, %rcx
	shrq	$6, %rcx
	cmpq	$4096, %r8
	jae	.LBB50_5
	shlq	$9, %rdx
	addq	%rdx, %rax
	orq	%r9, (%rax,%rcx,8)
.LBB50_4:
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	retq
.LBB50_6:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.152(%rip), %r8
	movq	%rdx, %rcx
	movl	$49, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	ud2
.LBB50_5:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.152(%rip), %r8
	movl	$64, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	ud2
.Lfunc_end50:
	.seh_endproc

	.def	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small21alloc_core_small_pool26alloc_core_small_pool_implNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore29release_or_pool_empty_segment;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small21alloc_core_small_pool26alloc_core_small_pool_implNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore29release_or_pool_empty_segment,unique,52
	.p2align	4
_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small21alloc_core_small_pool26alloc_core_small_pool_implNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore29release_or_pool_empty_segment:
.Lfunc_begin51:
.seh_proc _RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core5small21alloc_core_small_pool26alloc_core_small_pool_implNtNtNtB8_10alloc_core15alloc_core_impl9AllocCore29release_or_pool_empty_segment
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	movl	1012(%rcx), %eax
	cmpl	1016(%rcx), %eax
	jae	.LBB51_1
	movq	$0, 88(%rdx)
	movq	984(%rcx), %r8
	movq	%r8, 80(%rdx)
	testq	%r8, %r8
	je	.LBB51_103
	movq	%rdx, 88(%r8)
	movl	1012(%rcx), %eax
	jmp	.LBB51_106
.LBB51_1:
	movq	1000(%rcx), %r10
	testq	%r10, %r10
	je	.LBB51_4
	movq	%rcx, %r8
	movl	112(%rdx), %ecx
	movl	%ecx, %eax
	shrl	$6, %eax
	movq	$-2, %r9
	rolq	%cl, %r9
	cmpl	$4096, %ecx
	jae	.LBB51_107
	leaq	(%r10,%rax,8), %rax
	andq	%r9, (%rax)
	andq	%r9, 512(%rax)
	andq	%r9, 1024(%rax)
	andq	%r9, 1536(%rax)
	andq	%r9, 2048(%rax)
	andq	%r9, 2560(%rax)
	andq	%r9, 3072(%rax)
	andq	%r9, 3584(%rax)
	andq	%r9, 4096(%rax)
	andq	%r9, 4608(%rax)
	andq	%r9, 5120(%rax)
	andq	%r9, 5632(%rax)
	andq	%r9, 6144(%rax)
	andq	%r9, 6656(%rax)
	andq	%r9, 7168(%rax)
	andq	%r9, 7680(%rax)
	andq	%r9, 8192(%rax)
	andq	%r9, 8704(%rax)
	andq	%r9, 9216(%rax)
	andq	%r9, 9728(%rax)
	andq	%r9, 10240(%rax)
	andq	%r9, 10752(%rax)
	andq	%r9, 11264(%rax)
	andq	%r9, 11776(%rax)
	andq	%r9, 12288(%rax)
	andq	%r9, 12800(%rax)
	andq	%r9, 13312(%rax)
	andq	%r9, 13824(%rax)
	andq	%r9, 14336(%rax)
	andq	%r9, 14848(%rax)
	andq	%r9, 15360(%rax)
	andq	%r9, 15872(%rax)
	andq	%r9, 16384(%rax)
	andq	%r9, 16896(%rax)
	andq	%r9, 17408(%rax)
	andq	%r9, 17920(%rax)
	andq	%r9, 18432(%rax)
	andq	%r9, 18944(%rax)
	andq	%r9, 19456(%rax)
	andq	%r9, 19968(%rax)
	andq	%r9, 20480(%rax)
	andq	%r9, 20992(%rax)
	andq	%r9, 21504(%rax)
	andq	%r9, 22016(%rax)
	andq	%r9, 22528(%rax)
	andq	%r9, 23040(%rax)
	andq	%r9, 23552(%rax)
	andq	%r9, 24064(%rax)
	andq	%r9, 24576(%rax)
	movq	%r8, %rcx
.LBB51_4:
	lock		incq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core10alloc_core8counters14DECOMMIT_CALLS.llvm.2560762130558453100(%rip)
	movq	$270336, (%rdx)
	movl	$1, 24(%rdx)
	movl	112(%rdx), %eax
	movzwl	1020(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_5
	movzwl	1022(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_7
.LBB51_8:
	movzwl	1024(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_9
.LBB51_10:
	movzwl	1026(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_11
.LBB51_12:
	movzwl	1028(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_13
.LBB51_14:
	movzwl	1030(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_15
.LBB51_16:
	movzwl	1032(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_17
.LBB51_18:
	movzwl	1034(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_19
.LBB51_20:
	movzwl	1036(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_21
.LBB51_22:
	movzwl	1038(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_23
.LBB51_24:
	movzwl	1040(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_25
.LBB51_26:
	movzwl	1042(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_27
.LBB51_28:
	movzwl	1044(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_29
.LBB51_30:
	movzwl	1046(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_31
.LBB51_32:
	movzwl	1048(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_33
.LBB51_34:
	movzwl	1050(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_35
.LBB51_36:
	movzwl	1052(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_37
.LBB51_38:
	movzwl	1054(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_39
.LBB51_40:
	movzwl	1056(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_41
.LBB51_42:
	movzwl	1058(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_43
.LBB51_44:
	movzwl	1060(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_45
.LBB51_46:
	movzwl	1062(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_47
.LBB51_48:
	movzwl	1064(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_49
.LBB51_50:
	movzwl	1066(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_51
.LBB51_52:
	movzwl	1068(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_53
.LBB51_54:
	movzwl	1070(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_55
.LBB51_56:
	movzwl	1072(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_57
.LBB51_58:
	movzwl	1074(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_59
.LBB51_60:
	movzwl	1076(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_61
.LBB51_62:
	movzwl	1078(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_63
.LBB51_64:
	movzwl	1080(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_65
.LBB51_66:
	movzwl	1082(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_67
.LBB51_68:
	movzwl	1084(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_69
.LBB51_70:
	movzwl	1086(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_71
.LBB51_72:
	movzwl	1088(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_73
.LBB51_74:
	movzwl	1090(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_75
.LBB51_76:
	movzwl	1092(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_77
.LBB51_78:
	movzwl	1094(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_79
.LBB51_80:
	movzwl	1096(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_81
.LBB51_82:
	movzwl	1098(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_83
.LBB51_84:
	movzwl	1100(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_85
.LBB51_86:
	movzwl	1102(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_87
.LBB51_88:
	movzwl	1104(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_89
.LBB51_90:
	movzwl	1106(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_91
.LBB51_92:
	movzwl	1108(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_93
.LBB51_94:
	movzwl	1110(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_95
.LBB51_96:
	movzwl	1112(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_97
.LBB51_98:
	movzwl	1114(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_99
.LBB51_100:
	movzwl	1116(%rcx), %r8d
	cmpl	%r8d, %eax
	je	.LBB51_101
.LBB51_102:
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	jmp	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core7segment13segment_table18segment_table_implNtB2_12SegmentTable7recycle
.LBB51_5:
	movw	$-1, 1020(%rcx)
	movzwl	1022(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_8
.LBB51_7:
	movw	$-1, 1022(%rcx)
	movzwl	1024(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_10
.LBB51_9:
	movw	$-1, 1024(%rcx)
	movzwl	1026(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_12
.LBB51_11:
	movw	$-1, 1026(%rcx)
	movzwl	1028(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_14
.LBB51_13:
	movw	$-1, 1028(%rcx)
	movzwl	1030(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_16
.LBB51_15:
	movw	$-1, 1030(%rcx)
	movzwl	1032(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_18
.LBB51_17:
	movw	$-1, 1032(%rcx)
	movzwl	1034(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_20
.LBB51_19:
	movw	$-1, 1034(%rcx)
	movzwl	1036(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_22
.LBB51_21:
	movw	$-1, 1036(%rcx)
	movzwl	1038(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_24
.LBB51_23:
	movw	$-1, 1038(%rcx)
	movzwl	1040(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_26
.LBB51_25:
	movw	$-1, 1040(%rcx)
	movzwl	1042(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_28
.LBB51_27:
	movw	$-1, 1042(%rcx)
	movzwl	1044(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_30
.LBB51_29:
	movw	$-1, 1044(%rcx)
	movzwl	1046(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_32
.LBB51_31:
	movw	$-1, 1046(%rcx)
	movzwl	1048(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_34
.LBB51_33:
	movw	$-1, 1048(%rcx)
	movzwl	1050(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_36
.LBB51_35:
	movw	$-1, 1050(%rcx)
	movzwl	1052(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_38
.LBB51_37:
	movw	$-1, 1052(%rcx)
	movzwl	1054(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_40
.LBB51_39:
	movw	$-1, 1054(%rcx)
	movzwl	1056(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_42
.LBB51_41:
	movw	$-1, 1056(%rcx)
	movzwl	1058(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_44
.LBB51_43:
	movw	$-1, 1058(%rcx)
	movzwl	1060(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_46
.LBB51_45:
	movw	$-1, 1060(%rcx)
	movzwl	1062(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_48
.LBB51_47:
	movw	$-1, 1062(%rcx)
	movzwl	1064(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_50
.LBB51_49:
	movw	$-1, 1064(%rcx)
	movzwl	1066(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_52
.LBB51_51:
	movw	$-1, 1066(%rcx)
	movzwl	1068(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_54
.LBB51_53:
	movw	$-1, 1068(%rcx)
	movzwl	1070(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_56
.LBB51_55:
	movw	$-1, 1070(%rcx)
	movzwl	1072(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_58
.LBB51_57:
	movw	$-1, 1072(%rcx)
	movzwl	1074(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_60
.LBB51_59:
	movw	$-1, 1074(%rcx)
	movzwl	1076(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_62
.LBB51_61:
	movw	$-1, 1076(%rcx)
	movzwl	1078(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_64
.LBB51_63:
	movw	$-1, 1078(%rcx)
	movzwl	1080(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_66
.LBB51_65:
	movw	$-1, 1080(%rcx)
	movzwl	1082(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_68
.LBB51_67:
	movw	$-1, 1082(%rcx)
	movzwl	1084(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_70
.LBB51_69:
	movw	$-1, 1084(%rcx)
	movzwl	1086(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_72
.LBB51_71:
	movw	$-1, 1086(%rcx)
	movzwl	1088(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_74
.LBB51_73:
	movw	$-1, 1088(%rcx)
	movzwl	1090(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_76
.LBB51_75:
	movw	$-1, 1090(%rcx)
	movzwl	1092(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_78
.LBB51_77:
	movw	$-1, 1092(%rcx)
	movzwl	1094(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_80
.LBB51_79:
	movw	$-1, 1094(%rcx)
	movzwl	1096(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_82
.LBB51_81:
	movw	$-1, 1096(%rcx)
	movzwl	1098(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_84
.LBB51_83:
	movw	$-1, 1098(%rcx)
	movzwl	1100(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_86
.LBB51_85:
	movw	$-1, 1100(%rcx)
	movzwl	1102(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_88
.LBB51_87:
	movw	$-1, 1102(%rcx)
	movzwl	1104(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_90
.LBB51_89:
	movw	$-1, 1104(%rcx)
	movzwl	1106(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_92
.LBB51_91:
	movw	$-1, 1106(%rcx)
	movzwl	1108(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_94
.LBB51_93:
	movw	$-1, 1108(%rcx)
	movzwl	1110(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_96
.LBB51_95:
	movw	$-1, 1110(%rcx)
	movzwl	1112(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_98
.LBB51_97:
	movw	$-1, 1112(%rcx)
	movzwl	1114(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_100
.LBB51_99:
	movw	$-1, 1114(%rcx)
	movzwl	1116(%rcx), %r8d
	cmpl	%r8d, %eax
	jne	.LBB51_102
.LBB51_101:
	movw	$-1, 1116(%rcx)
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	jmp	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc10alloc_core7segment13segment_table18segment_table_implNtB2_12SegmentTable7recycle
.LBB51_103:
	movq	%rdx, 992(%rcx)
.LBB51_106:
	movq	%rdx, 984(%rcx)
	incl	%eax
	movl	%eax, 1012(%rcx)
	.seh_startepilogue
	addq	$40, %rsp
	.seh_endepilogue
	retq
.LBB51_107:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.153(%rip), %r8
	movl	$64, %edx
	movq	%rax, %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking18panic_bounds_check
	ud2
.Lfunc_end51:
	.seh_endproc

	.def	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text$unlikely,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,53
	.p2align	4
_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin52:
.seh_proc _RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
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
	subq	$64, %rsp
	.seh_stackalloc 64
	leaq	64(%rsp), %rbp
	.seh_setframe %rbp, 64
	.seh_endprologue
	movq	$-2, -8(%rbp)
	movq	%rdx, -24(%rbp)
	movq	32(%rcx), %rdi
	movq	40(%rcx), %rsi
	incq	%rsi
	cmpq	%rdi, %rsi
	cmovaeq	%rdi, %rsi
	movq	24(%rcx), %rbx
	leaq	(%rbx,%rsi), %r8
	movq	_RNvNvNtNtNtCsloPA8o21a2b_6memchr4arch6x86_646memchr11memrchr_raw2FN(%rip), %rax
	movb	$1, -9(%rbp)
.Ltmp240:
	movb	$10, %cl
	movq	%rbx, %rdx
	callq	*%rax
	nop
.Ltmp241:
	cmpq	$1, %rax
	jne	.LBB52_2
	subq	%rbx, %rdx
	leaq	1(%rdx), %r14
	cmpq	%rdi, %rdx
	jb	.LBB52_3
	movb	$1, -9(%rbp)
.Ltmp242:
	leaq	anon.7e59a8bc2ee43327c2a223b709be5804.51.llvm.84982629146409838(%rip), %r9
	xorl	%ecx, %ecx
	movq	%r14, %rdx
	movq	%rdi, %r8
	callq	_RNvNtNtCsd9eshbB3hfz_4core5slice5index16slice_index_fail
	nop
.Ltmp243:
	ud2
.LBB52_2:
	xorl	%r14d, %r14d
.LBB52_3:
	leaq	(%rbx,%r14), %r8
	movq	_RNvNvNtNtNtCsloPA8o21a2b_6memchr4arch6x86_646memchr9count_raw2FN(%rip), %rax
	movb	$1, -9(%rbp)
.Ltmp244:
	movb	$10, %cl
	movq	%rbx, %rdx
	callq	*%rax
	nop
.Ltmp245:
	incq	%rax
	subq	%r14, %rsi
	movb	$0, -9(%rbp)
.Ltmp246:
	movq	-24(%rbp), %rcx
	movq	%rax, %rdx
	movq	%rsi, %r8
	callq	_RNvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB5_5Error6syntax
	nop
.Ltmp247:
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.long	$cppxdata$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text$unlikely,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,53
	.seh_endproc
	.def	"?dtor$9@?0?_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$9@?0?_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$9@?0?_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB52_9:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	64(%rdx), %rbp
	.seh_endprologue
	cmpb	$0, -9(%rbp)
	jne	.LBB52_10
.LBB52_11:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.LBB52_10:
	movq	-24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error9ErrorCodeECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB52_11
.Lfunc_end52:
	.seh_handlerdata
	.section	.text$unlikely,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,53
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,23
	.p2align	2, 0x0
$cppxdata$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	56
	.long	0
	.long	1
$stateUnwindMap$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$9@?0?_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin52@IMGREL
	.long	-1
	.long	.Ltmp240@IMGREL
	.long	0
	.long	.Ltmp247@IMGREL
	.long	-1
	.section	.text$unlikely,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,53

	.def	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE12parse_numberCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.globl	__xmm@00000000000000004530000043300000
	.section	.rdata,"dr",discard,__xmm@00000000000000004530000043300000
	.p2align	4, 0x0
__xmm@00000000000000004530000043300000:
	.long	1127219200
	.long	1160773632
	.long	0
	.long	0
	.globl	__xmm@45300000000000004330000000000000
	.section	.rdata,"dr",discard,__xmm@45300000000000004330000000000000
	.p2align	4, 0x0
__xmm@45300000000000004330000000000000:
	.quad	0x4330000000000000
	.quad	0x4530000000000000
	.globl	__real@7fe1ccf385ebc8a0
	.section	.rdata,"dr",discard,__real@7fe1ccf385ebc8a0
	.p2align	3, 0x0
__real@7fe1ccf385ebc8a0:
	.quad	0x7fe1ccf385ebc8a0
	.globl	__xmm@80000000000000008000000000000000
	.section	.rdata,"dr",discard,__xmm@80000000000000008000000000000000
	.p2align	4, 0x0
__xmm@80000000000000008000000000000000:
	.quad	0x8000000000000000
	.quad	0x8000000000000000
	.section	.text,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE12parse_numberCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,54
	.p2align	4
_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE12parse_numberCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin53:
.seh_proc _RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE12parse_numberCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%r15
	.seh_pushreg %r15
	pushq	%r14
	.seh_pushreg %r14
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
	subq	$80, %rsp
	.seh_stackalloc 80
	.seh_endprologue
	movq	32(%rdx), %rsi
	movq	40(%rdx), %r10
	cmpq	%rsi, %r10
	jae	.LBB53_6
	movq	24(%rdx), %r11
	movzbl	(%r11,%r10), %eax
	cmpl	$46, %eax
	je	.LBB53_11
	cmpl	$69, %eax
	je	.LBB53_4
	cmpl	$101, %eax
	jne	.LBB53_6
.LBB53_4:
	movq	%rcx, %rsi
	movl	$0, 32(%rsp)
	leaq	56(%rsp), %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE14parse_exponentCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	cmpb	$0, 56(%rsp)
	je	.LBB53_10
	movq	64(%rsp), %rax
	movq	%rax, 8(%rsi)
	movq	$-1, (%rsi)
	jmp	.LBB53_43
.LBB53_6:
	movl	$1, %edx
	testb	%r8b, %r8b
	je	.LBB53_8
	movq	%r9, %rax
	jmp	.LBB53_42
.LBB53_8:
	movq	%r9, %rax
	negq	%rax
	js	.LBB53_19
	movq	%r9, %xmm0
	punpckldq	__xmm@00000000000000004530000043300000(%rip), %xmm0
	subpd	__xmm@45300000000000004330000000000000(%rip), %xmm0
	movapd	%xmm0, %xmm1
	unpckhpd	%xmm0, %xmm1
	addsd	%xmm0, %xmm1
	movq	%xmm1, %rdx
	movabsq	$-9223372036854775808, %rax
	orq	%rdx, %rax
	jmp	.LBB53_41
.LBB53_10:
	movq	64(%rsp), %rax
	xorl	%edx, %edx
	movq	%rsi, %rcx
	jmp	.LBB53_42
.LBB53_11:
	leaq	1(%r10), %r14
	movq	%r14, 40(%rdx)
	cmpq	%rsi, %r14
	jae	.LBB53_25
	movabsq	$1844674407370955160, %rdi
	movl	%r10d, %eax
	subl	%esi, %eax
	incl	%eax
	movq	%r10, %rbx
	notq	%rbx
	addq	%rsi, %rbx
	addq	$2, %r10
	addq	%r14, %r11
	xorl	%esi, %esi
	leaq	1(%rdi), %r14
	jmp	.LBB53_14
	.p2align	4
.LBB53_13:
	movzbl	%bpl, %r15d
	leaq	(%r10,%rsi), %r12
	movq	%r12, 40(%rdx)
	leaq	(%r9,%r9,4), %r9
	leaq	(%r15,%r9,2), %r9
	incq	%rsi
	cmpq	%rsi, %rbx
	je	.LBB53_24
.LBB53_14:
	movzbl	(%r11,%rsi), %r15d
	leal	-48(%r15), %ebp
	cmpb	$10, %bpl
	jae	.LBB53_20
	cmpq	%rdi, %r9
	jbe	.LBB53_13
	cmpq	%r14, %r9
	jne	.LBB53_18
	cmpb	$5, %bpl
	jbe	.LBB53_13
.LBB53_18:
	negl	%esi
	movl	%esi, 32(%rsp)
	leaq	40(%rsp), %rax
	movq	%rcx, %rsi
	movq	%rax, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE22parse_decimal_overflowB7_
	jmp	.LBB53_23
.LBB53_19:
	movl	$2, %edx
	jmp	.LBB53_42
.LBB53_20:
	testl	%esi, %esi
	je	.LBB53_45
	negq	%rsi
	orl	$32, %r15d
	cmpl	$101, %r15d
	jne	.LBB53_26
	movl	%esi, 32(%rsp)
	leaq	40(%rsp), %rax
	movq	%rcx, %rsi
	movq	%rax, %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE14parse_exponentCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB53_23:
	movq	%rsi, %rcx
	cmpb	$0, 40(%rsp)
	je	.LBB53_40
	jmp	.LBB53_39
.LBB53_24:
	testl	%eax, %eax
	jne	.LBB53_27
.LBB53_25:
	movq	$5, 56(%rsp)
	jmp	.LBB53_46
.LBB53_26:
	movl	%esi, %eax
.LBB53_27:
	movq	%r9, %xmm1
	punpckldq	__xmm@00000000000000004530000043300000(%rip), %xmm1
	subpd	__xmm@45300000000000004330000000000000(%rip), %xmm1
	movapd	%xmm1, %xmm0
	unpckhpd	%xmm1, %xmm0
	addsd	%xmm1, %xmm0
	movl	%eax, %r9d
	negl	%r9d
	cmovsl	%eax, %r9d
	cmpl	$309, %r9d
	jb	.LBB53_32
	xorpd	%xmm1, %xmm1
	movsd	__real@7fe1ccf385ebc8a0(%rip), %xmm2
	.p2align	4
.LBB53_29:
	ucomisd	%xmm1, %xmm0
	jne	.LBB53_30
	jnp	.LBB53_34
.LBB53_30:
	testl	%eax, %eax
	jns	.LBB53_47
	divsd	%xmm2, %xmm0
	addl	$308, %eax
	movl	%eax, %r9d
	negl	%r9d
	cmovsl	%eax, %r9d
	cmpl	$309, %r9d
	jae	.LBB53_29
.LBB53_32:
	movl	%r9d, %r9d
	leaq	_RNvNtCsfZWMcdrZHWk_10serde_json2de5POW10(%rip), %r10
	movsd	(%r10,%r9,8), %xmm1
	testl	%eax, %eax
	js	.LBB53_44
	mulsd	%xmm1, %xmm0
	movq	%xmm0, %rax
	movabsq	$9223372036854775807, %r9
	andq	%rax, %r9
	movabsq	$9218868437227405312, %rax
	cmpq	%rax, %r9
	je	.LBB53_47
.LBB53_34:
	testb	%r8b, %r8b
	jne	.LBB53_36
.LBB53_35:
	xorpd	__xmm@80000000000000008000000000000000(%rip), %xmm0
.LBB53_36:
	movsd	%xmm0, 48(%rsp)
	xorl	%eax, %eax
	movq	%rax, 40(%rsp)
	cmpb	$0, 40(%rsp)
	je	.LBB53_40
.LBB53_39:
	movq	48(%rsp), %rax
	movq	%rax, 8(%rcx)
	movq	$-1, (%rcx)
	jmp	.LBB53_43
.LBB53_44:
	divsd	%xmm1, %xmm0
	testb	%r8b, %r8b
	je	.LBB53_35
	jmp	.LBB53_36
.LBB53_45:
	movq	$13, 56(%rsp)
.LBB53_46:
	leaq	56(%rsp), %rax
	movq	%rcx, %rsi
	movq	%rdx, %rcx
	movq	%rax, %rdx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rsi, %rcx
	movq	%rax, 48(%rsp)
	movq	$1, 40(%rsp)
	cmpb	$0, 40(%rsp)
	jne	.LBB53_39
.LBB53_40:
	movq	48(%rsp), %rax
.LBB53_41:
	xorl	%edx, %edx
.LBB53_42:
	movq	%rdx, (%rcx)
	movq	%rax, 8(%rcx)
.LBB53_43:
	.seh_startepilogue
	addq	$80, %rsp
	popq	%rbx
	popq	%rbp
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r14
	popq	%r15
	.seh_endepilogue
	retq
.LBB53_47:
	movq	$14, 56(%rsp)
	leaq	56(%rsp), %rax
	movq	%rcx, %rsi
	movq	%rdx, %rcx
	movq	%rax, %rdx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rsi, %rcx
	movq	%rax, 48(%rsp)
	movl	$1, %eax
	movq	%rax, 40(%rsp)
	cmpb	$0, 40(%rsp)
	je	.LBB53_40
	jmp	.LBB53_39
.Lfunc_end53:
	.seh_endproc

	.def	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE13parse_integerCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE13parse_integerCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,55
	.p2align	4
_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE13parse_integerCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin54:
.seh_proc _RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE13parse_integerCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$64, %rsp
	.seh_stackalloc 64
	.seh_endprologue
	movq	32(%rdx), %rdi
	movq	40(%rdx), %rax
	cmpq	%rdi, %rax
	jae	.LBB54_5
	movq	24(%rdx), %r10
	movzbl	(%r10,%rax), %r9d
	leaq	1(%rax), %r11
	movq	%r11, 40(%rdx)
	cmpb	$48, %r9b
	jne	.LBB54_9
	cmpq	%rdi, %r11
	jae	.LBB54_4
	movzbl	(%r10,%r11), %eax
	addb	$-48, %al
	cmpb	$10, %al
	jae	.LBB54_4
	movq	$13, 40(%rsp)
	leaq	40(%rsp), %rax
	movq	%rcx, %rsi
	movq	%rdx, %rcx
	movq	%rax, %rdx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE10peek_errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB54_7
.LBB54_9:
	leal	-49(%r9), %esi
	cmpb	$9, %sil
	jae	.LBB54_10
	addb	$-48, %r9b
	movzbl	%r9b, %r9d
	cmpq	%rdi, %r11
	jae	.LBB54_18
	movabsq	$1844674407370955160, %r11
	movl	$1, %esi
	subq	%rdi, %rsi
	addq	$2, %rax
	leaq	1(%r11), %rdi
	jmp	.LBB54_13
	.p2align	4
.LBB54_17:
	movzbl	%bl, %ebx
	movq	%rax, 40(%rdx)
	leaq	(%r9,%r9,4), %r9
	leaq	(%rbx,%r9,2), %r9
	leaq	(%rsi,%rax), %rbx
	incq	%rbx
	incq	%rax
	cmpq	$2, %rbx
	je	.LBB54_18
.LBB54_13:
	movzbl	-1(%r10,%rax), %ebx
	addb	$-48, %bl
	cmpb	$10, %bl
	jae	.LBB54_18
	cmpq	%r11, %r9
	jbe	.LBB54_17
	cmpq	%rdi, %r9
	jne	.LBB54_19
	cmpb	$5, %bl
	jbe	.LBB54_17
.LBB54_19:
	movq	%rcx, %rsi
	leaq	40(%rsp), %rcx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE18parse_long_integerB7_
	cmpl	$1, 40(%rsp)
	jne	.LBB54_23
	movq	48(%rsp), %rax
	movq	%rax, 8(%rsi)
	movq	$-1, %rax
	jmp	.LBB54_21
.LBB54_4:
	xorl	%r9d, %r9d
.LBB54_18:
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	jmp	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE12parse_numberCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB54_5:
	movq	$5, 40(%rsp)
.LBB54_6:
	leaq	40(%rsp), %rax
	movq	%rcx, %rsi
	movq	%rdx, %rcx
	movq	%rax, %rdx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB54_7:
	movq	%rax, 8(%rsi)
	movq	$-1, (%rsi)
.LBB54_22:
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.LBB54_23:
	movsd	48(%rsp), %xmm0
	movsd	%xmm0, 8(%rsi)
	xorl	%eax, %eax
.LBB54_21:
	movq	%rax, (%rsi)
	jmp	.LBB54_22
.LBB54_10:
	movq	$13, 40(%rsp)
	jmp	.LBB54_6
.Lfunc_end54:
	.seh_endproc

	.def	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE14parse_exponentCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE14parse_exponentCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,56
	.p2align	4
_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE14parse_exponentCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin55:
.seh_proc _RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE14parse_exponentCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$64, %rsp
	.seh_stackalloc 64
	.seh_endprologue
	movq	32(%rdx), %rsi
	movq	40(%rdx), %r11
	leaq	1(%r11), %r10
	movq	%r10, 40(%rdx)
	movb	$1, %al
	cmpq	%rsi, %r10
	jae	.LBB55_5
	movq	24(%rdx), %rax
	movzbl	(%rax,%r10), %edi
	movb	$1, %al
	cmpl	$43, %edi
	je	.LBB55_4
	cmpl	$45, %edi
	jne	.LBB55_5
	xorl	%eax, %eax
.LBB55_4:
	addq	$2, %r11
	movq	%r11, 40(%rdx)
	movq	%r11, %r10
.LBB55_5:
	cmpq	%rsi, %r10
	jae	.LBB55_8
	movq	24(%rdx), %rdi
	movzbl	(%rdi,%r10), %r11d
	leaq	1(%r10), %rbx
	movq	%rbx, 40(%rdx)
	addb	$-48, %r11b
	cmpb	$10, %r11b
	jae	.LBB55_7
	movzbl	%r11b, %r11d
	cmpq	%rsi, %rbx
	jae	.LBB55_13
	movl	$1, %ebx
	subq	%rsi, %rbx
	addq	$2, %r10
	jmp	.LBB55_12
	.p2align	4
.LBB55_33:
	movzbl	%sil, %esi
	leal	(%r11,%r11,4), %r11d
	leal	(%rsi,%r11,2), %r11d
	leaq	(%rbx,%r10), %rsi
	incq	%rsi
	incq	%r10
	cmpq	$2, %rsi
	je	.LBB55_13
.LBB55_12:
	movzbl	-1(%rdi,%r10), %esi
	addb	$-48, %sil
	cmpb	$10, %sil
	jae	.LBB55_13
	movq	%r10, 40(%rdx)
	cmpl	$214748363, %r11d
	jle	.LBB55_33
	cmpl	$214748364, %r11d
	jne	.LBB55_34
	cmpb	$7, %sil
	jbe	.LBB55_33
.LBB55_34:
	testq	%r9, %r9
	sete	%r9b
	movb	%al, 32(%rsp)
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE23parse_exponent_overflowB7_
	jmp	.LBB55_29
.LBB55_13:
	movl	128(%rsp), %r10d
	testb	%al, %al
	je	.LBB55_14
	leal	(%r10,%r11), %eax
	sarl	$31, %eax
	addl	$-2147483648, %eax
	addl	%r11d, %r10d
	jmp	.LBB55_16
.LBB55_14:
	xorl	%eax, %eax
	cmpl	%r11d, %r10d
	setns	%al
	addl	$2147483647, %eax
	subl	%r11d, %r10d
.LBB55_16:
	cmovol	%eax, %r10d
	movq	%r9, %xmm1
	punpckldq	__xmm@00000000000000004530000043300000(%rip), %xmm1
	subpd	__xmm@45300000000000004330000000000000(%rip), %xmm1
	movapd	%xmm1, %xmm0
	unpckhpd	%xmm1, %xmm0
	addsd	%xmm1, %xmm0
	movl	%r10d, %eax
	negl	%eax
	cmovsl	%r10d, %eax
	cmpl	$309, %eax
	jb	.LBB55_21
	xorpd	%xmm1, %xmm1
	movsd	__real@7fe1ccf385ebc8a0(%rip), %xmm2
	.p2align	4
.LBB55_18:
	ucomisd	%xmm1, %xmm0
	jne	.LBB55_19
	jnp	.LBB55_23
.LBB55_19:
	testl	%r10d, %r10d
	jns	.LBB55_27
	divsd	%xmm2, %xmm0
	addl	$308, %r10d
	movl	%r10d, %eax
	negl	%eax
	cmovsl	%r10d, %eax
	cmpl	$309, %eax
	jae	.LBB55_18
.LBB55_21:
	movl	%eax, %eax
	leaq	_RNvNtCsfZWMcdrZHWk_10serde_json2de5POW10(%rip), %r9
	movsd	(%r9,%rax,8), %xmm1
	testl	%r10d, %r10d
	js	.LBB55_22
	mulsd	%xmm1, %xmm0
	movq	%xmm0, %rax
	movabsq	$9223372036854775807, %r9
	andq	%rax, %r9
	movabsq	$9218868437227405312, %rax
	cmpq	%rax, %r9
	je	.LBB55_27
.LBB55_23:
	testb	%r8b, %r8b
	jne	.LBB55_25
.LBB55_24:
	xorpd	__xmm@80000000000000008000000000000000(%rip), %xmm0
.LBB55_25:
	movsd	%xmm0, 8(%rcx)
	xorl	%eax, %eax
.LBB55_28:
	movq	%rax, (%rcx)
.LBB55_29:
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.LBB55_22:
	divsd	%xmm1, %xmm0
	testb	%r8b, %r8b
	je	.LBB55_24
	jmp	.LBB55_25
.LBB55_8:
	movq	$5, 40(%rsp)
.LBB55_9:
	leaq	40(%rsp), %rax
	movq	%rcx, %rsi
	movq	%rdx, %rcx
	movq	%rax, %rdx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rax, 8(%rsi)
	movq	$1, (%rsi)
	jmp	.LBB55_29
.LBB55_27:
	movq	$14, 40(%rsp)
	leaq	40(%rsp), %rax
	movq	%rcx, %rsi
	movq	%rdx, %rcx
	movq	%rax, %rdx
	callq	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rsi, %rcx
	movq	%rax, 8(%rsi)
	movl	$1, %eax
	jmp	.LBB55_28
.LBB55_7:
	movq	$13, 40(%rsp)
	jmp	.LBB55_9
.Lfunc_end55:
	.seh_endproc

	.def	_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text$unlikely,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,57
	.p2align	4
_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin56:
.seh_proc _RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
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
	subq	$64, %rsp
	.seh_stackalloc 64
	leaq	64(%rsp), %rbp
	.seh_setframe %rbp, 64
	.seh_endprologue
	movq	$-2, -8(%rbp)
	movq	%rdx, -24(%rbp)
	movq	40(%rcx), %rsi
	movq	32(%rcx), %rdi
	cmpq	%rdi, %rsi
	ja	.LBB56_1
	movq	24(%rcx), %rbx
	leaq	(%rbx,%rsi), %r8
	movq	_RNvNvNtNtNtCsloPA8o21a2b_6memchr4arch6x86_646memchr11memrchr_raw2FN(%rip), %rax
	movb	$1, -9(%rbp)
.Ltmp248:
	movb	$10, %cl
	movq	%rbx, %rdx
	callq	*%rax
	nop
.Ltmp249:
	cmpq	$1, %rax
	jne	.LBB56_5
	subq	%rbx, %rdx
	leaq	1(%rdx), %r14
	cmpq	%rdi, %rdx
	jb	.LBB56_6
	movb	$1, -9(%rbp)
.Ltmp250:
	leaq	anon.7e59a8bc2ee43327c2a223b709be5804.51.llvm.84982629146409838(%rip), %r9
	xorl	%ecx, %ecx
	movq	%r14, %rdx
	movq	%rdi, %r8
	callq	_RNvNtNtCsd9eshbB3hfz_4core5slice5index16slice_index_fail
	nop
.Ltmp251:
	jmp	.LBB56_2
.LBB56_5:
	xorl	%r14d, %r14d
.LBB56_6:
	leaq	(%rbx,%r14), %r8
	movq	_RNvNvNtNtNtCsloPA8o21a2b_6memchr4arch6x86_646memchr9count_raw2FN(%rip), %rax
	movb	$1, -9(%rbp)
.Ltmp252:
	movb	$10, %cl
	movq	%rbx, %rdx
	callq	*%rax
	nop
.Ltmp253:
	incq	%rax
	subq	%r14, %rsi
	movb	$0, -9(%rbp)
.Ltmp254:
	movq	-24(%rbp), %rcx
	movq	%rax, %rdx
	movq	%rsi, %r8
	callq	_RNvMs0_NtCsfZWMcdrZHWk_10serde_json5errorNtB5_5Error6syntax
	nop
.Ltmp255:
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.LBB56_1:
	movb	$1, -9(%rbp)
.Ltmp256:
	leaq	anon.7e59a8bc2ee43327c2a223b709be5804.52.llvm.84982629146409838(%rip), %r9
	xorl	%ecx, %ecx
	movq	%rsi, %rdx
	movq	%rdi, %r8
	callq	_RNvNtNtCsd9eshbB3hfz_4core5slice5index16slice_index_fail
	nop
.Ltmp257:
.LBB56_2:
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text$unlikely,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,57
	.seh_endproc
	.def	"?dtor$11@?0?_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$11@?0?_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$11@?0?_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB56_11:
	movq	%rdx, 16(%rsp)
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
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	64(%rdx), %rbp
	.seh_endprologue
	cmpb	$0, -9(%rbp)
	jne	.LBB56_12
.LBB56_13:
	.seh_startepilogue
	addq	$32, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%rbp
	.seh_endepilogue
	retq
.LBB56_12:
	movq	-24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error9ErrorCodeECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB56_13
.Lfunc_end56:
	.seh_handlerdata
	.section	.text$unlikely,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,57
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,24
	.p2align	2, 0x0
$cppxdata$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	56
	.long	0
	.long	1
$stateUnwindMap$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$11@?0?_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin56@IMGREL
	.long	-1
	.long	.Ltmp248@IMGREL
	.long	0
	.long	.Ltmp257@IMGREL
	.long	-1
	.section	.text$unlikely,"xr",one_only,_RNvMs3_NtCsfZWMcdrZHWk_10serde_json2deINtB5_12DeserializerNtNtB7_4read7StrReadE5errorCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,57

	.def	_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.p2align	4
_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin57:
.seh_proc _RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$456, %rsp
	.seh_stackalloc 456
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 320(%rbp)
	movq	%r9, 312(%rbp)
	movq	%rdx, %r12
	movq	(%rdx), %rax
	testq	%rax, %rax
	je	.LBB57_11
	movq	%rcx, 248(%rbp)
	movq	%r12, 208(%rbp)
	movq	8(%r12), %rdi
	movq	8(%r8), %rcx
	movq	%rcx, 304(%rbp)
	movq	%r8, 296(%rbp)
	movq	16(%r8), %r14
	movq	%rax, 32(%rbp)
	movq	%rax, %rsi
.LBB57_2:
	leaq	360(%rsi), %rcx
	movq	%rsi, 288(%rbp)
	movzwl	626(%rsi), %eax
	movq	%rax, %rbx
	leal	(,%rax,8), %eax
	leaq	(%rax,%rax,2), %rsi
	movq	$-1, %r12
	movq	%rcx, 256(%rbp)
	.p2align	4
.LBB57_3:
	testq	%rsi, %rsi
	je	.LBB57_6
	leaq	24(%rcx), %r13
	movq	8(%rcx), %rdx
	movq	16(%rcx), %r8
	movq	%r14, %r15
	subq	%r8, %r15
	cmovbq	%r14, %r8
	movq	304(%rbp), %rcx
	callq	memcmp
	cltq
	testl	%eax, %eax
	cmovneq	%rax, %r15
	testq	%r15, %r15
	sets	%cl
	setg	%al
	subb	%cl, %al
	incq	%r12
	addq	$-24, %rsi
	movq	%r13, %rcx
	cmpb	$1, %al
	je	.LBB57_3
	movzbl	%al, %eax
	testl	%eax, %eax
	jne	.LBB57_7
	jmp	.LBB57_9
	.p2align	4
.LBB57_6:
	movq	%rbx, %r12
.LBB57_7:
	movq	288(%rbp), %rsi
	subq	$1, %rdi
	jb	.LBB57_19
	movq	632(%rsi,%r12,8), %rsi
	jmp	.LBB57_2
.LBB57_9:
	movq	296(%rbp), %rax
	movq	(%rax), %rdx
	testq	%rdx, %rdx
	movq	288(%rbp), %rsi
	je	.LBB57_15
	movl	$1, %r8d
	movq	304(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	248(%rbp), %rcx
	jmp	.LBB57_17
.LBB57_11:
	movq	(%r8), %rsi
	movq	8(%r8), %r15
	cmpq	$-1, %rsi
	je	.LBB57_16
	movq	%rax, 32(%rbp)
	movq	%rcx, %r14
	movq	16(%r8), %rdi
	movq	312(%rbp), %rbx
	movups	(%rbx), %xmm0
	movups	16(%rbx), %xmm1
	movaps	%xmm1, -48(%rbp)
	movaps	%xmm0, -64(%rbp)
	movl	$632, %ecx
	movl	$8, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB57_86
	movq	$0, 352(%rax)
	movq	%rax, (%r12)
	movq	$0, 8(%r12)
	movw	$1, 626(%rax)
	movq	%rsi, 360(%rax)
	movq	%r15, 368(%rax)
	movq	%rdi, 376(%rax)
	movups	(%rbx), %xmm0
	movups	16(%rbx), %xmm1
	movups	%xmm0, (%rax)
	movups	%xmm1, 16(%rax)
.LBB57_14:
	incq	16(%r12)
	movb	$-1, (%r14)
	jmp	.LBB57_18
.LBB57_15:
	movq	248(%rbp), %rcx
	jmp	.LBB57_17
.LBB57_16:
	movq	%r15, %rsi
.LBB57_17:
	movq	312(%rbp), %rax
	shlq	$5, %r12
	movups	(%rsi,%r12), %xmm0
	movups	16(%rsi,%r12), %xmm1
	movaps	%xmm1, 64(%rbp)
	movaps	%xmm0, 48(%rbp)
	movups	(%rax), %xmm0
	movups	16(%rax), %xmm1
	movups	%xmm1, 16(%rsi,%r12)
	movups	%xmm0, (%rsi,%r12)
	movaps	48(%rbp), %xmm0
	movaps	64(%rbp), %xmm1
	movups	%xmm1, 16(%rcx)
	movups	%xmm0, (%rcx)
.LBB57_18:
	.seh_startepilogue
	addq	$456, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB57_19:
	movq	296(%rbp), %rax
	movq	(%rax), %rdi
	cmpq	$-1, %rdi
	je	.LBB57_23
	movq	312(%rbp), %rax
	movups	(%rax), %xmm0
	movups	16(%rax), %xmm1
	movaps	%xmm1, -48(%rbp)
	movaps	%xmm0, -64(%rbp)
	cmpw	$11, %bx
	jae	.LBB57_24
	leaq	(%r12,%r12,2), %rax
	movq	%rbx, %r15
	movq	256(%rbp), %rcx
	leaq	(%rcx,%rax,8), %rbx
	cmpq	%r12, %r15
	jbe	.LBB57_26
	leaq	1(%r12), %r13
	leaq	(,%r13,2), %rax
	addq	%r13, %rax
	leaq	(%rcx,%rax,8), %rcx
	movl	%r15d, %eax
	subl	%r12d, %eax
	movq	%rax, 288(%rbp)
	leal	(,%rax,8), %eax
	leaq	(%rax,%rax,2), %r8
	movq	%rbx, %rdx
	callq	memmove
	movq	%rdi, (%rbx)
	movq	304(%rbp), %rax
	movq	%rax, 8(%rbx)
	movq	%r14, 16(%rbx)
	movq	312(%rbp), %rax
	movups	(%rax), %xmm0
	movups	16(%rax), %xmm1
	movaps	%xmm1, 64(%rbp)
	movaps	%xmm0, 48(%rbp)
	movq	%r12, %rdx
	shlq	$5, %rdx
	addq	%rsi, %rdx
	shlq	$5, %r13
	addq	%rsi, %r13
	movq	288(%rbp), %r8
	shll	$5, %r8d
	movq	%r13, %rcx
	callq	memmove
	jmp	.LBB57_27
.LBB57_23:
	movq	304(%rbp), %rsi
	movq	208(%rbp), %r12
	movq	248(%rbp), %rcx
	jmp	.LBB57_17
.LBB57_24:
	cmpq	$5, %r12
	movq	256(%rbp), %rbx
	movq	%rdi, 224(%rbp)
	jae	.LBB57_28
	movl	$4, %edi
	movl	$0, 264(%rbp)
	jmp	.LBB57_33
.LBB57_26:
	movq	%rdi, (%rbx)
	movq	304(%rbp), %rax
	movq	%rax, 8(%rbx)
	movq	%r14, 16(%rbx)
	movq	312(%rbp), %rax
	movups	(%rax), %xmm0
	movups	16(%rax), %xmm1
	movaps	%xmm1, 64(%rbp)
	movaps	%xmm0, 48(%rbp)
.LBB57_27:
	incl	%r15d
	shlq	$5, %r12
	movaps	48(%rbp), %xmm0
	movaps	64(%rbp), %xmm1
	movups	%xmm1, 16(%rsi,%r12)
	movups	%xmm0, (%rsi,%r12)
	movw	%r15w, 626(%rsi)
	movq	248(%rbp), %r14
	jmp	.LBB57_68
.LBB57_28:
	je	.LBB57_31
	cmpq	$6, %r12
	jne	.LBB57_32
	movl	$5, %edi
	movb	$1, %al
	movl	%eax, 264(%rbp)
	xorl	%r12d, %r12d
	jmp	.LBB57_33
.LBB57_31:
	movl	$0, 264(%rbp)
	movq	%r12, %rdi
	jmp	.LBB57_33
.LBB57_32:
	addq	$-7, %r12
	movl	$6, %edi
	movb	$1, %al
	movl	%eax, 264(%rbp)
.LBB57_33:
	movl	$632, %ecx
	movl	$8, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB57_92
	movq	%rax, %r13
	movq	$0, 352(%rax)
	movzwl	626(%rsi), %eax
	movq	%rdi, %r10
	notq	%r10
	addq	%rax, %r10
	movw	%r10w, 626(%r13)
	leaq	(%rdi,%rdi,2), %rax
	movq	(%rbx,%rax,8), %rcx
	movq	%rcx, 296(%rbp)
	movq	8(%rbx,%rax,8), %rcx
	movq	%rcx, 272(%rbp)
	movq	16(%rbx,%rax,8), %rax
	movq	%rax, 216(%rbp)
	movq	%rsi, %r15
	movq	%rdi, %rsi
	shlq	$5, %rsi
	movups	(%r15,%rsi), %xmm0
	movups	16(%r15,%rsi), %xmm1
	movaps	%xmm1, 64(%rbp)
	movaps	%xmm0, 48(%rbp)
	cmpq	$12, %r10
	movq	%r13, 280(%rbp)
	jae	.LBB57_91
	movq	%rbx, %rcx
	addq	%r15, %rsi
	leaq	1(%rdi), %rbx
	leaq	(%rbx,%rbx,2), %rax
	leaq	(%rcx,%rax,8), %rdx
	leaq	360(%r13), %rcx
	leaq	(,%r10,8), %rax
	leaq	(%rax,%rax,2), %r8
	movq	%rdi, 256(%rbp)
	movq	%r10, %rdi
	callq	memcpy
	shlq	$5, %rbx
	addq	%r15, %rbx
	shlq	$5, %rdi
	movq	%r13, %rcx
	movq	%rbx, %rdx
	movq	%rdi, %r8
	callq	memcpy
	movq	256(%rbp), %rax
	movw	%ax, 626(%r15)
	movups	(%rsi), %xmm0
	movups	16(%rsi), %xmm1
	movaps	%xmm0, -32(%rbp)
	movaps	%xmm1, -16(%rbp)
	cmpb	$0, 264(%rbp)
	movq	%r13, %rsi
	cmoveq	%r15, %rsi
	movzwl	626(%rsi), %edi
	movzwl	%di, %ebx
	leaq	(%r12,%r12,2), %rax
	leaq	(%rsi,%rax,8), %r15
	addq	$360, %r15
	cmpq	%r12, %rbx
	jbe	.LBB57_37
	leaq	360(%rsi), %rax
	leaq	1(%r12), %rcx
	leaq	(%rcx,%rcx,2), %rcx
	leaq	(%rax,%rcx,8), %rcx
	subl	%r12d, %ebx
	leal	(,%rbx,8), %eax
	leaq	(%rax,%rax,2), %r8
	movq	%r15, %rdx
	callq	memmove
	movq	224(%rbp), %r13
	movq	%r13, (%r15)
	movq	304(%rbp), %rax
	movq	%rax, 8(%r15)
	movq	%r14, 16(%r15)
	movq	312(%rbp), %rax
	movups	(%rax), %xmm0
	movups	16(%rax), %xmm1
	movaps	%xmm1, 64(%rbp)
	movaps	%xmm0, 48(%rbp)
	movq	%r12, %rdx
	shlq	$5, %rdx
	addq	%rsi, %rdx
	leaq	1(%r12), %rcx
	shlq	$5, %rcx
	addq	%rsi, %rcx
	shll	$5, %ebx
	movq	%rbx, %r8
	callq	memmove
	jmp	.LBB57_38
.LBB57_37:
	movq	224(%rbp), %r13
	movq	%r13, (%r15)
	movq	304(%rbp), %rax
	movq	%rax, 8(%r15)
	movq	%r14, 16(%r15)
	movq	312(%rbp), %rax
	movups	(%rax), %xmm0
	movups	16(%rax), %xmm1
	movaps	%xmm1, 64(%rbp)
	movaps	%xmm0, 48(%rbp)
.LBB57_38:
	incl	%edi
	shlq	$5, %r12
	movaps	48(%rbp), %xmm0
	movaps	64(%rbp), %xmm1
	movups	%xmm1, 16(%rsi,%r12)
	movups	%xmm0, (%rsi,%r12)
	movw	%di, 626(%rsi)
	movaps	-32(%rbp), %xmm0
	movaps	-16(%rbp), %xmm1
	movaps	%xmm1, 16(%rbp)
	movaps	%xmm0, (%rbp)
	movq	296(%rbp), %r15
	cmpq	$-1, %r15
	movq	248(%rbp), %r14
	movq	280(%rbp), %rdi
	movq	272(%rbp), %rbx
	je	.LBB57_68
	movaps	(%rbp), %xmm0
	movaps	16(%rbp), %xmm1
	movaps	%xmm1, 176(%rbp)
	movaps	%xmm0, 160(%rbp)
	movq	288(%rbp), %r13
	movq	352(%r13), %rax
	testq	%rax, %rax
	movq	208(%rbp), %r12
	je	.LBB57_69
	movq	$0, 264(%rbp)
	movq	216(%rbp), %r11
	movq	%rbx, %rdx
	movq	%rdi, %r10
	movq	%r15, %r9
.LBB57_41:
	movzwl	624(%r13), %ecx
	movzwl	%cx, %r8d
	movzwl	626(%rax), %esi
	cmpq	$11, %rsi
	movq	%rdx, 200(%rbp)
	movq	%r10, 280(%rbp)
	jb	.LBB57_75
	cmpw	$5, %cx
	movq	%rax, 288(%rbp)
	movq	%r9, 144(%rbp)
	movq	%r11, 40(%rbp)
	jae	.LBB57_44
	movq	%r8, 312(%rbp)
	movl	$4, %r15d
	xorl	%edi, %edi
	jmp	.LBB57_49
.LBB57_44:
	je	.LBB57_47
	cmpl	$6, %r8d
	jne	.LBB57_48
	movb	$1, %dil
	movl	$5, %r15d
	movq	$0, 312(%rbp)
	jmp	.LBB57_49
.LBB57_47:
	xorl	%edi, %edi
	movq	%r8, 312(%rbp)
	movq	%r8, %r15
	jmp	.LBB57_49
.LBB57_48:
	addq	$-7, %r8
	movq	%r8, 312(%rbp)
	movb	$1, %dil
	movl	$6, %r15d
.LBB57_49:
	movq	%rsi, 152(%rbp)
	movl	$728, %ecx
	movl	$8, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB57_89
	movq	%rax, %rbx
	movl	%edi, 140(%rbp)
	movq	%rax, %r12
	movq	$0, 352(%rbx)
	movq	288(%rbp), %r13
	movzwl	626(%r13), %eax
	movq	%r15, %r14
	notq	%r14
	addq	%rax, %r14
	movw	%r14w, 626(%rbx)
	leaq	(%r15,%r15,2), %rax
	movq	360(%r13,%rax,8), %rcx
	movq	%rcx, 296(%rbp)
	movq	368(%r13,%rax,8), %rcx
	movq	%rcx, 272(%rbp)
	movq	376(%r13,%rax,8), %rsi
	movl	%r15d, %edi
	shll	$5, %edi
	movups	(%r13,%rdi), %xmm0
	movups	16(%r13,%rdi), %xmm1
	movaps	%xmm1, -16(%rbp)
	movaps	%xmm0, -32(%rbp)
	cmpq	$12, %r14
	jae	.LBB57_87
	leaq	360(%r13), %rax
	addq	%r13, %rdi
	movq	%r15, 256(%rbp)
	movq	256(%rbp), %rcx
	leaq	1(%rcx), %r15
	leaq	(%r15,%r15,2), %rcx
	leaq	(%rax,%rcx,8), %rdx
	leaq	360(%rbx), %rcx
	leaq	(,%r14,8), %rax
	leaq	(%rax,%rax,2), %r8
	callq	memcpy
	shll	$5, %r15d
	addq	%r13, %r15
	shlq	$5, %r14
	movq	%r12, %rcx
	movq	%r15, %rdx
	movq	256(%rbp), %r15
	movq	%r14, %r8
	callq	memcpy
	movw	%r15w, 626(%r13)
	movups	(%rdi), %xmm0
	movups	16(%rdi), %xmm1
	leaq	72(%rbp), %rax
	movups	%xmm0, (%rax)
	movups	%xmm1, 16(%rax)
	movq	296(%rbp), %rax
	movq	%rax, 48(%rbp)
	movq	272(%rbp), %rax
	movq	%rax, 56(%rbp)
	movq	%rsi, 216(%rbp)
	movq	%rsi, 64(%rbp)
	movzwl	626(%r12), %edi
	leaq	1(%rdi), %r8
	cmpq	$11, %rdi
	ja	.LBB57_88
	movzwl	152(%rbp), %eax
	subl	%r15d, %eax
	cmpl	%r8d, %eax
	movq	264(%rbp), %rax
	movl	140(%rbp), %r14d
	jne	.LBB57_90
	incq	%rax
	movq	%rax, 264(%rbp)
	leaq	632(%rbx), %rcx
	movq	288(%rbp), %rsi
	leaq	(%rsi,%r15,8), %rdx
	addq	$640, %rdx
	shll	$3, %r8d
	callq	memcpy
	xorl	%eax, %eax
	.p2align	4
.LBB57_54:
	movq	%rax, %rcx
	cmpq	%rdi, %rax
	adcq	$0, %rax
	movq	632(%r12,%rcx,8), %rdx
	movq	%r12, 352(%rdx)
	movw	%cx, 624(%rdx)
	cmpq	%rdi, %rcx
	jae	.LBB57_56
	cmpq	%rdi, %rax
	jbe	.LBB57_54
.LBB57_56:
	movq	%r12, %r13
	leaq	72(%rbp), %rax
	movups	(%rax), %xmm0
	movups	16(%rax), %xmm1
	movaps	%xmm1, 16(%rbp)
	movaps	%xmm0, (%rbp)
	testb	%r14b, %r14b
	cmoveq	%rsi, %rbx
	movzwl	626(%rbx), %esi
	movq	312(%rbp), %rdx
	leaq	1(%rdx), %r14
	movq	%rsi, %r10
	leaq	(%rdx,%rdx,2), %rax
	leaq	(%rbx,%rax,8), %r15
	addq	$360, %r15
	subq	%rdx, %r10
	jbe	.LBB57_58
	leaq	360(%rbx), %rax
	leaq	(%r14,%r14,2), %rcx
	leaq	(%rax,%rcx,8), %rcx
	movl	%esi, %r12d
	subl	%edx, %r12d
	leal	(,%r12,8), %eax
	movq	%rax, 152(%rbp)
	leaq	(%rax,%rax,2), %r8
	movq	%r15, %rdx
	movq	%r10, 256(%rbp)
	callq	memmove
	movq	144(%rbp), %rax
	movq	%rax, (%r15)
	movq	200(%rbp), %rax
	movq	%rax, 8(%r15)
	movq	40(%rbp), %rax
	movq	%rax, 16(%r15)
	movq	312(%rbp), %rdi
	shlq	$5, %rdi
	leaq	(%rbx,%rdi), %rdx
	movq	%r14, %rcx
	shlq	$5, %rcx
	addq	%rbx, %rcx
	shll	$5, %r12d
	movq	%r12, %r8
	callq	memmove
	movaps	160(%rbp), %xmm0
	movaps	176(%rbp), %xmm1
	movups	%xmm0, (%rbx,%rdi)
	movups	%xmm1, 16(%rbx,%rdi)
	leaq	632(%rbx,%r14,8), %rdx
	movq	312(%rbp), %rax
	leaq	(%rbx,%rax,8), %rcx
	addq	$648, %rcx
	movq	152(%rbp), %r8
	callq	memmove
	movq	256(%rbp), %r10
	movq	312(%rbp), %rdx
	jmp	.LBB57_59
.LBB57_58:
	movq	144(%rbp), %rax
	movq	%rax, (%r15)
	movq	200(%rbp), %rax
	movq	%rax, 8(%r15)
	movq	40(%rbp), %rax
	movq	%rax, 16(%r15)
	movq	%rdx, %rax
	shlq	$5, %rax
	movaps	160(%rbp), %xmm0
	movaps	176(%rbp), %xmm1
	movups	%xmm1, 16(%rbx,%rax)
	movups	%xmm0, (%rbx,%rax)
.LBB57_59:
	movq	208(%rbp), %r12
	movq	280(%rbp), %r8
	leal	1(%rsi), %ecx
	leaq	2(%rsi), %rax
	movq	%r8, 640(%rbx,%rdx,8)
	movw	%cx, 626(%rbx)
	cmpl	%eax, %r14d
	movq	%r13, %rdi
	movq	296(%rbp), %r15
	jae	.LBB57_66
	subl	%edx, %esi
	incl	%esi
	andl	$3, %esi
	je	.LBB57_64
	leaq	(%rbx,%rdx,8), %rdx
	addq	$640, %rdx
	xorl	%ecx, %ecx
	.p2align	4
.LBB57_62:
	movq	(%rdx,%rcx,8), %r8
	movq	%rbx, 352(%r8)
	leal	(%r14,%rcx), %r9d
	movw	%r9w, 624(%r8)
	incq	%rcx
	cmpq	%rcx, %rsi
	jne	.LBB57_62
	addq	%rcx, %r14
.LBB57_64:
	cmpl	$3, %r10d
	jb	.LBB57_66
	.p2align	4
.LBB57_65:
	movq	632(%rbx,%r14,8), %rcx
	movq	%rbx, 352(%rcx)
	movw	%r14w, 624(%rcx)
	movq	640(%rbx,%r14,8), %rcx
	movq	%rbx, 352(%rcx)
	leal	1(%r14), %edx
	movw	%dx, 624(%rcx)
	movq	648(%rbx,%r14,8), %rcx
	movq	%rbx, 352(%rcx)
	leal	2(%r14), %edx
	movw	%dx, 624(%rcx)
	movq	656(%rbx,%r14,8), %rcx
	movq	%rbx, 352(%rcx)
	leal	3(%r14), %edx
	movw	%dx, 624(%rcx)
	addq	$4, %r14
	cmpq	%rax, %r14
	jne	.LBB57_65
.LBB57_66:
	movaps	(%rbp), %xmm0
	movaps	16(%rbp), %xmm1
	movaps	%xmm1, -80(%rbp)
	movaps	%xmm0, -96(%rbp)
	cmpq	$-1, %r15
	movq	248(%rbp), %r14
	je	.LBB57_14
	movaps	-96(%rbp), %xmm0
	movaps	-80(%rbp), %xmm1
	movaps	%xmm1, 176(%rbp)
	movaps	%xmm0, 160(%rbp)
	movq	288(%rbp), %r13
	movq	352(%r13), %rax
	movq	216(%rbp), %rcx
	movq	%rcx, %r11
	movq	272(%rbp), %rbx
	movq	%rbx, %rdx
	movq	%rdi, %r10
	movq	%r15, %r9
	testq	%rax, %rax
	jne	.LBB57_41
	jmp	.LBB57_70
.LBB57_69:
	movq	$0, 264(%rbp)
	movq	216(%rbp), %rcx
.LBB57_70:
	movaps	160(%rbp), %xmm0
	movaps	176(%rbp), %xmm1
	movups	%xmm1, 88(%rbp)
	movups	%xmm0, 72(%rbp)
	movq	%r15, 48(%rbp)
	movq	%rbx, 56(%rbp)
	movq	%rcx, 216(%rbp)
	movq	%rcx, 64(%rbp)
	movq	%r13, 104(%rbp)
	movq	264(%rbp), %r13
	movq	%r13, 112(%rbp)
	movq	%rdi, 120(%rbp)
	movq	%r13, 128(%rbp)
	movq	(%r12), %rax
	movq	%rax, 288(%rbp)
	testq	%rax, %rax
	je	.LBB57_93
	movq	8(%r12), %rsi
	movl	$728, %ecx
	movl	$8, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB57_94
	movq	$0, 352(%rax)
	movw	$0, 626(%rax)
	movq	%rax, %rdx
	movq	288(%rbp), %rcx
	movq	%rcx, 632(%rax)
	movq	%rsi, %rax
	incq	%rax
	je	.LBB57_95
	movq	%rdx, 352(%rcx)
	movw	$0, 624(%rcx)
	movq	%rdx, (%r12)
	movq	%rax, 8(%r12)
	cmpq	%rsi, %r13
	jne	.LBB57_96
	movw	$1, 626(%rdx)
	movq	%r15, 360(%rdx)
	movq	%rbx, 368(%rdx)
	movq	216(%rbp), %rax
	movq	%rax, 376(%rdx)
	movaps	160(%rbp), %xmm0
	movaps	176(%rbp), %xmm1
	movups	%xmm0, (%rdx)
	movups	%xmm1, 16(%rdx)
	movq	%rdi, 640(%rdx)
	movq	%rdx, 352(%rdi)
	movw	$1, 624(%rdi)
	jmp	.LBB57_14
.LBB57_75:
	movq	%rax, %r13
	leal	1(%rsi), %edi
	leaq	1(%r8), %rbx
	leaq	(%r8,%r8,2), %rax
	leaq	360(,%rax,8), %r15
	addq	%r13, %r15
	cmpw	%si, %cx
	jae	.LBB57_77
	leaq	360(%r13), %rax
	leaq	(%rbx,%rbx,2), %rcx
	leaq	(%rax,%rcx,8), %rcx
	movq	%rsi, %r14
	subq	%r8, %r14
	leaq	(,%r14,8), %rax
	movq	%rax, 288(%rbp)
	movq	%r8, 312(%rbp)
	leaq	(%rax,%rax,2), %r8
	movq	%r15, %rdx
	movl	%edi, 304(%rbp)
	movq	%rsi, %rdi
	movq	%r9, %rsi
	movq	%r11, %r12
	callq	memmove
	movq	%rsi, (%r15)
	movq	%rdi, %rsi
	movq	200(%rbp), %rax
	movq	%rax, 8(%r15)
	movq	%r12, 16(%r15)
	movq	312(%rbp), %rdi
	shll	$5, %edi
	leaq	(%rdi,%r13), %rdx
	movl	%ebx, %ecx
	shll	$5, %ecx
	addq	%r13, %rcx
	shlq	$5, %r14
	movq	%r14, %r8
	movq	248(%rbp), %r14
	callq	memmove
	movaps	160(%rbp), %xmm0
	movaps	176(%rbp), %xmm1
	movups	%xmm0, (%r13,%rdi)
	movups	%xmm1, 16(%r13,%rdi)
	movl	304(%rbp), %edi
	leaq	632(%r13,%rbx,8), %rdx
	movq	312(%rbp), %rax
	leaq	648(,%rax,8), %rcx
	addq	%r13, %rcx
	movq	288(%rbp), %r8
	callq	memmove
	movq	280(%rbp), %r10
	movq	312(%rbp), %r8
	jmp	.LBB57_78
.LBB57_77:
	movq	%r9, (%r15)
	movq	%rdx, 8(%r15)
	movq	%r11, 16(%r15)
	movl	%r8d, %eax
	shll	$5, %eax
	movaps	160(%rbp), %xmm0
	movaps	176(%rbp), %xmm1
	movups	%xmm1, 16(%r13,%rax)
	movups	%xmm0, (%r13,%rax)
.LBB57_78:
	leaq	2(%rsi), %rax
	movq	%r10, 640(%r13,%r8,8)
	movw	%di, 626(%r13)
	cmpl	%eax, %ebx
	jae	.LBB57_68
	movq	%r13, %r11
	movl	%esi, %ecx
	subl	%r8d, %ecx
	incl	%ecx
	subq	%r8, %rsi
	andl	$3, %ecx
	je	.LBB57_83
	leaq	(%r11,%r8,8), %r8
	addq	$640, %r8
	xorl	%edx, %edx
.LBB57_81:
	movq	(%r8,%rdx,8), %r9
	movq	%r11, 352(%r9)
	leal	(%rbx,%rdx), %r10d
	movw	%r10w, 624(%r9)
	incq	%rdx
	cmpq	%rdx, %rcx
	jne	.LBB57_81
	addq	%rdx, %rbx
.LBB57_83:
	cmpl	$3, %esi
	jb	.LBB57_68
.LBB57_84:
	movq	632(%r11,%rbx,8), %rcx
	movq	%r11, 352(%rcx)
	movw	%bx, 624(%rcx)
	movq	640(%r11,%rbx,8), %rcx
	movq	%r11, 352(%rcx)
	leal	1(%rbx), %edx
	movw	%dx, 624(%rcx)
	movq	648(%r11,%rbx,8), %rcx
	movq	%r11, 352(%rcx)
	leal	2(%rbx), %edx
	movw	%dx, 624(%rcx)
	movq	656(%r11,%rbx,8), %rcx
	movq	%r11, 352(%rcx)
	leal	3(%rbx), %edx
	movw	%dx, 624(%rcx)
	addq	$4, %rbx
	cmpq	%rax, %rbx
	jne	.LBB57_84
.LBB57_68:
	movq	208(%rbp), %r12
	jmp	.LBB57_14
.LBB57_86:
	movq	%rsi, 240(%rbp)
	movq	%r15, 232(%rbp)
.Ltmp278:
	movl	$8, %ecx
	movl	$632, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc5alloc18handle_alloc_error
	nop
.Ltmp279:
	jmp	.LBB57_97
.LBB57_87:
.Ltmp260:
	movq	%r12, 280(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.163(%rip), %r9
	movl	$11, %r8d
	xorl	%ecx, %ecx
	movq	%r14, %rdx
	callq	_RNvNtNtCsd9eshbB3hfz_4core5slice5index16slice_index_fail
	nop
.Ltmp261:
	jmp	.LBB57_97
.LBB57_88:
.Ltmp262:
	movq	%r12, 280(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.6(%rip), %r9
	movq	%r8, %rdx
	movl	$12, %r8d
	xorl	%ecx, %ecx
	callq	_RNvNtNtCsd9eshbB3hfz_4core5slice5index16slice_index_fail
	nop
.Ltmp263:
	jmp	.LBB57_97
.LBB57_89:
.Ltmp266:
	movl	$8, %ecx
	movl	$728, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc5alloc18handle_alloc_error
	nop
.Ltmp267:
	jmp	.LBB57_97
.LBB57_90:
.Ltmp264:
	movq	%r12, 280(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.13(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.14(%rip), %r8
	movl	$40, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking5panic
	nop
.Ltmp265:
	jmp	.LBB57_97
.LBB57_91:
.Ltmp258:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.163(%rip), %r9
	movl	$11, %r8d
	xorl	%ecx, %ecx
	movq	%r10, %rdx
	callq	_RNvNtNtCsd9eshbB3hfz_4core5slice5index16slice_index_fail
	nop
.Ltmp259:
	jmp	.LBB57_97
.LBB57_92:
.Ltmp276:
	movl	$8, %ecx
	movl	$632, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc5alloc18handle_alloc_error
	nop
.Ltmp277:
	jmp	.LBB57_97
.LBB57_93:
.Ltmp274:
	movq	%rbx, 272(%rbp)
	movq	%r15, 296(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.32(%rip), %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13unwrap_failed
	nop
.Ltmp275:
	jmp	.LBB57_97
.LBB57_94:
.Ltmp272:
	movl	$8, %ecx
	movl	$728, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc5alloc18handle_alloc_error
	nop
.Ltmp273:
	jmp	.LBB57_97
.LBB57_95:
.Ltmp270:
	movq	%rdx, 304(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.5(%rip), %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13unwrap_failed
	nop
.Ltmp271:
	jmp	.LBB57_97
.LBB57_96:
.Ltmp268:
	movq	%rbx, 272(%rbp)
	movq	%r15, 296(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.164(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.165(%rip), %r8
	movl	$48, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking5panic
	nop
.Ltmp269:
.LBB57_97:
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$98@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$98@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$98@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_98:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	72(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	cmpq	$0, 296(%rbp)
	je	.LBB57_100
	movl	$1, %r8d
	movq	272(%rbp), %rcx
	movq	296(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB57_100:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$101@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$101@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$101@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_101:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$728, %edx
	movl	$8, %r8d
	movq	304(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$102@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$102@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$102@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_102:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	ud2
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$103@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$103@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$103@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_103:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$0, 288(%rbp)
	jne	.LBB57_107
	cmpq	$0, 296(%rbp)
	je	.LBB57_106
	movl	$1, %r8d
	movq	272(%rbp), %rcx
	movq	296(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB57_106:
	movq	224(%rbp), %rax
	movq	%rax, 240(%rbp)
	movq	304(%rbp), %rax
	movq	%rax, 232(%rbp)
	leaq	72(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
.LBB57_107:
	movq	224(%rbp), %rax
	movq	%rax, 240(%rbp)
	movq	304(%rbp), %rax
	movq	%rax, 232(%rbp)
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$108@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$108@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$108@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_108:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	48(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	cmpq	$0, 296(%rbp)
	je	.LBB57_110
	movl	$1, %r8d
	movq	272(%rbp), %rcx
	movq	296(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB57_110:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$111@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$111@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$111@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_111:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$632, %edx
	movl	$8, %r8d
	movq	280(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$112@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$112@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$112@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_112:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	224(%rbp), %rdi
	movq	%rdi, 240(%rbp)
	movq	304(%rbp), %rax
	movq	%rax, 232(%rbp)
	movq	312(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rdi, %rsi
	testq	%rdi, %rdi
	je	.LBB57_114
	movl	$1, %r8d
	movq	304(%rbp), %rcx
	movq	%rsi, %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB57_114:
	movq	%rsi, 240(%rbp)
	movq	304(%rbp), %rax
	movq	%rax, 232(%rbp)
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$115@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$115@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$115@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_115:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	-32(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	cmpq	$0, 296(%rbp)
	je	.LBB57_117
	movl	$1, %r8d
	movq	272(%rbp), %rcx
	movq	296(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB57_117:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$118@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$118@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$118@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_118:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	48(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueTNtNtCsdkPGTeTFVes_5alloc6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$119@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$119@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$119@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_119:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movl	$728, %edx
	movl	$8, %r8d
	movq	280(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$120@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$120@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$120@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_120:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	224(%rbp), %rsi
	movq	%rsi, 240(%rbp)
	movq	304(%rbp), %rax
	movq	%rax, 232(%rbp)
	leaq	160(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	144(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB57_122
	movl	$1, %r8d
	movq	200(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB57_122:
	movq	%rsi, 240(%rbp)
	movq	304(%rbp), %rax
	movq	%rax, 232(%rbp)
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.def	"?dtor$123@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$123@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$123@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB57_123:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$0, 32(%rbp)
	je	.LBB57_125
.LBB57_124:
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB57_125:
	movq	232(%rbp), %rsi
	movq	240(%rbp), %rdi
	leaq	-64(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5value5ValueECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testq	%rdi, %rdi
	je	.LBB57_124
	movl	$1, %r8d
	movq	%rsi, %rcx
	movq	%rdi, %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	jmp	.LBB57_124
.Lfunc_end57:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,25
	.p2align	2, 0x0
$cppxdata$_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	12
	.long	$stateUnwindMap$_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	13
	.long	$ip2state$_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	448
	.long	0
	.long	1
$stateUnwindMap$_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$123@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	0
	.long	"?dtor$120@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	1
	.long	"?dtor$119@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	2
	.long	"?dtor$118@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	2
	.long	"?dtor$115@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	0
	.long	"?dtor$103@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	5
	.long	"?dtor$98@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	0
	.long	"?dtor$112@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	7
	.long	"?dtor$111@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	8
	.long	"?dtor$108@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	-1
	.long	"?dtor$102@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
	.long	10
	.long	"?dtor$101@?0?_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin57@IMGREL
	.long	-1
	.long	.Ltmp278@IMGREL
	.long	0
	.long	.Ltmp260@IMGREL
	.long	4
	.long	.Ltmp262@IMGREL
	.long	3
	.long	.Ltmp266@IMGREL
	.long	1
	.long	.Ltmp264@IMGREL
	.long	3
	.long	.Ltmp258@IMGREL
	.long	9
	.long	.Ltmp276@IMGREL
	.long	7
	.long	.Ltmp274@IMGREL
	.long	5
	.long	.Ltmp272@IMGREL
	.long	10
	.long	.Ltmp270@IMGREL
	.long	11
	.long	.Ltmp268@IMGREL
	.long	6
	.long	.Ltmp269@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvMsi_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8BTreeMapNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE6insertCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,58

	.def	_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,59
	.p2align	4
_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin58:
.seh_proc _RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%r15
	.seh_pushreg %r15
	pushq	%r14
	.seh_pushreg %r14
	pushq	%r12
	.seh_pushreg %r12
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$48, %rsp
	.seh_stackalloc 48
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, -8(%rbp)
	movq	%rcx, %rsi
	movq	64(%rdx), %rax
	testq	%rax, %rax
	je	.LBB58_1
	decq	%rax
	movq	%rax, 64(%rdx)
	cmpl	$1, (%rdx)
	jne	.LBB58_49
	movq	8(%rdx), %rcx
	testq	%rcx, %rcx
	je	.LBB58_21
	movq	16(%rdx), %rax
	movq	24(%rdx), %rdi
	movzwl	626(%rcx), %r8d
	cmpq	%r8, %rdi
	jae	.LBB58_31
.LBB58_30:
	movq	%rax, %r14
	movq	%rcx, %rbx
	testq	%r14, %r14
	je	.LBB58_36
.LBB58_39:
	leaq	(%rbx,%rdi,8), %rax
	addq	$640, %rax
	movq	%r14, %r8
	andq	$7, %r8
	je	.LBB58_40
	xorl	%r10d, %r10d
	.p2align	4
.LBB58_42:
	movq	(%rax), %rcx
	leaq	632(%rcx), %rax
	incq	%r10
	cmpq	%r10, %r8
	jne	.LBB58_42
	movq	%r14, %r9
	subq	%r10, %r9
	xorl	%r8d, %r8d
	cmpq	$8, %r14
	jae	.LBB58_45
	jmp	.LBB58_37
.LBB58_1:
	movq	(%rdx), %r8
	movq	8(%rdx), %rcx
	movq	16(%rdx), %rdi
	movq	24(%rdx), %rax
	movq	$0, (%rdx)
	cmpq	$1, %r8
	jne	.LBB58_17
	testq	%rcx, %rcx
	je	.LBB58_4
	movq	%rdi, %rbx
	movq	%rcx, %rdi
.LBB58_13:
	movq	352(%rdi), %rax
	testq	%rax, %rax
	je	.LBB58_16
	movl	$632, %r14d
	movq	%rdi, %rcx
	.p2align	4
.LBB58_15:
	movq	%rax, %rdi
	testq	%rbx, %rbx
	leaq	1(%rbx), %rbx
	movl	$728, %edx
	cmoveq	%r14, %rdx
	movl	$8, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	352(%rdi), %rax
	movq	%rdi, %rcx
	testq	%rax, %rax
	jne	.LBB58_15
.LBB58_16:
	testq	%rbx, %rbx
	movl	$632, %eax
	movl	$728, %edx
	cmoveq	%rax, %rdx
	movl	$8, %r8d
	movq	%rdi, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB58_17:
	movq	$0, (%rsi)
	jmp	.LBB58_38
.LBB58_21:
	movq	16(%rdx), %rcx
	movq	24(%rdx), %rax
	testq	%rax, %rax
	je	.LBB58_28
	movq	%rax, %r8
	andq	$7, %r8
	je	.LBB58_23
	xorl	%r9d, %r9d
	.p2align	4
.LBB58_25:
	movq	632(%rcx), %rcx
	incq	%r9
	cmpq	%r9, %r8
	jne	.LBB58_25
	movq	%rax, %r8
	subq	%r9, %r8
	cmpq	$8, %rax
	jb	.LBB58_28
	.p2align	4
.LBB58_50:
	movq	632(%rcx), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rcx
	addq	$-8, %r8
	jne	.LBB58_50
.LBB58_28:
	movq	$1, (%rdx)
	xorl	%edi, %edi
	xorl	%eax, %eax
	movzwl	626(%rcx), %r8d
	cmpq	%r8, %rdi
	jb	.LBB58_30
.LBB58_31:
	movq	%rdx, %r15
	movl	$632, %r12d
	.p2align	4
.LBB58_32:
	movq	352(%rcx), %rbx
	testq	%rbx, %rbx
	je	.LBB58_46
	leaq	1(%rax), %r14
	movzwl	624(%rcx), %edi
	testq	%rax, %rax
	movl	$728, %edx
	cmoveq	%r12, %rdx
	movl	$8, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	movq	%rbx, %rcx
	movq	%r14, %rax
	cmpw	626(%rbx), %di
	jae	.LBB58_32
	movq	%r15, %rdx
	testq	%r14, %r14
	jne	.LBB58_39
.LBB58_36:
	leaq	1(%rdi), %r8
	movq	%rbx, %rcx
	jmp	.LBB58_37
.LBB58_40:
	movq	%r14, %r9
	xorl	%r8d, %r8d
	cmpq	$8, %r14
	jb	.LBB58_37
	.p2align	4
.LBB58_45:
	movq	(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rcx
	leaq	632(%rcx), %rax
	addq	$-8, %r9
	jne	.LBB58_45
.LBB58_37:
	movq	%rcx, 8(%rdx)
	movq	$0, 16(%rdx)
	movq	%r8, 24(%rdx)
	movq	%rbx, (%rsi)
	movq	%r14, 8(%rsi)
	movq	%rdi, 16(%rsi)
.LBB58_38:
	.seh_startepilogue
	addq	$48, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB58_4:
	testq	%rax, %rax
	je	.LBB58_5
	movq	%rax, %rcx
	andq	$7, %rcx
	je	.LBB58_7
	xorl	%edx, %edx
	.p2align	4
.LBB58_9:
	movq	632(%rdi), %rdi
	incq	%rdx
	cmpq	%rdx, %rcx
	jne	.LBB58_9
	movq	%rax, %rcx
	subq	%rdx, %rcx
	jmp	.LBB58_11
.LBB58_23:
	movq	%rax, %r8
	cmpq	$8, %rax
	jae	.LBB58_50
	jmp	.LBB58_28
.LBB58_5:
	xorl	%ebx, %ebx
	jmp	.LBB58_13
.LBB58_7:
	movq	%rax, %rcx
.LBB58_11:
	xorl	%ebx, %ebx
	cmpq	$8, %rax
	jb	.LBB58_13
	.p2align	4
.LBB58_12:
	movq	632(%rdi), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rax
	movq	632(%rax), %rdi
	addq	$-8, %rcx
	jne	.LBB58_12
	jmp	.LBB58_13
.LBB58_46:
	testq	%rax, %rax
	movl	$632, %eax
	movl	$728, %edx
	cmoveq	%rax, %rdx
	movl	$8, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.Ltmp280:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.16(%rip), %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13unwrap_failed
	nop
.Ltmp281:
	ud2
.LBB58_49:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.8(%rip), %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13unwrap_failed
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.section	.text,"xr",one_only,_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,59
	.seh_endproc
	.def	"?dtor$48@?0?_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$48@?0?_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA":
.seh_proc "?dtor$48@?0?_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"
.LBB58_48:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%r15
	.seh_pushreg %r15
	pushq	%r14
	.seh_pushreg %r14
	pushq	%r12
	.seh_pushreg %r12
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$32, %rsp
	.seh_stackalloc 32
	leaq	48(%rdx), %rbp
	.seh_endprologue
	ud2
.Lfunc_end58:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,59
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,26
	.p2align	2, 0x0
$cppxdata$_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@IMGREL
	.long	40
	.long	0
	.long	1
$stateUnwindMap$_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	-1
	.long	"?dtor$48@?0?_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack@4HA"@IMGREL
$ip2state$_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.long	.Lfunc_begin58@IMGREL
	.long	-1
	.long	.Ltmp280@IMGREL
	.long	0
	.long	.Ltmp281@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvMsz_NtNtNtCsdkPGTeTFVes_5alloc11collections5btree3mapINtB5_8IntoIterNtNtBb_6string6StringNtNtCsfZWMcdrZHWk_10serde_json5value5ValueE10dying_nextCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,59

	.def	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,60
	.p2align	4
_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin59:
	movq	(%rcx), %rax
	movzbl	(%rax), %eax
	leaq	switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack(%rip), %rcx
	movq	(%rcx,%rax,8), %r8
	leaq	switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack.138.rel(%rip), %rcx
	movslq	(%rcx,%rax,4), %rax
	addq	%rcx, %rax
	movq	(%rdx), %rcx
	movq	8(%rdx), %rdx
	movq	24(%rdx), %r9
	movq	%rax, %rdx
	rex64 jmpq	*%r9
.Lfunc_end59:

	.def	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRlNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRlNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,61
	.p2align	4
_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRlNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin60:
.seh_proc _RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRlNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	subq	$56, %rsp
	.seh_stackalloc 56
	.seh_endprologue
	movq	(%rcx), %rcx
	movl	16(%rdx), %eax
	testl	$33554432, %eax
	jne	.LBB60_3
	testl	$67108864, %eax
	jne	.LBB60_5
	.seh_startepilogue
	addq	$56, %rsp
	.seh_endepilogue
	jmp	_RNvXs9_NtNtNtCsd9eshbB3hfz_4core3fmt3num3implNtB9_7Display3fmt
.LBB60_3:
	movl	(%rcx), %r9d
	xorl	%eax, %eax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.1.llvm.3738276955999817974(%rip), %rcx
	movl	%r9d, %r8d
	.p2align	4
.LBB60_4:
	shrl	$4, %r8d
	andl	$15, %r9d
	movzbl	(%r9,%rcx), %r9d
	movb	%r9b, 55(%rsp,%rax)
	decq	%rax
	movl	%r8d, %r9d
	testl	%r8d, %r8d
	jne	.LBB60_4
	jmp	.LBB60_7
.LBB60_5:
	movl	(%rcx), %r9d
	xorl	%eax, %eax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.398.llvm.3738276955999817974(%rip), %rcx
	movl	%r9d, %r8d
	.p2align	4
.LBB60_6:
	shrl	$4, %r8d
	andl	$15, %r9d
	movzbl	(%r9,%rcx), %r9d
	movb	%r9b, 55(%rsp,%rax)
	decq	%rax
	movl	%r8d, %r9d
	testl	%r8d, %r8d
	jne	.LBB60_6
.LBB60_7:
	leaq	(%rsp,%rax), %rcx
	addq	$56, %rcx
	negq	%rax
	movq	%rax, 40(%rsp)
	movq	%rcx, 32(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.386.llvm.3738276955999817974(%rip), %r8
	movl	$2, %r9d
	movq	%rdx, %rcx
	movb	$1, %dl
	callq	_RNvMsa_NtCsd9eshbB3hfz_4core3fmtNtB5_9Formatter12pad_integral
	nop
	.seh_startepilogue
	addq	$56, %rsp
	.seh_endepilogue
	retq
.Lfunc_end60:
	.seh_endproc

	.def	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,62
	.p2align	4
_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin61:
.seh_proc _RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	subq	$72, %rsp
	.seh_stackalloc 72
	.seh_endprologue
	movq	%rdx, %rsi
	movq	(%rcx), %rax
	movl	16(%rdx), %ecx
	testl	$33554432, %ecx
	jne	.LBB61_3
	testl	$67108864, %ecx
	jne	.LBB61_5
	movq	(%rax), %rcx
	leaq	52(%rsp), %rdx
	movl	$20, %edi
	movl	$20, %r8d
	callq	_RNvMsf_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impy10__fmt_inner.llvm.3738276955999817974
	subq	%rax, %rdi
	addq	%rsp, %rax
	addq	$52, %rax
	movq	%rdi, 40(%rsp)
	movq	%rax, 32(%rsp)
	movl	$1, %r8d
	movq	%rsi, %rcx
	movb	$1, %dl
	xorl	%r9d, %r9d
	jmp	.LBB61_8
.LBB61_3:
	movq	(%rax), %rcx
	xorl	%eax, %eax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.1.llvm.3738276955999817974(%rip), %rdx
	.p2align	4
.LBB61_4:
	movl	%ecx, %r8d
	andl	$15, %r8d
	shrq	$4, %rcx
	movzbl	(%r8,%rdx), %r8d
	movb	%r8b, 67(%rsp,%rax)
	decq	%rax
	testq	%rcx, %rcx
	jne	.LBB61_4
	jmp	.LBB61_7
.LBB61_5:
	movq	(%rax), %rcx
	xorl	%eax, %eax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.398.llvm.3738276955999817974(%rip), %rdx
	.p2align	4
.LBB61_6:
	movl	%ecx, %r8d
	andl	$15, %r8d
	shrq	$4, %rcx
	movzbl	(%r8,%rdx), %r8d
	movb	%r8b, 67(%rsp,%rax)
	decq	%rax
	testq	%rcx, %rcx
	jne	.LBB61_6
.LBB61_7:
	leaq	(%rsp,%rax), %rcx
	addq	$68, %rcx
	negq	%rax
	movq	%rax, 40(%rsp)
	movq	%rcx, 32(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.386.llvm.3738276955999817974(%rip), %r8
	movl	$2, %r9d
	movq	%rsi, %rcx
	movb	$1, %dl
.LBB61_8:
	callq	_RNvMsa_NtCsd9eshbB3hfz_4core3fmtNtB5_9Formatter12pad_integral
	nop
	.seh_startepilogue
	addq	$72, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.Lfunc_end61:
	.seh_endproc

	.def	_RNvXs1i_NtCsd9eshbB3hfz_4core3fmtReNtB6_7Display3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXs1i_NtCsd9eshbB3hfz_4core3fmtReNtB6_7Display3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,63
	.p2align	4
_RNvXs1i_NtCsd9eshbB3hfz_4core3fmtReNtB6_7Display3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin62:
	movq	%rdx, %rax
	movq	(%rcx), %rdx
	movq	8(%rcx), %r8
	movq	%rax, %rcx
	jmp	_RNvMsa_NtCsd9eshbB3hfz_4core3fmtNtB5_9Formatter3pad
.Lfunc_end62:

	.def	_RNvXsR_NtCsd9eshbB3hfz_4core6optionINtB5_6OptionlENtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXsR_NtCsd9eshbB3hfz_4core6optionINtB5_6OptionlENtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,64
	.p2align	4
_RNvXsR_NtCsd9eshbB3hfz_4core6optionINtB5_6OptionlENtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin63:
.seh_proc _RNvXsR_NtCsd9eshbB3hfz_4core6optionINtB5_6OptionlENtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%r15
	.seh_pushreg %r15
	pushq	%r14
	.seh_pushreg %r14
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$96, %rsp
	.seh_stackalloc 96
	.seh_endprologue
	movq	%rdx, %rdi
	cmpl	$1, (%rcx)
	jne	.LBB63_11
	addq	$4, %rcx
	movq	%rcx, 40(%rsp)
	movq	(%rdi), %rsi
	movq	8(%rdi), %r15
	movq	24(%r15), %r14
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.170(%rip), %rdx
	movl	$4, %r8d
	movq	%rsi, %rcx
	callq	*%r14
	movb	$1, %bl
	testb	%al, %al
	je	.LBB63_2
.LBB63_10:
	movl	%ebx, %eax
	.seh_startepilogue
	addq	$96, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%r15
	.seh_endepilogue
	retq
.LBB63_11:
	movq	(%rdi), %rcx
	movq	8(%rdi), %rax
	movq	24(%rax), %rax
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.168(%rip), %rdx
	movl	$4, %r8d
	.seh_startepilogue
	addq	$96, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r14
	popq	%r15
	.seh_endepilogue
	rex64 jmpq	*%rax
.LBB63_2:
	testb	$-128, 18(%rdi)
	jne	.LBB63_6
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.39.llvm.3738276955999817974(%rip), %rdx
	movl	$1, %r8d
	movq	%rsi, %rcx
	callq	*%r14
	testb	%al, %al
	jne	.LBB63_10
	leaq	40(%rsp), %rcx
	movq	%rdi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRlNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB63_10
	movq	(%rdi), %rsi
	movq	8(%rdi), %rax
	movq	24(%rax), %r14
	jmp	.LBB63_9
.LBB63_6:
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.40.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	movq	%rsi, %rcx
	callq	*%r14
	testb	%al, %al
	jne	.LBB63_10
	movb	$1, 39(%rsp)
	movq	%rsi, 72(%rsp)
	movq	%r15, 80(%rsp)
	leaq	39(%rsp), %rax
	movq	%rax, 88(%rsp)
	movq	16(%rdi), %rax
	movq	%rax, 64(%rsp)
	leaq	72(%rsp), %rax
	movq	%rax, 48(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 56(%rsp)
	leaq	40(%rsp), %rcx
	leaq	48(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRlNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB63_10
	movq	48(%rsp), %rcx
	movq	56(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB63_10
.LBB63_9:
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.57.llvm.3738276955999817974(%rip), %rdx
	movl	$1, %r8d
	movq	%rsi, %rcx
	callq	*%r14
	movl	%eax, %ebx
	jmp	.LBB63_10
.Lfunc_end63:
	.seh_endproc

	.def	_RNvXsa_NtCsd9eshbB3hfz_4core5arrayAyjc_NtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXsa_NtCsd9eshbB3hfz_4core5arrayAyjc_NtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,65
	.p2align	4
_RNvXsa_NtCsd9eshbB3hfz_4core5arrayAyjc_NtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin64:
.seh_proc _RNvXsa_NtCsd9eshbB3hfz_4core5arrayAyjc_NtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	subq	$104, %rsp
	.seh_stackalloc 104
	.seh_endprologue
	movq	%rdx, %rsi
	movq	%rcx, %rdi
	movq	(%rdx), %rcx
	movq	8(%rdx), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.192.llvm.3738276955999817974(%rip), %rdx
	movl	$1, %r8d
	callq	*24(%rax)
	movq	%rdi, 48(%rsp)
	testb	%al, %al
	je	.LBB64_1
.LBB64_17:
	movb	$1, %al
.LBB64_18:
	.seh_startepilogue
	addq	$104, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.LBB64_1:
	testb	$-128, 18(%rsi)
	jne	.LBB64_6
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	leaq	8(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
.LBB64_3:
	testb	$-128, 18(%rsi)
	jne	.LBB64_9
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	leaq	16(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	je	.LBB64_11
	jmp	.LBB64_17
.LBB64_6:
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.41.llvm.3738276955999817974(%rip), %rdx
	movl	$1, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	leaq	8(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	je	.LBB64_3
	jmp	.LBB64_17
.LBB64_9:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	leaq	16(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
.LBB64_11:
	testb	$-128, 18(%rsi)
	jne	.LBB64_14
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_16
.LBB64_14:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_16:
	leaq	24(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
	testb	$-128, 18(%rsi)
	jne	.LBB64_22
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_24
.LBB64_22:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_24:
	leaq	32(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
	testb	$-128, 18(%rsi)
	jne	.LBB64_28
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_30
.LBB64_28:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_30:
	leaq	40(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
	testb	$-128, 18(%rsi)
	jne	.LBB64_34
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_36
.LBB64_34:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_36:
	leaq	48(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
	testb	$-128, 18(%rsi)
	jne	.LBB64_40
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_42
.LBB64_40:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_42:
	leaq	56(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
	testb	$-128, 18(%rsi)
	jne	.LBB64_46
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_48
.LBB64_46:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_48:
	leaq	64(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
	testb	$-128, 18(%rsi)
	jne	.LBB64_52
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_54
.LBB64_52:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_54:
	leaq	72(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
	testb	$-128, 18(%rsi)
	jne	.LBB64_58
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_60
.LBB64_58:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_60:
	leaq	80(%rdi), %rcx
	movq	%rcx, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
	testb	$-128, 18(%rsi)
	jne	.LBB64_64
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_66
.LBB64_64:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_66:
	addq	$88, %rdi
	movq	%rdi, 48(%rsp)
	testb	%al, %al
	jne	.LBB64_17
	testb	$-128, 18(%rsi)
	jne	.LBB64_70
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.35.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB64_17
	leaq	48(%rsp), %rcx
	movq	%rsi, %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	jmp	.LBB64_72
.LBB64_70:
	movb	$1, 47(%rsp)
	movups	(%rsi), %xmm0
	movaps	%xmm0, 80(%rsp)
	leaq	47(%rsp), %rax
	movq	%rax, 96(%rsp)
	movq	16(%rsi), %rax
	movq	%rax, 72(%rsp)
	leaq	80(%rsp), %rax
	movq	%rax, 56(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 64(%rsp)
	leaq	48(%rsp), %rcx
	leaq	56(%rsp), %rdx
	callq	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRyNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	testb	%al, %al
	jne	.LBB64_17
	movq	56(%rsp), %rcx
	movq	64(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
.LBB64_72:
	movl	%eax, %ecx
	movb	$1, %al
	testb	%cl, %cl
	jne	.LBB64_18
	movq	(%rsi), %rcx
	movq	8(%rsi), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.63.llvm.3738276955999817974(%rip), %rdx
	movl	$1, %r8d
	callq	*24(%rax)
	nop
	.seh_startepilogue
	addq	$104, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.Lfunc_end64:
	.seh_endproc

	.def	_RNvXsb_NtCsdkPGTeTFVes_5alloc6borrowINtB5_3CoweENtNtCsd9eshbB3hfz_4core3fmt7Display3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXsb_NtCsdkPGTeTFVes_5alloc6borrowINtB5_3CoweENtNtCsd9eshbB3hfz_4core3fmt7Display3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,66
	.p2align	4
_RNvXsb_NtCsdkPGTeTFVes_5alloc6borrowINtB5_3CoweENtNtCsd9eshbB3hfz_4core3fmt7Display3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin65:
	movq	%rdx, %rax
	movq	8(%rcx), %rdx
	movq	16(%rcx), %r8
	movq	%rax, %rcx
	jmp	_RNvMsa_NtCsd9eshbB3hfz_4core3fmtNtB5_9Formatter3pad
.Lfunc_end65:

	.def	_RNvXsd_NtNtCsd9eshbB3hfz_4core3num5errorNtB5_13ParseIntErrorNtNtB9_3fmt5Debug3fmt;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXsd_NtNtCsd9eshbB3hfz_4core3num5errorNtB5_13ParseIntErrorNtNtB9_3fmt5Debug3fmt,unique,67
	.p2align	4
_RNvXsd_NtNtCsd9eshbB3hfz_4core3num5errorNtB5_13ParseIntErrorNtNtB9_3fmt5Debug3fmt:
.Lfunc_begin66:
.seh_proc _RNvXsd_NtNtCsd9eshbB3hfz_4core3num5errorNtB5_13ParseIntErrorNtNtB9_3fmt5Debug3fmt
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$64, %rsp
	.seh_stackalloc 64
	.seh_endprologue
	movq	%rdx, %rsi
	movq	%rcx, 56(%rsp)
	movq	(%rdx), %rcx
	movq	8(%rdx), %rax
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.171(%rip), %rdx
	movl	$13, %r8d
	callq	*24(%rax)
	movq	%rsi, 40(%rsp)
	movb	%al, 48(%rsp)
	movb	$0, 49(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.167(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.172(%rip), %rdx
	leaq	40(%rsp), %rcx
	leaq	56(%rsp), %r9
	movl	$4, %r8d
	callq	_RNvMs1_NtNtCsd9eshbB3hfz_4core3fmt8buildersNtB5_11DebugStruct5field
	movzbl	48(%rsp), %ecx
	movzbl	49(%rsp), %eax
	movl	%eax, %edx
	notb	%dl
	orb	%cl, %dl
	testb	$1, %dl
	je	.LBB66_3
	orb	%cl, %al
	andb	$1, %al
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rsi
	.seh_endepilogue
	retq
.LBB66_3:
	movq	40(%rsp), %rax
	testb	$-128, 18(%rax)
	jne	.LBB66_5
	movq	(%rax), %rcx
	movq	8(%rax), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.137.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	jmp	.LBB66_6
.LBB66_5:
	movq	(%rax), %rcx
	movq	8(%rax), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.54.llvm.3738276955999817974(%rip), %rdx
	movl	$1, %r8d
.LBB66_6:
	callq	*24(%rax)
	andb	$1, %al
	.seh_startepilogue
	addq	$64, %rsp
	popq	%rsi
	.seh_endepilogue
	retq
.Lfunc_end66:
	.seh_endproc

	.def	_RNvXsd_NtNtCsd9eshbB3hfz_4core5alloc6layoutNtB5_11LayoutErrorNtNtB9_3fmt5Debug3fmt;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXsd_NtNtCsd9eshbB3hfz_4core5alloc6layoutNtB5_11LayoutErrorNtNtB9_3fmt5Debug3fmt,unique,68
	.p2align	4
_RNvXsd_NtNtCsd9eshbB3hfz_4core5alloc6layoutNtB5_11LayoutErrorNtNtB9_3fmt5Debug3fmt:
.Lfunc_begin67:
	movq	(%rdx), %rcx
	movq	8(%rdx), %rax
	movq	24(%rax), %rax
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.173(%rip), %rdx
	movl	$11, %r8d
	rex64 jmpq	*%rax
.Lfunc_end67:

	.def	_RNvXsk_NtCsaZOKLwlFm27_3std3envNtB5_8VarErrorNtNtCsd9eshbB3hfz_4core3fmt5Debug3fmt;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXsk_NtCsaZOKLwlFm27_3std3envNtB5_8VarErrorNtNtCsd9eshbB3hfz_4core3fmt5Debug3fmt,unique,69
	.p2align	4
_RNvXsk_NtCsaZOKLwlFm27_3std3envNtB5_8VarErrorNtNtCsd9eshbB3hfz_4core3fmt5Debug3fmt:
.Lfunc_begin68:
.seh_proc _RNvXsk_NtCsaZOKLwlFm27_3std3envNtB5_8VarErrorNtNtCsd9eshbB3hfz_4core3fmt5Debug3fmt
	pushq	%r15
	.seh_pushreg %r15
	pushq	%r14
	.seh_pushreg %r14
	pushq	%r12
	.seh_pushreg %r12
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$88, %rsp
	.seh_stackalloc 88
	.seh_endprologue
	movq	%rdx, %rdi
	cmpq	$-1, (%rcx)
	je	.LBB68_11
	movq	%rcx, %r14
	movq	(%rdi), %rsi
	movq	8(%rdi), %r12
	movq	24(%r12), %r15
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.182(%rip), %rdx
	movl	$10, %r8d
	movq	%rsi, %rcx
	callq	*%r15
	movb	$1, %bl
	testb	%al, %al
	je	.LBB68_2
.LBB68_10:
	movl	%ebx, %eax
	.seh_startepilogue
	addq	$88, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r14
	popq	%r15
	.seh_endepilogue
	retq
.LBB68_11:
	movq	(%rdi), %rcx
	movq	8(%rdi), %rax
	movq	24(%rax), %rax
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.180(%rip), %rdx
	movl	$10, %r8d
	.seh_startepilogue
	addq	$88, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r14
	popq	%r15
	.seh_endepilogue
	rex64 jmpq	*%rax
.LBB68_2:
	testb	$-128, 18(%rdi)
	jne	.LBB68_6
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.39.llvm.3738276955999817974(%rip), %rdx
	movl	$1, %r8d
	movq	%rsi, %rcx
	callq	*%r15
	testb	%al, %al
	jne	.LBB68_10
	movq	8(%r14), %rcx
	movq	16(%r14), %rdx
	movq	%rdi, %r8
	callq	_RNvXs1_NtCsd9eshbB3hfz_4core4wtf8NtB5_4Wtf8NtNtB7_3fmt5Debug3fmt
	testb	%al, %al
	jne	.LBB68_10
	movq	(%rdi), %rsi
	movq	8(%rdi), %rax
	movq	24(%rax), %r15
	jmp	.LBB68_9
.LBB68_6:
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.40.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	movq	%rsi, %rcx
	callq	*%r15
	testb	%al, %al
	jne	.LBB68_10
	movb	$1, 39(%rsp)
	movq	%rsi, 64(%rsp)
	movq	%r12, 72(%rsp)
	leaq	39(%rsp), %rax
	movq	%rax, 80(%rsp)
	movq	16(%rdi), %rax
	movq	%rax, 56(%rsp)
	leaq	64(%rsp), %rax
	movq	%rax, 40(%rsp)
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.142.llvm.3738276955999817974(%rip), %rax
	movq	%rax, 48(%rsp)
	movq	8(%r14), %rcx
	movq	16(%r14), %rdx
	leaq	40(%rsp), %r8
	callq	_RNvXs1_NtCsd9eshbB3hfz_4core4wtf8NtB5_4Wtf8NtNtB7_3fmt5Debug3fmt
	testb	%al, %al
	jne	.LBB68_10
	movq	40(%rsp), %rcx
	movq	48(%rsp), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.38.llvm.3738276955999817974(%rip), %rdx
	movl	$2, %r8d
	callq	*24(%rax)
	testb	%al, %al
	jne	.LBB68_10
.LBB68_9:
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.57.llvm.3738276955999817974(%rip), %rdx
	movl	$1, %r8d
	movq	%rsi, %rcx
	callq	*%r15
	movl	%eax, %ebx
	jmp	.LBB68_10
.Lfunc_end68:
	.seh_endproc

	.def	_RNvXsn_NtCsdkPGTeTFVes_5alloc5boxedINtB5_3BoxDNtNtCsd9eshbB3hfz_4core3any3AnyNtNtBM_6marker4SendEL_ENtNtBM_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXsn_NtCsdkPGTeTFVes_5alloc5boxedINtB5_3BoxDNtNtCsd9eshbB3hfz_4core3any3AnyNtNtBM_6marker4SendEL_ENtNtBM_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,70
	.p2align	4
_RNvXsn_NtCsdkPGTeTFVes_5alloc5boxedINtB5_3BoxDNtNtCsd9eshbB3hfz_4core3any3AnyNtNtBM_6marker4SendEL_ENtNtBM_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin69:
.seh_proc _RNvXsn_NtCsdkPGTeTFVes_5alloc5boxedINtB5_3BoxDNtNtCsd9eshbB3hfz_4core3any3AnyNtNtBM_6marker4SendEL_ENtNtBM_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	subq	$40, %rsp
	.seh_stackalloc 40
	.seh_endprologue
	movq	(%rdx), %rsi
	movq	8(%rdx), %rax
	movq	24(%rax), %rax
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.334.llvm.3738276955999817974(%rip), %rdx
	movl	$3, %r8d
	movq	%rsi, %rcx
	movq	%rax, %rdi
	callq	*%rax
	testb	%al, %al
	je	.LBB69_2
	movb	$1, %al
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	retq
.LBB69_2:
	leaq	anon.974f1136b6141ea9da06b7a43ea1d3e9.51.llvm.3738276955999817974(%rip), %rdx
	movl	$7, %r8d
	movq	%rsi, %rcx
	movq	%rdi, %rax
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rdi
	popq	%rsi
	.seh_endepilogue
	rex64 jmpq	*%rax
.Lfunc_end69:
	.seh_endproc

	.def	_RNvXsq_NtCsdkPGTeTFVes_5alloc6stringNtB5_6StringNtNtCsd9eshbB3hfz_4core3fmt7Display3fmt;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvXsq_NtCsdkPGTeTFVes_5alloc6stringNtB5_6StringNtNtCsd9eshbB3hfz_4core3fmt7Display3fmt,unique,71
	.p2align	4
_RNvXsq_NtCsdkPGTeTFVes_5alloc6stringNtB5_6StringNtNtCsd9eshbB3hfz_4core3fmt7Display3fmt:
.Lfunc_begin70:
	movq	%rdx, %rax
	movq	8(%rcx), %rdx
	movq	16(%rcx), %r8
	movq	%rax, %rcx
	jmp	_RNvMsa_NtCsd9eshbB3hfz_4core3fmtNtB5_9Formatter3pad
.Lfunc_end70:

	.def	_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.p2align	4
_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
.Lfunc_begin71:
.seh_proc _RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$680, %rsp
	.seh_stackalloc 680
	leaq	128(%rsp), %rbp
	.seh_setframe %rbp, 128
	.seh_endprologue
	movq	$-2, 544(%rbp)
	movq	%rcx, %rsi
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.77(%rip), %rdx
	leaq	160(%rbp), %rcx
	movl	$20, %r8d
	callq	_RNvNtCsaZOKLwlFm27_3std3env7__var_os
	movq	160(%rbp), %rax
	cmpq	$-1, %rax
	je	.LBB71_161
	movq	168(%rbp), %rdi
	movq	176(%rbp), %rbx
	testb	$1, 184(%rbp)
	jne	.LBB71_15
	testq	%rbx, %rbx
	je	.LBB71_15
	movzbl	191(%rbp), %ecx
	shll	$16, %ecx
	movzwl	189(%rbp), %edx
	orl	%ecx, %edx
	shlq	$32, %rdx
	movl	185(%rbp), %ecx
	orq	%rdx, %rcx
	leaq	(%rdi,%rbx), %rdx
	movq	%rdi, %r9
	jmp	.LBB71_6
.LBB71_4:
	addq	$2, %r9
	cmpq	%rdx, %r8
	cmovneq	%r9, %r8
	.p2align	4
.LBB71_5:
	movq	%r8, %r9
	cmpq	%rdx, %r8
	je	.LBB71_15
.LBB71_6:
	leaq	1(%r9), %r8
	movzbl	(%r9), %r10d
	testb	%r10b, %r10b
	jns	.LBB71_5
	cmpb	$-32, %r10b
	jb	.LBB71_4
	cmpb	$-19, %r10b
	jne	.LBB71_13
	cmpq	%rdx, %r8
	je	.LBB71_15
	leaq	2(%r9), %r8
	cmpq	%rdx, %r8
	je	.LBB71_15
	cmpb	$-97, 1(%r9)
	ja	.LBB71_172
	addq	$3, %r9
	movq	%r9, %r8
	jmp	.LBB71_5
.LBB71_13:
	addq	$2, %r9
	cmpq	%rdx, %r8
	cmoveq	%r8, %r9
	xorl	%r8d, %r8d
	cmpq	%rdx, %r9
	setne	%r8b
	addq	%r9, %r8
	cmpb	$-16, %r10b
	jb	.LBB71_5
	xorl	%r9d, %r9d
	cmpq	%rdx, %r8
	setne	%r9b
	addq	%r9, %r8
	jmp	.LBB71_5
.LBB71_15:
	movq	%rax, 456(%rbp)
	movq	%rdi, 464(%rbp)
	movq	%rbx, 472(%rbp)
.Ltmp282:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.79(%rip), %rdx
	leaq	160(%rbp), %rcx
	movl	$16, %r8d
	callq	_RNvNtCsaZOKLwlFm27_3std3env7__var_os
	nop
.Ltmp283:
	movq	160(%rbp), %rax
	cmpq	$-1, %rax
	je	.LBB71_21
	movq	%rax, 520(%rbp)
	movq	168(%rbp), %rdx
	movq	176(%rbp), %r8
.Ltmp284:
	leaq	64(%rbp), %rcx
	movq	%rdx, 496(%rbp)
	callq	_RNvMs5_NtCsdkPGTeTFVes_5alloc4wtf8NtNtCsd9eshbB3hfz_4core4wtf84Wtf815to_string_lossy
	nop
.Ltmp285:
	movq	64(%rbp), %r11
	movq	72(%rbp), %rcx
	movq	80(%rbp), %r14
	cmpq	$1, %r14
	je	.LBB71_22
	testq	%r14, %r14
	je	.LBB71_175
	movzbl	(%rcx), %edx
	jmp	.LBB71_24
.LBB71_21:
	xorl	%r13d, %r13d
	jmp	.LBB71_40
.LBB71_22:
	movzbl	(%rcx), %edx
	movb	$1, %al
	cmpl	$43, %edx
	je	.LBB71_136
	cmpl	$45, %edx
	je	.LBB71_136
.LBB71_24:
	xorl	%r8d, %r8d
	cmpb	$43, %dl
	sete	%r8b
	movq	%r8, %rax
	negq	%rax
	movq	%r14, %rdx
	subq	%r8, %rdx
	addq	%rcx, %r8
	cmpq	$17, %rdx
	jae	.LBB71_29
	testq	%rdx, %rdx
	je	.LBB71_35
	addq	%rax, %r14
	negq	%r14
	xorl	%eax, %eax
	xorl	%edx, %edx
	.p2align	4
.LBB71_27:
	movzbl	(%r8,%rdx), %r9d
	addl	$-48, %r9d
	cmpl	$9, %r9d
	ja	.LBB71_159
	leaq	(%rax,%rax,4), %rax
	movl	%r9d, %r9d
	leaq	(%r9,%rax,2), %rax
	incq	%rdx
	movq	%r14, %r9
	addq	%rdx, %r9
	jne	.LBB71_27
	jmp	.LBB71_36
.LBB71_29:
	addq	%rax, %r14
	negq	%r14
	xorl	%eax, %eax
	movl	$10, %r9d
	xorl	%r10d, %r10d
	.p2align	4
.LBB71_30:
	movq	%r14, %rdx
	addq	%r10, %rdx
	je	.LBB71_36
	mulq	%r9
	movzbl	(%r8,%r10), %edx
	jo	.LBB71_135
	movzbl	%dl, %edx
	addl	$-48, %edx
	cmpl	$10, %edx
	jae	.LBB71_159
	movl	%edx, %edx
	incq	%r10
	addq	%rdx, %rax
	jae	.LBB71_30
	movb	$2, %al
	jmp	.LBB71_136
.LBB71_35:
	xorl	%eax, %eax
.LBB71_36:
	movq	%rax, %r12
	testq	%r11, %r11
	jle	.LBB71_38
	movl	$1, %r8d
	movq	%r11, %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB71_38:
	movq	520(%rbp), %rdx
	movl	$1, %r13d
	testq	%rdx, %rdx
	je	.LBB71_40
	movl	$1, %r8d
	movq	496(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB71_40:
.Ltmp288:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.78(%rip), %rdx
	leaq	160(%rbp), %rcx
	movl	$13, %r8d
	callq	_RNvNtCsaZOKLwlFm27_3std3env7__var_os
	nop
.Ltmp289:
	movq	160(%rbp), %r11
	cmpq	$-1, %r11
	je	.LBB71_162
	movq	168(%rbp), %rcx
	movq	176(%rbp), %r15
	testb	$1, 184(%rbp)
	je	.LBB71_46
.LBB71_43:
	cmpq	$1, %r15
	je	.LBB71_59
	testq	%r15, %r15
	je	.LBB71_164
	movzbl	(%rcx), %edx
	jmp	.LBB71_61
.LBB71_46:
	testq	%r15, %r15
	je	.LBB71_164
	movzbl	191(%rbp), %eax
	shll	$16, %eax
	movzwl	189(%rbp), %edx
	orl	%eax, %edx
	shlq	$32, %rdx
	movl	185(%rbp), %eax
	orq	%rdx, %rax
	leaq	(%rcx,%r15), %rdx
	movq	%rcx, %r9
	jmp	.LBB71_50
.LBB71_48:
	addq	$2, %r9
	cmpq	%rdx, %r8
	cmovneq	%r9, %r8
	.p2align	4
.LBB71_49:
	movq	%r8, %r9
	cmpq	%rdx, %r8
	je	.LBB71_43
.LBB71_50:
	leaq	1(%r9), %r8
	movzbl	(%r9), %r10d
	testb	%r10b, %r10b
	jns	.LBB71_49
	cmpb	$-32, %r10b
	jb	.LBB71_48
	cmpb	$-19, %r10b
	jne	.LBB71_57
	cmpq	%rdx, %r8
	je	.LBB71_43
	leaq	2(%r9), %r8
	cmpq	%rdx, %r8
	je	.LBB71_43
	cmpb	$-97, 1(%r9)
	ja	.LBB71_160
	addq	$3, %r9
	movq	%r9, %r8
	jmp	.LBB71_49
.LBB71_57:
	addq	$2, %r9
	cmpq	%rdx, %r8
	cmoveq	%r8, %r9
	xorl	%r8d, %r8d
	cmpq	%rdx, %r9
	setne	%r8b
	addq	%r9, %r8
	cmpb	$-16, %r10b
	jb	.LBB71_49
	xorl	%r9d, %r9d
	cmpq	%rdx, %r8
	setne	%r9b
	addq	%r9, %r8
	jmp	.LBB71_49
.LBB71_59:
	movzbl	(%rcx), %edx
	movb	$1, %al
	cmpl	$43, %edx
	je	.LBB71_134
	cmpl	$45, %edx
	je	.LBB71_134
.LBB71_61:
	xorl	%r8d, %r8d
	cmpb	$43, %dl
	sete	%r8b
	movq	%r8, %rax
	negq	%rax
	movq	%r15, %rdx
	subq	%r8, %rdx
	addq	%rcx, %r8
	cmpq	$17, %rdx
	jae	.LBB71_66
	testq	%rdx, %rdx
	je	.LBB71_72
	addq	%rax, %r15
	negq	%r15
	xorl	%r14d, %r14d
	xorl	%eax, %eax
	.p2align	4
.LBB71_64:
	movzbl	(%r8,%rax), %edx
	addl	$-48, %edx
	cmpl	$9, %edx
	ja	.LBB71_157
	leaq	(%r14,%r14,4), %r9
	movl	%edx, %edx
	leaq	(%rdx,%r9,2), %r14
	incq	%rax
	movq	%r15, %rdx
	addq	%rax, %rdx
	jne	.LBB71_64
	jmp	.LBB71_73
.LBB71_66:
	addq	%rax, %r15
	negq	%r15
	xorl	%r14d, %r14d
	movl	$10, %r9d
	xorl	%r10d, %r10d
	.p2align	4
.LBB71_67:
	movq	%r15, %rax
	addq	%r10, %rax
	je	.LBB71_73
	movq	%r14, %rax
	mulq	%r9
	movzbl	(%r8,%r10), %edx
	jo	.LBB71_133
	movq	%rax, %r14
	movzbl	%dl, %eax
	addl	$-48, %eax
	cmpl	$10, %eax
	jae	.LBB71_157
	movl	%eax, %eax
	incq	%r10
	addq	%rax, %r14
	jae	.LBB71_67
	movb	$2, %al
	jmp	.LBB71_134
.LBB71_72:
	xorl	%r14d, %r14d
.LBB71_73:
	movq	%r14, 16(%rbp)
	testq	%r11, %r11
	je	.LBB71_75
	movl	$1, %r8d
	movq	%r11, %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB71_75:
	movl	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9bootstrap8registry8REGISTRY+520(%rip), %eax
	movq	%rax, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM0.0(%rip)
	testq	%rbx, %rbx
	je	.LBB71_81
	cmpq	$12, %rbx
	jne	.LBB71_78
	movabsq	$3273667681516154726, %rax
	xorq	(%rdi), %rax
	movl	8(%rdi), %ecx
	xorq	$1701147238, %rcx
	orq	%rax, %rcx
	je	.LBB71_126
.LBB71_78:
	xorl	%r15d, %r15d
.LBB71_79:
	movl	$1, %edx
	movq	%rbx, %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB71_174
	movq	%rax, 520(%rbp)
	movq	%rax, %rcx
	movq	%rdi, %rdx
	movq	%rbx, %r8
	callq	memcpy
	jmp	.LBB71_82
.LBB71_81:
	movl	$1, %eax
	movq	%rax, 520(%rbp)
	movq	%rbx, %r15
.LBB71_82:
	movq	$0, 488(%rbp)
	movq	_RNvNvMNtNtCsaZOKLwlFm27_3std6thread2idNtB4_8ThreadId3new7COUNTER.llvm.14433937248639318539(%rip), %rax
	movq	%rbx, 528(%rbp)
	.p2align	4
.LBB71_83:
	cmpq	$-1, %rax
	je	.LBB71_132
	leaq	1(%rax), %rdi
	lock		cmpxchgq	%rdi, _RNvNvMNtNtCsaZOKLwlFm27_3std6thread2idNtB4_8ThreadId3new7COUNTER.llvm.14433937248639318539(%rip)
	jne	.LBB71_83
	movl	$48, %r8d
	xorl	%edx, %edx
	callq	_RNvNtNtNtCsaZOKLwlFm27_3std3sys5alloc7windows18process_heap_alloc
	testq	%rax, %rax
	je	.LBB71_178
	movq	$1, (%rax)
	movq	$1, 8(%rax)
	movq	$0, 24(%rax)
	movq	%rdi, 16(%rax)
	movb	$0, 40(%rax)
	movq	%rax, 496(%rbp)
	movq	%rax, 480(%rbp)
	movb	$1, 540(%rbp)
	movb	$1, 539(%rbp)
.Ltmp302:
	leaq	24(%rbp), %rdi
	leaq	480(%rbp), %rbx
	movq	%rdi, %rcx
	movq	%rbx, %rdx
	callq	_RNvNtNtCsaZOKLwlFm27_3std6thread9spawnhook15run_spawn_hooks
	nop
.Ltmp303:
	movq	$1, 160(%rbp)
	movq	$1, 168(%rbp)
	xorps	%xmm0, %xmm0
	movups	%xmm0, 176(%rbp)
	movl	$48, %ecx
	movl	$8, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB71_137
	movups	160(%rbp), %xmm0
	movups	176(%rbp), %xmm1
	movups	192(%rbp), %xmm2
	movups	%xmm2, 32(%rax)
	movups	%xmm1, 16(%rax)
	movups	%xmm0, (%rax)
	movq	%rax, 504(%rbp)
	lock		incq	(%rax)
	jle	.LBB71_179
	movq	%r13, 64(%rbp)
	movq	%r12, 72(%rbp)
	movq	528(%rbp), %rdx
	movq	%rdx, 80(%rbp)
	movq	520(%rbp), %rcx
	movq	%rcx, 88(%rbp)
	movq	%rdx, 96(%rbp)
	movq	%r15, 104(%rbp)
	movq	%r14, 112(%rbp)
	movups	24(%rbp), %xmm0
	movups	40(%rbp), %xmm1
	movups	%xmm1, 136(%rbp)
	movups	%xmm0, 120(%rbp)
	movq	%rax, 152(%rbp)
	movq	16(%rax), %rcx
	testq	%rcx, %rcx
	movq	%rax, %r12
	je	.LBB71_91
	lock		incq	24(%rcx)
	jle	.LBB71_145
.LBB71_91:
	movq	144(%rbp), %rax
	movq	%rax, 240(%rbp)
	movq	152(%rbp), %rax
	movq	%rax, 248(%rbp)
	movups	128(%rbp), %xmm0
	movaps	%xmm0, 224(%rbp)
	movups	64(%rbp), %xmm0
	movups	80(%rbp), %xmm1
	movups	96(%rbp), %xmm2
	movups	112(%rbp), %xmm3
	movaps	%xmm3, 208(%rbp)
	movaps	%xmm2, 192(%rbp)
	movaps	%xmm1, 176(%rbp)
	movaps	%xmm0, 160(%rbp)
	movl	$96, %ecx
	movl	$8, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB71_138
	movups	144(%rbp), %xmm0
	movups	%xmm0, 80(%rax)
	movups	128(%rbp), %xmm0
	movups	%xmm0, 64(%rax)
	movups	64(%rbp), %xmm0
	movups	80(%rbp), %xmm1
	movups	96(%rbp), %xmm2
	movups	112(%rbp), %xmm3
	movups	%xmm3, 48(%rax)
	movups	%xmm2, 32(%rax)
	movups	%xmm1, 16(%rax)
	movups	%xmm0, (%rax)
	movq	496(%rbp), %r15
	lock		incq	(%r15)
	jle	.LBB71_179
	movq	%r15, 160(%rbp)
	movq	%rax, 168(%rbp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.11(%rip), %rax
	movq	%rax, 176(%rbp)
	movl	$24, %ecx
	movl	$8, %edx
	callq	_RNvCs9hJ03s5DiqP_7___rustc12___rust_alloc
	testq	%rax, %rax
	je	.LBB71_139
	movq	%rax, %r14
	movq	176(%rbp), %rax
	movq	%rax, 16(%r14)
	movups	160(%rbp), %xmm0
	movups	%xmm0, (%r14)
	movq	$0, 40(%rsp)
	movl	$65536, 32(%rsp)
	leaq	_RNvNvMNtNtNtCsaZOKLwlFm27_3std3sys6thread7windowsNtB4_6Thread3new12thread_start.llvm.14433937248639318539(%rip), %r8
	movl	$2097152, %edx
	xorl	%ecx, %ecx
	movq	%r14, %r9
	callq	*__imp_CreateThread(%rip)
	testq	%rax, %rax
	je	.LBB71_165
	movq	%rax, %r13
	movq	504(%rbp), %r14
	movq	%r15, 160(%rbp)
	leaq	168(%rbp), %r12
	movq	%r14, 168(%rbp)
	movq	%rax, 176(%rbp)
	movq	%rax, %rcx
	movl	$-1, %edx
	callq	*__imp_WaitForSingleObject(%rip)
	cmpl	$-1, %eax
	je	.LBB71_171
	movq	%r13, %rcx
	callq	*__imp_CloseHandle(%rip)
	movq	$-1, %rcx
	movl	$1, %eax
	lock		cmpxchgq	%rcx, 8(%r14)
	jne	.LBB71_140
	movq	(%r14), %rax
	movq	$1, 8(%r14)
	cmpq	$1, %rax
	jne	.LBB71_140
	movq	168(%rbp), %rax
	movq	32(%rax), %r14
	movq	40(%rax), %r15
	testb	$1, 24(%rax)
	movq	$0, 24(%rax)
	je	.LBB71_141
	movq	160(%rbp), %rax
	lock		decq	(%rax)
	jne	.LBB71_101
	#MEMBARRIER
	leaq	160(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread6thread5InnerNtNtBM_5alloc6SystemE9drop_slowBM_
.LBB71_101:
	movq	168(%rbp), %rax
	lock		decq	(%rax)
	jne	.LBB71_103
	#MEMBARRIER
.Ltmp310:
	movq	%r12, %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEE9drop_slowBN_
	nop
.Ltmp311:
.LBB71_103:
	testq	%r14, %r14
	jne	.LBB71_142
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6RESULT.0(%rip), %rdx
	movq	%rdx, 56(%rbp)
	movq	472(%rbp), %rcx
	cmpq	$5, %rcx
	je	.LBB71_109
	cmpq	$12, %rcx
	je	.LBB71_108
	cmpq	$8, %rcx
	jne	.LBB71_146
	movq	464(%rbp), %rax
	movl	$1, %r8d
	movabsq	$8749489599857058149, %r9
	cmpq	%r9, (%rax)
	je	.LBB71_111
	jmp	.LBB71_146
.LBB71_108:
	movq	464(%rbp), %rax
	movabsq	$3273667681516154726, %r9
	xorq	(%rax), %r9
	movl	8(%rax), %r10d
	xorq	$1701147238, %r10
	movl	$1, %r8d
	orq	%r9, %r10
	je	.LBB71_111
	jmp	.LBB71_146
.LBB71_109:
	movq	464(%rbp), %rax
	movl	$1818324339, %r9d
	xorl	(%rax), %r9d
	movzbl	4(%rax), %r10d
	xorl	$108, %r10d
	movl	$3, %r8d
	orl	%r9d, %r10d
	je	.LBB71_111
	movl	$1735549292, %r9d
	xorl	(%rax), %r9d
	movzbl	4(%rax), %r10d
	xorl	$101, %r10d
	orl	%r9d, %r10d
	jne	.LBB71_146
.LBB71_111:
	movq	%r8, 8(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM2.0(%rip), %r10
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM1.0(%rip), %r11
	xorl	%r9d, %r9d
	subq	%r11, %r10
	cmovaeq	%r10, %r9
	movq	%r9, (%rbp)
	cmpq	$5, %rcx
	jne	.LBB71_114
	movl	$1818324339, %r11d
	xorl	(%rax), %r11d
	movzbl	4(%rax), %r14d
	xorl	$108, %r14d
	movl	$1, %r10d
	orl	%r11d, %r14d
	je	.LBB71_115
	movl	$1735549292, %r11d
	xorl	(%rax), %r11d
	movzbl	4(%rax), %r14d
	xorl	$101, %r14d
	orl	%r11d, %r14d
	je	.LBB71_115
.LBB71_114:
	xorl	%r10d, %r10d
.LBB71_115:
	movq	%r10, -8(%rbp)
	cmpq	%r10, %r9
	jne	.LBB71_143
	cmpq	%r8, %rdx
	jne	.LBB71_144
	movq	$7872, -16(%rbp)
	xorps	%xmm0, %xmm0
	movaps	%xmm0, 144(%rbp)
	movaps	%xmm0, 128(%rbp)
	movaps	%xmm0, 112(%rbp)
	movaps	%xmm0, 96(%rbp)
	movaps	%xmm0, 80(%rbp)
	movaps	%xmm0, 64(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack5ENTRY.0(%rip), %rdx
	movq	%rdx, -24(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4DEEP.0(%rip), %rdx
	movq	%rdx, -32(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3LOW.0(%rip), %rdx
	movq	%rdx, -40(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4HIGH.0(%rip), %rdx
	movq	%rdx, -48(%rbp)
	movq	$0, -56(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack10STRIDE_ACT.0(%rip), %rdx
	movq	%rdx, -64(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB0.0(%rip), %rdx
	movq	%rdx, -72(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB1.0(%rip), %rdx
	movq	%rdx, -80(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM0.0(%rip), %rdx
	movq	%rdx, 488(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM1.0(%rip), %rdx
	movq	%rdx, 480(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM2.0(%rip), %rdx
	movq	%rdx, 504(%rbp)
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM2.0(%rip), %r8
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM1.0(%rip), %r9
	xorl	%edx, %edx
	subq	%r9, %r8
	movl	$0, %r9d
	cmovaeq	%r8, %r9
	movq	%r9, 24(%rbp)
	cmpq	$5, %rcx
	jne	.LBB71_122
	movl	$1818324339, %ecx
	xorl	(%rax), %ecx
	movzbl	4(%rax), %edx
	xorl	$108, %edx
	orl	%ecx, %edx
	je	.LBB71_121
	movl	$1735549292, %ecx
	xorl	(%rax), %ecx
	movzbl	4(%rax), %eax
	xorl	$101, %eax
	orl	%ecx, %eax
	je	.LBB71_121
	xorl	%edx, %edx
	jmp	.LBB71_122
.LBB71_121:
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM2.0(%rip), %rax
	movq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM1.0(%rip), %rcx
	incq	%rcx
	cmpq	%rcx, %rax
	sete	%dl
.LBB71_122:
	movb	%dl, 538(%rbp)
	leaq	456(%rbp), %rax
	movq	%rax, 160(%rbp)
	leaq	_RNvXsq_NtCsdkPGTeTFVes_5alloc6stringNtB5_6StringNtNtCsd9eshbB3hfz_4core3fmt7Display3fmt(%rip), %rax
	movq	%rax, 168(%rbp)
	leaq	16(%rbp), %rax
	movq	%rax, 176(%rbp)
	leaq	_RNvXsi_NtNtNtCsd9eshbB3hfz_4core3fmt3num3impjNtB9_7Display3fmt(%rip), %rax
	movq	%rax, 184(%rbp)
	leaq	-24(%rbp), %rcx
	movq	%rcx, 192(%rbp)
	movq	%rax, 200(%rbp)
	leaq	-32(%rbp), %rcx
	movq	%rcx, 208(%rbp)
	movq	%rax, 216(%rbp)
	leaq	-40(%rbp), %rcx
	movq	%rcx, 224(%rbp)
	movq	%rax, 232(%rbp)
	leaq	-48(%rbp), %rcx
	movq	%rcx, 240(%rbp)
	movq	%rax, 248(%rbp)
	leaq	-56(%rbp), %rcx
	movq	%rcx, 256(%rbp)
	movq	%rax, 264(%rbp)
	leaq	56(%rbp), %rcx
	movq	%rcx, 272(%rbp)
	movq	%rax, 280(%rbp)
	leaq	-16(%rbp), %rcx
	movq	%rcx, 288(%rbp)
	movq	%rax, 296(%rbp)
	leaq	-64(%rbp), %rcx
	movq	%rcx, 304(%rbp)
	movq	%rax, 312(%rbp)
	leaq	-72(%rbp), %rcx
	movq	%rcx, 320(%rbp)
	movq	%rax, 328(%rbp)
	leaq	-80(%rbp), %rcx
	movq	%rcx, 336(%rbp)
	movq	%rax, 344(%rbp)
	leaq	488(%rbp), %rcx
	movq	%rcx, 352(%rbp)
	movq	%rax, 360(%rbp)
	movq	%rbx, 368(%rbp)
	movq	%rax, 376(%rbp)
	leaq	504(%rbp), %rcx
	movq	%rcx, 384(%rbp)
	movq	%rax, 392(%rbp)
	movq	%rdi, 400(%rbp)
	movq	%rax, 408(%rbp)
	leaq	538(%rbp), %rax
	movq	%rax, 416(%rbp)
	leaq	_RNvXsg_NtCsd9eshbB3hfz_4core3fmtbNtB5_7Display3fmt(%rip), %rax
	movq	%rax, 424(%rbp)
	leaq	64(%rbp), %rax
	movq	%rax, 432(%rbp)
	leaq	_RNvXsa_NtCsd9eshbB3hfz_4core5arrayAyjc_NtNtB7_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack(%rip), %rax
	movq	%rax, 440(%rbp)
.Ltmp318:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.140(%rip), %rcx
	leaq	160(%rbp), %rdx
	callq	_RNvNtNtCsaZOKLwlFm27_3std2io5stdio6__print
	nop
.Ltmp319:
	movq	456(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB71_125
	movq	464(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB71_125:
	movq	$-1, (%rsi)
	movq	%rsi, %rax
	.seh_startepilogue
	addq	$680, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB71_126:
	movl	_tls_index(%rip), %eax
	movq	%gs:88, %rcx
	movq	(%rcx,%rax,8), %rax
	movq	_RNvNCNKNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap5LOCAL0s_023___RUST_STD_INTERNAL_VAL@SECREL32(%rax), %rcx
	leaq	-1(%rcx), %rax
	cmpq	$-2, %rax
	jae	.LBB71_129
	movzbl	1600(%rcx), %eax
	testq	%rax, %rax
	je	.LBB71_150
.LBB71_128:
	decq	%rax
	movb	%al, 1600(%rcx)
	andl	$15, %eax
	movq	1608(%rcx,%rax,8), %r15
	movq	%r15, %rax
	andq	$-4194304, %rax
	movl	%r15d, %ecx
	shrl	$4, %ecx
	andl	$262143, %ecx
	andb	$63, 5512(%rax,%rcx)
	jmp	.LBB71_79
.LBB71_129:
	testq	%rcx, %rcx
	je	.LBB71_147
.LBB71_130:
.Ltmp294:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665(%rip), %rcx
	movl	$16, %edx
	movl	$64, %r8d
	callq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc18alloc_via_fallback
	nop
.Ltmp295:
.LBB71_131:
	movq	%rax, %r15
	testq	%rax, %rax
	jne	.LBB71_79
	jmp	.LBB71_155
.LBB71_132:
	movb	$1, 542(%rbp)
	movb	$1, 541(%rbp)
.Ltmp338:
	callq	_RNvNvMNtNtCsaZOKLwlFm27_3std6thread2idNtB4_8ThreadId3new9exhausted.llvm.14433937248639318539
	nop
.Ltmp339:
	jmp	.LBB71_179
.LBB71_133:
	addb	$-48, %dl
	xorl	%eax, %eax
	cmpb	$10, %dl
	adcb	$1, %al
.LBB71_134:
	movq	%rcx, 520(%rbp)
	movq	%r11, 528(%rbp)
	movb	%al, 160(%rbp)
.Ltmp290:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.129(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.128(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.145(%rip), %r9
	leaq	160(%rbp), %r8
	movl	$7, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp291:
	jmp	.LBB71_179
.LBB71_135:
	addb	$-48, %dl
	xorl	%eax, %eax
	cmpb	$10, %dl
	adcb	$1, %al
.LBB71_136:
	movq	%rcx, 512(%rbp)
	movq	%r11, 528(%rbp)
	movb	%al, 24(%rbp)
.Ltmp286:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.22(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.21(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.145(%rip), %r9
	leaq	24(%rbp), %r8
	movl	$11, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp287:
	jmp	.LBB71_179
.LBB71_137:
.Ltmp334:
	movq	%rax, 512(%rbp)
	movl	$8, %ecx
	movl	$48, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc5alloc18handle_alloc_error
	nop
.Ltmp335:
	jmp	.LBB71_179
.LBB71_138:
.Ltmp332:
	movq	%r12, 512(%rbp)
	movl	$8, %ecx
	movl	$96, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc5alloc18handle_alloc_error
	nop
.Ltmp333:
	jmp	.LBB71_179
.LBB71_139:
.Ltmp330:
	movq	%r12, 512(%rbp)
	movl	$8, %ecx
	movl	$24, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc5alloc18handle_alloc_error
	nop
.Ltmp331:
	jmp	.LBB71_179
.LBB71_140:
.Ltmp306:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.159(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.161(%rip), %r8
	movl	$41, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13expect_failed
	nop
.Ltmp307:
	jmp	.LBB71_179
.LBB71_141:
.Ltmp308:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.162(%rip), %rcx
	callq	_RNvNtCsd9eshbB3hfz_4core6option13unwrap_failed
	nop
.Ltmp309:
	jmp	.LBB71_179
.LBB71_142:
	movq	%r14, 160(%rbp)
	movq	%r15, 168(%rbp)
.Ltmp312:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.134(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.133(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.146(%rip), %r9
	leaq	160(%rbp), %r8
	movl	$12, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp313:
	jmp	.LBB71_179
.LBB71_143:
.Ltmp314:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.137(%rip), %rax
	movq	%rax, 40(%rsp)
	movq	$63, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.136(%rip), %r9
	movq	%rbp, %rdx
	leaq	-8(%rbp), %r8
	xorl	%ecx, %ecx
	callq	_RINvNtCsd9eshbB3hfz_4core9panicking13assert_failedjjEB4_
	nop
.Ltmp315:
	jmp	.LBB71_179
.LBB71_144:
.Ltmp316:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.139(%rip), %rax
	movq	%rax, 40(%rsp)
	movq	$47, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.138(%rip), %r9
	leaq	56(%rbp), %rdx
	leaq	8(%rbp), %r8
	xorl	%ecx, %ecx
	callq	_RINvNtCsd9eshbB3hfz_4core9panicking13assert_failedjjEB4_
	nop
.Ltmp317:
	jmp	.LBB71_179
.LBB71_145:
	movq	%r12, 512(%rbp)
	addq	$16, %rcx
	movb	$1, 543(%rbp)
.Ltmp304:
	callq	_RNvMNtNtCsaZOKLwlFm27_3std6thread6scopedNtB2_9ScopeData8overflow.llvm.14433937248639318539
	nop
.Ltmp305:
	jmp	.LBB71_179
.LBB71_146:
.Ltmp320:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.12(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.135(%rip), %r8
	movl	$40, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking5panic
	nop
.Ltmp321:
	jmp	.LBB71_179
.LBB71_147:
	xorps	%xmm0, %xmm0
	movaps	%xmm0, 224(%rbp)
	movaps	%xmm0, 208(%rbp)
	movaps	%xmm0, 192(%rbp)
	movaps	%xmm0, 176(%rbp)
	movaps	%xmm0, 160(%rbp)
	movq	$0, 240(%rbp)
.Ltmp292:
	leaq	160(%rbp), %rcx
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global8tls_heap28bind_slow_tagged_with_config
	nop
.Ltmp293:
	cmpq	$1, %rax
	je	.LBB71_130
	movq	%rdx, %rcx
	movzbl	1600(%rcx), %eax
	testq	%rax, %rax
	jne	.LBB71_128
.LBB71_150:
.Ltmp296:
	movl	$3, %edx
	callq	_RNvMNtNtNtNtCs2UMCkEEnPXf_11sefer_alloc8registry9heap_core5alloc3hotNtNtB6_4core8HeapCore20refill_magazine_slow
	nop
.Ltmp297:
	movq	%rax, %r15
	testq	%rax, %rax
	jne	.LBB71_79
	movq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit5QUERY(%rip), %rax
	testq	%rax, %rax
	je	.LBB71_177
	cmpq	$1, %rax
	je	.LBB71_155
.LBB71_154:
	callq	*%rax
	testb	%al, %al
	jne	.LBB71_176
.LBB71_155:
.Ltmp300:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.110(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.130(%rip), %r8
	movl	$30, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking5panic
	nop
.Ltmp301:
	jmp	.LBB71_179
.LBB71_157:
	movb	$1, %al
	jmp	.LBB71_134
.LBB71_159:
	movb	$1, %al
	jmp	.LBB71_136
.LBB71_160:
	shlq	$8, %rax
	jmp	.LBB71_163
.LBB71_161:
	jmp	.LBB71_173
.LBB71_162:
.LBB71_163:
	movq	%r11, 160(%rbp)
	movq	%rcx, 168(%rbp)
	movq	%r15, 176(%rbp)
	movq	%rax, 184(%rbp)
.Ltmp342:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.127(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.126(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.142(%rip), %r9
	leaq	160(%rbp), %r8
	movl	$5, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp343:
	jmp	.LBB71_179
.LBB71_164:
	xorl	%eax, %eax
	jmp	.LBB71_134
.LBB71_165:
	movq	%r12, 512(%rbp)
	movb	$0, 543(%rbp)
.Ltmp324:
	movq	%r14, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitEEB1g_.llvm.14433937248639318539
	nop
.Ltmp325:
	callq	*__imp_GetLastError(%rip)
	movl	%eax, %esi
	movq	504(%rbp), %rax
	lock		decq	(%rax)
	jne	.LBB71_168
	#MEMBARRIER
.Ltmp326:
	leaq	504(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEE9drop_slowBN_
	nop
.Ltmp327:
.LBB71_168:
	shlq	$32, %rsi
	orq	$2, %rsi
	movq	496(%rbp), %rax
	lock		decq	(%rax)
	jne	.LBB71_170
	#MEMBARRIER
	leaq	480(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread6thread5InnerNtNtBM_5alloc6SystemE9drop_slowBM_
.LBB71_170:
	movq	%rsi, 160(%rbp)
.Ltmp328:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.132(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.131(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.141(%rip), %r9
	leaq	160(%rbp), %r8
	movl	$14, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp329:
	jmp	.LBB71_179
.LBB71_171:
	movq	%r13, 528(%rbp)
	callq	*__imp_GetLastError(%rip)
	shlq	$32, %rax
	orq	$2, %rax
	movq	%rax, 24(%rbp)
	movq	%rdi, 64(%rbp)
	leaq	_RNvXs5_NtNtCsaZOKLwlFm27_3std2io5errorNtB5_5ErrorNtNtCsd9eshbB3hfz_4core3fmt7Display3fmt(%rip), %rax
	movq	%rax, 72(%rbp)
.Ltmp322:
	leaq	anon.9779e9ebe46825362f2cbf28c971eb7e.155.llvm.14433937248639318539(%rip), %rcx
	leaq	anon.9779e9ebe46825362f2cbf28c971eb7e.157.llvm.14433937248639318539(%rip), %r8
	leaq	64(%rbp), %rdx
	callq	_RNvNtCsd9eshbB3hfz_4core9panicking9panic_fmt
	nop
.Ltmp323:
	jmp	.LBB71_179
.LBB71_172:
	shlq	$8, %rcx
.LBB71_173:
	movq	%rax, 160(%rbp)
	movq	%rdi, 168(%rbp)
	movq	%rbx, 176(%rbp)
	movq	%rcx, 184(%rbp)
.Ltmp344:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.125(%rip), %rax
	movq	%rax, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.124(%rip), %rcx
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.142(%rip), %r9
	leaq	160(%rbp), %r8
	movl	$8, %edx
	callq	_RNvNtCsd9eshbB3hfz_4core6result13unwrap_failed
	nop
.Ltmp345:
	jmp	.LBB71_179
.LBB71_174:
.Ltmp340:
	movl	$1, %ecx
	movq	%rbx, %rdx
	callq	_RNvNtCsdkPGTeTFVes_5alloc7raw_vec12handle_error
	nop
.Ltmp341:
	jmp	.LBB71_179
.LBB71_175:
	xorl	%eax, %eax
	jmp	.LBB71_136
.LBB71_176:
.Ltmp298:
	movl	$16, %ecx
	movl	$64, %edx
	xorl	%r8d, %r8d
	callq	_RNvNtNtNtCs2UMCkEEnPXf_11sefer_alloc6global11sefer_alloc12global_alloc17exit_escape_alloc
	nop
.Ltmp299:
	jmp	.LBB71_131
.LBB71_177:
	callq	_RNvNtNtCs2UMCkEEnPXf_11sefer_alloc6global12process_exit7resolve
	cmpq	$1, %rax
	jne	.LBB71_154
	jmp	.LBB71_155
.LBB71_178:
	movb	$1, 542(%rbp)
	movb	$1, 541(%rbp)
.Ltmp336:
	movl	$8, %ecx
	movl	$48, %edx
	callq	_RNvNtCsdkPGTeTFVes_5alloc5alloc18handle_alloc_error
	nop
.Ltmp337:
.LBB71_179:
	ud2
	.seh_handlerdata
	.long	$cppxdata$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$180@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$180@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$180@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_180:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	160(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$181@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$181@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$181@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_181:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorEBH_.llvm.14433937248639318539
	movq	528(%rbp), %rcx
	callq	*__imp_CloseHandle(%rip)
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$182@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$182@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$182@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_182:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	160(%rbp), %rdx
	testq	%rdx, %rdx
	jle	.LBB71_184
	movq	168(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB71_184:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$185@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$185@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$185@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_185:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	160(%rbp), %rdx
	testq	%rdx, %rdx
	jle	.LBB71_187
	movq	168(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB71_187:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$188@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$188@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$188@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_188:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	160(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$189@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$189@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$189@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_189:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movb	$0, 543(%rbp)
	leaq	160(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9lifecycle10ThreadInitECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movb	$0, 543(%rbp)
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$190@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$190@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$190@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_190:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movb	$0, 543(%rbp)
	leaq	160(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_
	movb	$0, 543(%rbp)
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$191@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$191@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$191@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_191:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	leaq	160(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc4sync8ArcInnerINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$192@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$192@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$192@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_192:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	160(%rbp), %rax
	lock		decq	(%rax)
	jne	.LBB71_194
	#MEMBARRIER
	leaq	160(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread6thread5InnerNtNtBM_5alloc6SystemE9drop_slowBM_
.LBB71_194:
	movq	168(%rbp), %rax
	lock		decq	(%rax)
	leaq	168(%rbp), %rcx
	jne	.LBB71_196
	#MEMBARRIER
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEE9drop_slowBN_
	nop
.LBB71_196:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$197@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$197@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$197@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_197:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpb	$0, 543(%rbp)
	je	.LBB71_199
	leaq	64(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_
.LBB71_199:
	movq	504(%rbp), %rax
	lock		decq	(%rax)
	jne	.LBB71_201
	#MEMBARRIER
	leaq	504(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcINtNtNtCsaZOKLwlFm27_3std6thread9lifecycle6PacketuEE9drop_slowBN_
	nop
.LBB71_201:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$202@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$202@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$202@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_202:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	cmpq	$0, 512(%rbp)
	jne	.LBB71_204
	leaq	24(%rbp), %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std6thread9spawnhook15ChildSpawnHooksECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movb	$1, 540(%rbp)
	movb	$0, 539(%rbp)
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.LBB71_204:
	movb	$0, 540(%rbp)
	movb	$0, 539(%rbp)
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$205@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$205@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$205@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_205:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movzbl	539(%rbp), %ebx
	movzbl	540(%rbp), %esi
	movq	496(%rbp), %rax
	lock		decq	(%rax)
	jne	.LBB71_207
	#MEMBARRIER
	leaq	480(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread6thread5InnerNtNtBM_5alloc6SystemE9drop_slowBM_
.LBB71_207:
	andb	$1, %sil
	movb	%sil, 542(%rbp)
	andb	$1, %bl
	movb	%bl, 541(%rbp)
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$208@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$208@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$208@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_208:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	528(%rbp), %rdx
	testq	%rdx, %rdx
	jle	.LBB71_210
	movl	$1, %r8d
	movq	512(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB71_210:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$211@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$211@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$211@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_211:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	520(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB71_213
	movl	$1, %r8d
	movq	496(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB71_213:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$214@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$214@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$214@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_214:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	528(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB71_216
	movl	$1, %r8d
	movq	520(%rbp), %rcx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB71_216:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$217@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$217@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$217@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_217:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movzbl	541(%rbp), %ebx
	cmpq	$0, 528(%rbp)
	je	.LBB71_220
	testb	$1, 542(%rbp)
	je	.LBB71_220
	movl	$1, %r8d
	movq	520(%rbp), %rcx
	movq	528(%rbp), %rdx
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
.LBB71_220:
	testb	$1, %bl
	je	.LBB71_224
	movq	488(%rbp), %rax
	testq	%rax, %rax
	je	.LBB71_224
	lock		decq	(%rax)
	jne	.LBB71_224
	#MEMBARRIER
	leaq	488(%rbp), %rcx
	callq	_RNvMsn_NtCsdkPGTeTFVes_5alloc4syncINtB5_3ArcNtNtNtCsaZOKLwlFm27_3std6thread6scoped9ScopeDataE9drop_slowBM_
	nop
.LBB71_224:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.def	"?dtor$225@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$225@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$225@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB71_225:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
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
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	128(%rdx), %rbp
	.seh_endprologue
	movq	456(%rbp), %rdx
	testq	%rdx, %rdx
	je	.LBB71_227
	movq	464(%rbp), %rcx
	movl	$1, %r8d
	callq	_RNvCs9hJ03s5DiqP_7___rustc14___rust_dealloc
	nop
.LBB71_227:
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r13
	popq	%r14
	popq	%r15
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end71:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,27
	.p2align	2, 0x0
$cppxdata$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	429065506
	.long	17
	.long	$stateUnwindMap$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.long	0
	.long	0
	.long	33
	.long	$ip2state$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.long	672
	.long	0
	.long	1
$stateUnwindMap$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	-1
	.long	"?dtor$182@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	-1
	.long	"?dtor$225@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	1
	.long	"?dtor$188@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	1
	.long	"?dtor$192@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	3
	.long	"?dtor$181@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	1
	.long	"?dtor$180@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	1
	.long	"?dtor$217@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	6
	.long	"?dtor$205@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	7
	.long	"?dtor$202@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	8
	.long	"?dtor$197@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	9
	.long	"?dtor$189@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	9
	.long	"?dtor$190@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	8
	.long	"?dtor$191@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	1
	.long	"?dtor$214@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	1
	.long	"?dtor$185@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	1
	.long	"?dtor$211@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
	.long	15
	.long	"?dtor$208@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
$ip2state$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	.Lfunc_begin71@IMGREL
	.long	-1
	.long	.Ltmp282@IMGREL
	.long	1
	.long	.Ltmp284@IMGREL
	.long	15
	.long	.Ltmp285@IMGREL
	.long	-1
	.long	.Ltmp288@IMGREL
	.long	1
	.long	.Ltmp289@IMGREL
	.long	-1
	.long	.Ltmp302@IMGREL
	.long	7
	.long	.Ltmp303@IMGREL
	.long	-1
	.long	.Ltmp310@IMGREL
	.long	1
	.long	.Ltmp319@IMGREL
	.long	-1
	.long	.Ltmp294@IMGREL
	.long	1
	.long	.Ltmp338@IMGREL
	.long	6
	.long	.Ltmp290@IMGREL
	.long	13
	.long	.Ltmp286@IMGREL
	.long	16
	.long	.Ltmp334@IMGREL
	.long	12
	.long	.Ltmp332@IMGREL
	.long	11
	.long	.Ltmp330@IMGREL
	.long	10
	.long	.Ltmp306@IMGREL
	.long	3
	.long	.Ltmp312@IMGREL
	.long	2
	.long	.Ltmp314@IMGREL
	.long	1
	.long	.Ltmp304@IMGREL
	.long	9
	.long	.Ltmp320@IMGREL
	.long	1
	.long	.Ltmp297@IMGREL
	.long	-1
	.long	.Ltmp300@IMGREL
	.long	1
	.long	.Ltmp342@IMGREL
	.long	14
	.long	.Ltmp324@IMGREL
	.long	9
	.long	.Ltmp326@IMGREL
	.long	8
	.long	.Ltmp328@IMGREL
	.long	5
	.long	.Ltmp322@IMGREL
	.long	4
	.long	.Ltmp344@IMGREL
	.long	0
	.long	.Ltmp340@IMGREL
	.long	1
	.long	.Ltmp336@IMGREL
	.long	6
	.long	.Ltmp337@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,72

	.def	_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,73
	.p2align	4
_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
.Lfunc_begin72:
.seh_proc _RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, (%rbp)
	movq	%rcx, %rsi
	movb	$1, %cl
	xorl	%eax, %eax
	lock		cmpxchgb	%cl, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip)
	jne	.LBB72_1
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	jne	.LBB72_4
.LBB72_3:
	movl	$0, -4(%rbp)
.LBB72_5:
	movzbl	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE+1(%rip), %eax
.Ltmp346:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.89(%rip), %rcx
	movl	$5, %edx
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure
	nop
.Ltmp347:
	cmpb	$0, -4(%rbp)
	jne	.LBB72_10
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	jne	.LBB72_8
.LBB72_10:
	xorl	%eax, %eax
	xchgb	%al, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip)
	cmpb	$2, %al
	je	.LBB72_11
.LBB72_12:
	movq	$-1, (%rsi)
	movq	%rsi, %rax
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.LBB72_1:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip), %rcx
	callq	_RNvMNtNtNtNtCsaZOKLwlFm27_3std3sys4sync5mutex5futexNtB2_5Mutex14lock_contended
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	je	.LBB72_3
.LBB72_4:
	callq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count17is_zero_slow_path
	xorb	$1, %al
	movl	%eax, -4(%rbp)
	jmp	.LBB72_5
.LBB72_11:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip), %rcx
	callq	*__imp_WakeByAddressSingle(%rip)
	jmp	.LBB72_12
.LBB72_8:
	callq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count17is_zero_slow_path
	testb	%al, %al
	jne	.LBB72_10
	movb	$1, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE+1(%rip)
	jmp	.LBB72_10
	.seh_handlerdata
	.long	$cppxdata$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,73
	.seh_endproc
	.def	"?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB72_13:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	48(%rdx), %rbp
	.seh_endprologue
	movl	-4(%rbp), %ecx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtNtNtCsaZOKLwlFm27_3std4sync6poison5mutex10MutexGuarduEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end72:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,73
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,28
	.p2align	2, 0x0
$cppxdata$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.long	48
	.long	0
	.long	1
$stateUnwindMap$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	-1
	.long	"?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
$ip2state$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	.Lfunc_begin72@IMGREL
	.long	-1
	.long	.Ltmp346@IMGREL
	.long	0
	.long	.Ltmp347@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,73

	.def	_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,74
	.p2align	4
_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
.Lfunc_begin73:
.seh_proc _RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, (%rbp)
	movq	%rcx, %rsi
	movb	$1, %cl
	xorl	%eax, %eax
	lock		cmpxchgb	%cl, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip)
	jne	.LBB73_1
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	jne	.LBB73_4
.LBB73_3:
	movl	$0, -4(%rbp)
.LBB73_5:
	movzbl	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE+1(%rip), %eax
.Ltmp348:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.88(%rip), %rcx
	movl	$5, %edx
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure
	nop
.Ltmp349:
	cmpb	$0, -4(%rbp)
	jne	.LBB73_10
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	jne	.LBB73_8
.LBB73_10:
	xorl	%eax, %eax
	xchgb	%al, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip)
	cmpb	$2, %al
	je	.LBB73_11
.LBB73_12:
	movq	$-1, (%rsi)
	movq	%rsi, %rax
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.LBB73_1:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip), %rcx
	callq	_RNvMNtNtNtNtCsaZOKLwlFm27_3std3sys4sync5mutex5futexNtB2_5Mutex14lock_contended
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	je	.LBB73_3
.LBB73_4:
	callq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count17is_zero_slow_path
	xorb	$1, %al
	movl	%eax, -4(%rbp)
	jmp	.LBB73_5
.LBB73_11:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip), %rcx
	callq	*__imp_WakeByAddressSingle(%rip)
	jmp	.LBB73_12
.LBB73_8:
	callq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count17is_zero_slow_path
	testb	%al, %al
	jne	.LBB73_10
	movb	$1, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE+1(%rip)
	jmp	.LBB73_10
	.seh_handlerdata
	.long	$cppxdata$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,74
	.seh_endproc
	.def	"?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB73_13:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	48(%rdx), %rbp
	.seh_endprologue
	movl	-4(%rbp), %ecx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtNtNtCsaZOKLwlFm27_3std4sync6poison5mutex10MutexGuarduEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end73:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,74
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,29
	.p2align	2, 0x0
$cppxdata$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.long	48
	.long	0
	.long	1
$stateUnwindMap$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	-1
	.long	"?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
$ip2state$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	.Lfunc_begin73@IMGREL
	.long	-1
	.long	.Ltmp348@IMGREL
	.long	0
	.long	.Ltmp349@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,74

	.def	_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,75
	.p2align	4
_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
.Lfunc_begin74:
.seh_proc _RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_
	.seh_handler __CxxFrameHandler3, @unwind, @except
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$56, %rsp
	.seh_stackalloc 56
	leaq	48(%rsp), %rbp
	.seh_setframe %rbp, 48
	.seh_endprologue
	movq	$-2, (%rbp)
	movq	%rcx, %rsi
	movb	$1, %cl
	xorl	%eax, %eax
	lock		cmpxchgb	%cl, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip)
	jne	.LBB74_1
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	jne	.LBB74_4
.LBB74_3:
	movl	$0, -4(%rbp)
.LBB74_5:
	movzbl	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE+1(%rip), %eax
.Ltmp350:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.113(%rip), %rcx
	movl	$12, %edx
	callq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7measure
	nop
.Ltmp351:
	cmpb	$0, -4(%rbp)
	jne	.LBB74_10
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	jne	.LBB74_8
.LBB74_10:
	xorl	%eax, %eax
	xchgb	%al, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip)
	cmpb	$2, %al
	je	.LBB74_11
.LBB74_12:
	movq	$-1, (%rsi)
	movq	%rsi, %rax
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.LBB74_1:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip), %rcx
	callq	_RNvMNtNtNtNtCsaZOKLwlFm27_3std3sys4sync5mutex5futexNtB2_5Mutex14lock_contended
	movq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count18GLOBAL_PANIC_COUNT(%rip), %rax
	shlq	%rax
	testq	%rax, %rax
	je	.LBB74_3
.LBB74_4:
	callq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count17is_zero_slow_path
	xorb	$1, %al
	movl	%eax, -4(%rbp)
	jmp	.LBB74_5
.LBB74_11:
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE(%rip), %rcx
	callq	*__imp_WakeByAddressSingle(%rip)
	jmp	.LBB74_12
.LBB74_8:
	callq	_RNvNtNtCsaZOKLwlFm27_3std9panicking11panic_count17is_zero_slow_path
	testb	%al, %al
	jne	.LBB74_10
	movb	$1, _RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE+1(%rip)
	jmp	.LBB74_10
	.seh_handlerdata
	.long	$cppxdata$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,75
	.seh_endproc
	.def	"?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA";
	.scl	3;
	.type	32;
	.endef
	.p2align	4
"?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA":
.seh_proc "?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"
.LBB74_13:
	movq	%rdx, 16(%rsp)
	pushq	%rbp
	.seh_pushreg %rbp
	pushq	%rsi
	.seh_pushreg %rsi
	subq	$40, %rsp
	.seh_stackalloc 40
	leaq	48(%rdx), %rbp
	.seh_endprologue
	movl	-4(%rbp), %ecx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtNtNtCsaZOKLwlFm27_3std4sync6poison5mutex10MutexGuarduEECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	nop
	.seh_startepilogue
	addq	$40, %rsp
	popq	%rsi
	popq	%rbp
	.seh_endepilogue
	retq
.Lfunc_end74:
	.seh_handlerdata
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,75
	.seh_endproc
	.section	.xdata,"dr",associative,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,30
	.p2align	2, 0x0
$cppxdata$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	429065506
	.long	1
	.long	$stateUnwindMap$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.long	0
	.long	0
	.long	3
	.long	$ip2state$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@IMGREL
	.long	48
	.long	0
	.long	1
$stateUnwindMap$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	-1
	.long	"?dtor$13@?0?_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_@4HA"@IMGREL
$ip2state$_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_:
	.long	.Lfunc_begin74@IMGREL
	.long	-1
	.long	.Ltmp350@IMGREL
	.long	0
	.long	.Ltmp351@IMGREL
	.long	-1
	.section	.text,"xr",one_only,_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_,unique,75

	.def	_RNvYNtNtNtNtCsaZOKLwlFm27_3std3sys5stdio7windows6StderrNtNtBa_2io5Write9write_fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack;
	.scl	3;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvYNtNtNtNtCsaZOKLwlFm27_3std3sys5stdio7windows6StderrNtNtBa_2io5Write9write_fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,76
	.p2align	4
_RNvYNtNtNtNtCsaZOKLwlFm27_3std3sys5stdio7windows6StderrNtNtBa_2io5Write9write_fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
.Lfunc_begin75:
.seh_proc _RNvYNtNtNtNtCsaZOKLwlFm27_3std3sys5stdio7windows6StderrNtNtBa_2io5Write9write_fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	pushq	%r15
	.seh_pushreg %r15
	pushq	%r14
	.seh_pushreg %r14
	pushq	%r12
	.seh_pushreg %r12
	pushq	%rsi
	.seh_pushreg %rsi
	pushq	%rdi
	.seh_pushreg %rdi
	pushq	%rbx
	.seh_pushreg %rbx
	subq	$56, %rsp
	.seh_stackalloc 56
	.seh_endprologue
	movq	%rcx, %rdi
	movl	$62, %esi
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.166(%rip), %rbx
	leaq	48(%rsp), %r14
	leaq	.LJTI75_0(%rip), %r15
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.184(%rip), %r12
	jmp	.LBB75_1
	.p2align	4
.LBB75_13:
	testq	%rax, %rax
	je	.LBB75_14
.LBB75_1:
	movl	$-12, %ecx
	movq	%rbx, %rdx
	movq	%rsi, %r8
	movq	%rdi, %r9
	callq	_RNvNtNtNtCsaZOKLwlFm27_3std3sys5stdio7windows5write.llvm.14433937248639318539
	movq	%rax, 40(%rsp)
	movq	%rdx, 48(%rsp)
	cmpq	$1, %rax
	jne	.LBB75_3
	movl	%edx, %eax
	andl	$3, %eax
	movslq	(%r15,%rax,4), %rax
	addq	%r15, %rax
	jmpq	*%rax
.LBB75_9:
	movzbl	16(%rdx), %eax
	cmpb	$35, %al
	je	.LBB75_12
	jmp	.LBB75_15
	.p2align	4
.LBB75_3:
	testq	%rdx, %rdx
	je	.LBB75_4
	movq	%rsi, %rax
	subq	%rdx, %rax
	jb	.LBB75_5
	addq	%rdx, %rbx
	movq	%rax, %rsi
	jmp	.LBB75_13
.LBB75_10:
	movzbl	15(%rdx), %eax
	cmpb	$35, %al
	je	.LBB75_12
	jmp	.LBB75_15
.LBB75_8:
	movq	%rdx, %rax
	shrq	$32, %rax
	cmpb	$35, %al
	jne	.LBB75_15
.LBB75_12:
	movq	%r14, %rcx
	callq	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	movq	%rsi, %rax
	jmp	.LBB75_13
.LBB75_14:
	xorl	%edx, %edx
	jmp	.LBB75_15
.LBB75_4:
	movq	%r12, %rdx
.LBB75_15:
	movq	%rdx, %rax
	.seh_startepilogue
	addq	$56, %rsp
	popq	%rbx
	popq	%rdi
	popq	%rsi
	popq	%r12
	popq	%r14
	popq	%r15
	.seh_endepilogue
	retq
.LBB75_5:
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.185(%rip), %r9
	movq	%rdx, %rcx
	movq	%rsi, %rdx
	movq	%rsi, %r8
	callq	_RNvNtNtCsd9eshbB3hfz_4core5slice5index16slice_index_fail
	ud2
.Lfunc_end75:
	.section	.rdata,"dr",associative,_RNvYNtNtNtNtCsaZOKLwlFm27_3std3sys5stdio7windows6StderrNtNtBa_2io5Write9write_fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,77
	.p2align	2, 0x0
.LJTI75_0:
	.long	.LBB75_9-.LJTI75_0
	.long	.LBB75_10-.LJTI75_0
	.long	.LBB75_15-.LJTI75_0
	.long	.LBB75_8-.LJTI75_0
	.section	.text,"xr",one_only,_RNvYNtNtNtNtCsaZOKLwlFm27_3std3sys5stdio7windows6StderrNtNtBa_2io5Write9write_fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,76
	.seh_endproc

	.def	main;
	.scl	2;
	.type	32;
	.endef
	.section	.text,"xr",one_only,main,unique,78
	.globl	main
	.p2align	4
main:
.Lfunc_begin76:
.seh_proc main
	subq	$56, %rsp
	.seh_stackalloc 56
	.seh_endprologue
	movq	%rdx, %r9
	movslq	%ecx, %r8
	leaq	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4main(%rip), %rax
	movq	%rax, 48(%rsp)
	movb	$0, 32(%rsp)
	leaq	anon.22844a9aef6a14b72687ebf27443efb5.10(%rip), %rdx
	leaq	48(%rsp), %rcx
	callq	_RNvNtCsaZOKLwlFm27_3std2rt19lang_start_internal
	nop
	.seh_startepilogue
	addq	$56, %rsp
	.seh_endepilogue
	retq
.Lfunc_end76:
	.seh_endproc

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.0,unique,79
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.0:
	.asciz	"\000\000\000\000\000\000\000\000\b\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_RNSNvYNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtBd_4Once15call_once_forceNCINvMNtBf_9once_lockINtB1g_8OnceLockTjjjbEE10initializeNCINvB1f_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceTRNtBd_9OnceStateEE9call_once6vtableB2u_
	.quad	_RNCINvMs0_NtNtCsaZOKLwlFm27_3std4sync4onceNtB8_4Once15call_once_forceNCINvMNtBa_9once_lockINtB1b_8OnceLockTjjjbEE10initializeNCINvB1a_11get_or_initNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7control0E0zE0E0B2p_

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.1,unique,80
anon.22844a9aef6a14b72687ebf27443efb5.1:
	.asciz	"<rust-toolchain-dir>\\toolchains\\stable-x86_64-pc-windows-msvc\\lib/rustlib/src/rust\\library\\std\\src\\sync\\once.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.2,unique,81
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.2:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.1
	.asciz	"t\000\000\000\000\000\000\000\342\000\000\000\024\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.4,unique,82
anon.22844a9aef6a14b72687ebf27443efb5.4:
	.asciz	"<rust-toolchain-dir>\\toolchains\\stable-x86_64-pc-windows-msvc\\lib/rustlib/src/rust\\library\\alloc\\src\\collections\\btree\\node.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.5,unique,83
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.5:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.4
	.asciz	"\203\000\000\000\000\000\000\000\360\000\000\000M\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.6,unique,84
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.6:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.4
	.asciz	"\203\000\000\000\000\000\000\000\023\005\000\000$\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.7,unique,85
anon.22844a9aef6a14b72687ebf27443efb5.7:
	.asciz	"<rust-toolchain-dir>\\toolchains\\stable-x86_64-pc-windows-msvc\\lib/rustlib/src/rust\\library\\alloc\\src\\collections\\btree\\navigate.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.8,unique,86
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.8:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.7
	.asciz	"\207\000\000\000\000\000\000\000\306\000\000\000'\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.9,unique,87
anon.22844a9aef6a14b72687ebf27443efb5.9:
	.asciz	"<rust-toolchain-dir>\\toolchains\\stable-x86_64-pc-windows-msvc\\lib/rustlib/src/rust\\library\\std\\src\\io\\mod.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.10,unique,88
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.10:
	.asciz	"\000\000\000\000\000\000\000\000\b\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_RNSNvYNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.quad	_RNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0Cs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.quad	_RNCINvNtCsaZOKLwlFm27_3std2rt10lang_startuE0Cs8fXedXvU3Hn_27b4_30_first_alloc_min_stack

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.11,unique,89
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.11:
	.quad	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0EB1E_
	.asciz	"`\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_RNSNvYNCINvNtNtCsaZOKLwlFm27_3std6thread9lifecycle15spawn_uncheckedNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_0uEs_0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableB17_

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.12,unique,90
anon.22844a9aef6a14b72687ebf27443efb5.12:
	.ascii	"internal error: entered unreachable code"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.13,unique,91
anon.22844a9aef6a14b72687ebf27443efb5.13:
	.ascii	"assertion failed: src.len() == dst.len()"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.14,unique,92
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.14:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.4
	.asciz	"\203\000\000\000\000\000\000\000T\007\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.15,unique,93
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.15:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.1
	.asciz	"t\000\000\000\000\000\000\000\342\000\000\0001\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.16,unique,94
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.16:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.7
	.asciz	"\207\000\000\000\000\000\000\000X\002\000\0000\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.17,unique,95
anon.22844a9aef6a14b72687ebf27443efb5.17:
	.ascii	"eat-only"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.18,unique,96
anon.22844a9aef6a14b72687ebf27443efb5.18:
	.ascii	"depth-64 anchor probe overflowed"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.19,unique,97
anon.22844a9aef6a14b72687ebf27443efb5.19:
	.asciz	"tests\\b4_30_first_alloc_min_stack.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.20,unique,98
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.20:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000V\002\000\000\037\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.21,unique,99
anon.22844a9aef6a14b72687ebf27443efb5.21:
	.ascii	"free target"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.22,unique,100
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.22:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\363\000\000\000J\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.23,unique,101
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.23:
	.asciz	"\000\000\000\000\000\000\000\000\020\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_RNSNvYNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_once6vtableBa_
	.quad	_RNCNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stacks_17b4_30_probe_childs_00B5_

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.24,unique,102
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.24:
	.zero	8

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.25,unique,103
anon.22844a9aef6a14b72687ebf27443efb5.25:
	.ascii	"target residue not reached"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.26,unique,104
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.26:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\"\001\000\000\021\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.27,unique,105
anon.22844a9aef6a14b72687ebf27443efb5.27:
	.ascii	"layout"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.28,unique,106
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.28:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000-\001\000\0008\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.29,unique,107
anon.22844a9aef6a14b72687ebf27443efb5.29:
	.ascii	"fallback lock acquired during scenario: first-bind path not isolated"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.30,unique,108
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.30:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000>\001\000\000\021\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.31,unique,109
anon.22844a9aef6a14b72687ebf27443efb5.31:
	.asciz	"<rust-toolchain-dir>\\toolchains\\stable-x86_64-pc-windows-msvc\\lib/rustlib/src/rust\\library\\alloc\\src\\collections\\btree\\map\\entry.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.32,unique,110
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.32:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.31
	.asciz	"\210\000\000\000\000\000\000\000\217\001\000\000.\000\000"

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack10STRIDE_ACT.0,unique,111
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack10STRIDE_ACT.0:
	.quad	0

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.33,unique,112
anon.22844a9aef6a14b72687ebf27443efb5.33:
	.ascii	"child stdout JSON"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.34,unique,113
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.34:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\335\001\000\000\n\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.35,unique,114
anon.22844a9aef6a14b72687ebf27443efb5.35:
	.ascii	"child JSON parse"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.36,unique,115
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.36:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\336\001\000\000;\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.37,unique,116
anon.22844a9aef6a14b72687ebf27443efb5.37:
	.ascii	"entry_addr"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.38,unique,117
anon.22844a9aef6a14b72687ebf27443efb5.38:
	.ascii	"deep_addr"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.39,unique,118
anon.22844a9aef6a14b72687ebf27443efb5.39:
	.ascii	"stack_low"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.40,unique,119
anon.22844a9aef6a14b72687ebf27443efb5.40:
	.ascii	"stack_high"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.41,unique,120
anon.22844a9aef6a14b72687ebf27443efb5.41:
	.ascii	"stride_actual"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.42,unique,121
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.42:
	.asciz	"\000\000 \000\000\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.43,unique,122
anon.22844a9aef6a14b72687ebf27443efb5.43:
	.ascii	"address geometry"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.44,unique,123
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.44:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\347\001\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.45,unique,124
anon.22844a9aef6a14b72687ebf27443efb5.45:
	.asciz	"\016stride_actual \300# outside the measured bound 1..=512"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.46,unique,125
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.46:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\353\001\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.47,unique,126
anon.22844a9aef6a14b72687ebf27443efb5.47:
	.ascii	"worker stack size != 2 MiB"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.48,unique,127
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.48:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\346\001\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.49,unique,128
anon.22844a9aef6a14b72687ebf27443efb5.49:
	.ascii	"stride"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.50,unique,129
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.50:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\343\001\000\0002\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.51,unique,130
anon.22844a9aef6a14b72687ebf27443efb5.51:
	.ascii	"high"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.52,unique,131
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.52:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\342\001\000\000)\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.53,unique,132
anon.22844a9aef6a14b72687ebf27443efb5.53:
	.ascii	"low"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.54,unique,133
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.54:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\341\001\000\000'\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.55,unique,134
anon.22844a9aef6a14b72687ebf27443efb5.55:
	.ascii	"deep"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.56,unique,135
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.56:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\340\001\000\000(\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.57,unique,136
anon.22844a9aef6a14b72687ebf27443efb5.57:
	.ascii	"entry"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.58,unique,137
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.58:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\337\001\000\000*\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.59,unique,138
anon.22844a9aef6a14b72687ebf27443efb5.59:
	.asciz	"\013top anchor \300\b (free0 \300\f) overflowed"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.60,unique,139
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.60:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000,\002\000\000\033\000\000"

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB0.0,unique,140
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB0.0:
	.quad	0

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB1.0,unique,141
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3FB1.0:
	.quad	0

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3LOW.0,unique,142
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3LOW.0:
	.quad	0

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3PTR,unique,143
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack3PTR:
	.zero	8

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4DEEP.0,unique,144
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4DEEP.0:
	.quad	0

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4HIGH.0,unique,145
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4HIGH.0:
	.quad	0

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4PREV,unique,146
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4PREV:
	.zero	8

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4SIZE,unique,147
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack4SIZE:
	.zero	8

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.61,unique,148
anon.22844a9aef6a14b72687ebf27443efb5.61:
	.ascii	"b4_30_foreign_free_first_alloc_stack_budget"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.62,unique,149
anon.22844a9aef6a14b72687ebf27443efb5.62:
	.ascii	"tests\\b4_30_first_alloc_min_stack.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.63,unique,150
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.63:
	.zero	8
	.zero	16
	.ascii	"\000\000\000\000\000\000\000\200"
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.61
	.asciz	"+\000\000\000\000\000\000"
	.zero	8
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.62
	.asciz	"$\000\000\000\000\000\000\000\000\000\000\000\000\000\000"
	.zero	8
	.ascii	"\346\002\000\000\000\000\000\000\004\000\000\000\000\000\000\000\346\002\000\000\000\000\000\000/\000\000\000\000\000\000\000\000\000\000\001"
	.zero	4
	.zero	8
	.quad	_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack43b4_30_foreign_free_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_
	.zero	8

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.64,unique,151
anon.22844a9aef6a14b72687ebf27443efb5.64:
	.ascii	"b4_30_large_first_alloc_stack_budget"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.65,unique,152
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.65:
	.zero	8
	.zero	16
	.ascii	"\000\000\000\000\000\000\000\200"
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.64
	.asciz	"$\000\000\000\000\000\000"
	.zero	8
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.62
	.asciz	"$\000\000\000\000\000\000\000\000\000\000\000\000\000\000"
	.zero	8
	.ascii	"\336\002\000\000\000\000\000\000\004\000\000\000\000\000\000\000\336\002\000\000\000\000\000\000(\000\000\000\000\000\000\000\000\000\000\001"
	.zero	4
	.zero	8
	.quad	_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_large_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_
	.zero	8

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.66,unique,153
anon.22844a9aef6a14b72687ebf27443efb5.66:
	.ascii	"b4_30_probe_child"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.67,unique,154
anon.22844a9aef6a14b72687ebf27443efb5.67:
	.ascii	"measurement child only"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.68,unique,155
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.68:
	.zero	8
	.zero	16
	.ascii	"\000\000\000\000\000\000\000\200"
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.66
	.asciz	"\021\000\000\000\000\000\000"
	.zero	8
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.62
	.asciz	"$\000\000\000\000\000\000"
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.67
	.ascii	"\026\000\000\000\000\000\000\000\360\000\000\000\000\000\000\000\004\000\000\000\000\000\000\000\360\000\000\000\000\000\000\000\025\000\000\000\000\000\000\000\001\000\000\001"
	.zero	4
	.zero	8
	.quad	_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack17b4_30_probe_child0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_
	.zero	8

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.69,unique,156
anon.22844a9aef6a14b72687ebf27443efb5.69:
	.ascii	"b4_30_small_first_alloc_stack_budget"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.70,unique,157
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.70:
	.zero	8
	.zero	16
	.ascii	"\000\000\000\000\000\000\000\200"
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.69
	.asciz	"$\000\000\000\000\000\000"
	.zero	8
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.62
	.asciz	"$\000\000\000\000\000\000\000\000\000\000\000\000\000\000"
	.zero	8
	.ascii	"\326\002\000\000\000\000\000\000\004\000\000\000\000\000\000\000\326\002\000\000\000\000\000\000(\000\000\000\000\000\000\000\000\000\000\001"
	.zero	4
	.zero	8
	.quad	_RNvYNCNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack36b4_30_small_first_alloc_stack_budget0INtNtNtCsd9eshbB3hfz_4core3ops8function6FnOnceuE9call_onceB6_
	.zero	8

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.71,unique,158
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.71:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.63
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.65
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.68
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.70

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.72,unique,159
anon.22844a9aef6a14b72687ebf27443efb5.72:
	.ascii	"current_exe"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.73,unique,160
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.73:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\366\001\000\000F\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.74,unique,161
anon.22844a9aef6a14b72687ebf27443efb5.74:
	.ascii	"--exact"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.75,unique,162
anon.22844a9aef6a14b72687ebf27443efb5.75:
	.ascii	"--ignored"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.76,unique,163
anon.22844a9aef6a14b72687ebf27443efb5.76:
	.ascii	"--nocapture"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.77,unique,164
anon.22844a9aef6a14b72687ebf27443efb5.77:
	.ascii	"SEFER_B4_30_SCENARIO"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.78,unique,165
anon.22844a9aef6a14b72687ebf27443efb5.78:
	.ascii	"SEFER_B4_30_P"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.79,unique,166
anon.22844a9aef6a14b72687ebf27443efb5.79:
	.ascii	"SEFER_B4_30_FREE"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.80,unique,167
anon.22844a9aef6a14b72687ebf27443efb5.80:
	.ascii	"spawn child"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.81,unique,168
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.81:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\375\001\000\000\032\000\000"

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack5ENTRY.0,unique,169
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack5ENTRY.0:
	.quad	0

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM0.0,unique,170
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM0.0:
	.quad	0

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM1.0,unique,171
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM1.0:
	.quad	0

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM2.0,unique,172
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6CLAIM2.0:
	.quad	0

	.section	.rdata,"dr",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665,unique,173
	.globl	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665:
	.zero	8
	.zero	8
	.zero	8
	.zero	8
	.zero	8
	.zero	8
	.zero	8
	.zero	8
	.zero	4
	.zero	4
	.zero	4
	.zero	4
	.zero	1
	.zero	7

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6RESULT.0,unique,174
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6RESULT.0:
	.quad	0

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.82,unique,175
anon.22844a9aef6a14b72687ebf27443efb5.82:
	.ascii	"windows"

	.section	.data,"dw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL,unique,176
	.p2align	3, 0x0
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7CONTROL:
	.zero	32
	.asciz	"\003\000\000"
	.zero	4

	.section	.bss,"bw",one_only,_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE,unique,177
_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack7MEASURE:
	.zero	2

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.83,unique,178
anon.22844a9aef6a14b72687ebf27443efb5.83:
	.ascii	"SEFER_B4_30_ENFORCE"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.84,unique,179
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.84:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.82
	.asciz	"\007\000\000\000\000\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.85,unique,180
anon.22844a9aef6a14b72687ebf27443efb5.85:
	.ascii	"release"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.86,unique,181
anon.22844a9aef6a14b72687ebf27443efb5.86:
	.asciz	"*B4-30 first-alloc differential stack need \300\037 B exceeds the declared budget \300\007 B (os=\300\t profile=\300\n scenario=\300P): the first-alloc path grew past the evidenced worst differential + its reserve"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.87,unique,182
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.87:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\216\002\000\000\t\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.88,unique,183
anon.22844a9aef6a14b72687ebf27443efb5.88:
	.ascii	"small"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.89,unique,184
anon.22844a9aef6a14b72687ebf27443efb5.89:
	.ascii	"large"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.90,unique,185
anon.22844a9aef6a14b72687ebf27443efb5.90:
	.ascii	"[b4-30] enforced scenario=foreign-free\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.91,unique,186
anon.22844a9aef6a14b72687ebf27443efb5.91:
	.ascii	"[b4-30] enforced scenario=large\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.92,unique,187
anon.22844a9aef6a14b72687ebf27443efb5.92:
	.ascii	"[b4-30] enforced scenario=small\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.93,unique,188
anon.22844a9aef6a14b72687ebf27443efb5.93:
	.asciz	"\022confirmation free \300$ outside the dynamic-stride window [\300\002, \300\001)"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.94,unique,189
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.94:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\253\002\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.95,unique,190
anon.22844a9aef6a14b72687ebf27443efb5.95:
	.ascii	"[b4-30] scenario=foreign-free need=\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.96,unique,191
anon.22844a9aef6a14b72687ebf27443efb5.96:
	.ascii	"[b4-30] scenario=large need=\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.97,unique,192
anon.22844a9aef6a14b72687ebf27443efb5.97:
	.ascii	"[b4-30] scenario=small need=\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.98,unique,193
anon.22844a9aef6a14b72687ebf27443efb5.98:
	.ascii	"bisect"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.99,unique,194
anon.22844a9aef6a14b72687ebf27443efb5.99:
	.ascii	"floor"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.100,unique,195
anon.22844a9aef6a14b72687ebf27443efb5.100:
	.asciz	"\021[b4-30] scenario=\300\016 details need=\300\b budget=\300\b margin=\300\004 fs=\300\004 fc=\300\b stride=\300\n boundary=\300\t control=\300\t enforce=\300\001\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.101,unique,196
anon.22844a9aef6a14b72687ebf27443efb5.101:
	.ascii	"rustc"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.103,unique,197
anon.22844a9aef6a14b72687ebf27443efb5.103:
	.ascii	"1.93"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.104,unique,198
anon.22844a9aef6a14b72687ebf27443efb5.104:
	.asciz	"\300\020 (msrv fallback)"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.105,unique,199
anon.22844a9aef6a14b72687ebf27443efb5.105:
	.asciz	"\022[b4-30] toolchain=\300\004 os=\300\t profile=\300\001\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.106,unique,200
anon.22844a9aef6a14b72687ebf27443efb5.106:
	.asciz	"(confirmation probe at measured boundary \300\013 overflowed"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.107,unique,201
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.107:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\251\002\000\000\033\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.108,unique,202
anon.22844a9aef6a14b72687ebf27443efb5.108:
	.asciz	".b4-30 child ended with an unclassified status \300\002: \300"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.109,unique,203
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.109:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\323\001\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.110,unique,204
anon.22844a9aef6a14b72687ebf27443efb5.110:
	.ascii	"assertion failed: !p.is_null()"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.111,unique,205
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.111:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\327\000\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.112,unique,206
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.112:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\315\000\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.113,unique,207
anon.22844a9aef6a14b72687ebf27443efb5.113:
	.ascii	"foreign-free"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.114,unique,208
anon.22844a9aef6a14b72687ebf27443efb5.114:
	.ascii	"unknown scenario"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.115,unique,209
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.115:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\347\000\000\000\016\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.116,unique,210
anon.22844a9aef6a14b72687ebf27443efb5.116:
	.asciz	"\021two-point stride \300! != child-observed stride_actual \300"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.117,unique,211
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.117:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\016\002\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.118,unique,212
anon.22844a9aef6a14b72687ebf27443efb5.118:
	.asciz	"\021eat frame stride \300Y B > 512 B: protocol assumption broken, recalibrate the budgets before trusting this test"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.119,unique,213
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.119:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\023\002\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.120,unique,214
anon.22844a9aef6a14b72687ebf27443efb5.120:
	.ascii	"calibration depth 9 overflowed"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.121,unique,215
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.121:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\013\002\000\000\033\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.122,unique,216
anon.22844a9aef6a14b72687ebf27443efb5.122:
	.ascii	"calibration depth 8 overflowed"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.123,unique,217
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.123:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\007\002\000\000\033\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.124,unique,218
anon.22844a9aef6a14b72687ebf27443efb5.124:
	.ascii	"scenario"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.125,unique,219
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.125:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\361\000\000\000\"\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.126,unique,220
anon.22844a9aef6a14b72687ebf27443efb5.126:
	.ascii	"depth"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.127,unique,221
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.127:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\364\000\000\000)\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.128,unique,222
anon.22844a9aef6a14b72687ebf27443efb5.128:
	.ascii	"integer"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.129,unique,223
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.129:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\364\000\000\000A\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.130,unique,224
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.130:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\377\000\000\000\t\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.131,unique,225
anon.22844a9aef6a14b72687ebf27443efb5.131:
	.ascii	"unnamed worker"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.132,unique,226
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.132:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000Q\001\000\000\n\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.133,unique,227
anon.22844a9aef6a14b72687ebf27443efb5.133:
	.ascii	"worker panic"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.134,unique,228
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.134:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000w\001\000\000\023\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.135,unique,229
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.135:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000~\001\000\000\t\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.136,unique,230
anon.22844a9aef6a14b72687ebf27443efb5.136:
	.ascii	"registry claim high-water delta"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.137,unique,231
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.137:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\210\001\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.138,unique,232
anon.22844a9aef6a14b72687ebf27443efb5.138:
	.ascii	"binding oracle mismatch"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.139,unique,233
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.139:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.19
	.asciz	"$\000\000\000\000\000\000\000\214\001\000\000\005\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.140,unique,234
anon.22844a9aef6a14b72687ebf27443efb5.140:
	.asciz	"\r{\"scenario\":\"\300\n\",\"depth\":\300\016,\"entry_addr\":\300\r,\"deep_addr\":\300\r,\"stack_low\":\300\016,\"stack_high\":\300\017,\"guard_bytes\":\300\n,\"result\":\300\021,\"heapcore_size\":\300\021,\"stride_actual\":\300\030,\"fallback_lock_before\":\300\027,\"fallback_lock_after\":\300\020,\"claim_before\":\300\027,\"claim_worker_before\":\300\026,\"claim_worker_after\":\300\017,\"claim_delta\":\300\021,\"ctor_observed\":\300\026,\"claim_sources_flat\":\300\002}\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.141,unique,235
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.141:
	.quad	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtNtCsaZOKLwlFm27_3std2io5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.asciz	"\b\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_RNvXNtNtCsaZOKLwlFm27_3std2io5errorNtB2_5ErrorNtNtCsd9eshbB3hfz_4core3fmt5Debug3fmt

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.142,unique,236
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.142:
	.quad	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsaZOKLwlFm27_3std3env8VarErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.asciz	" \000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_RNvXsk_NtCsaZOKLwlFm27_3std3envNtB5_8VarErrorNtNtCsd9eshbB3hfz_4core3fmt5Debug3fmt

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.143,unique,237
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.143:
	.quad	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueNtNtCsfZWMcdrZHWk_10serde_json5error5ErrorECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.asciz	"\b\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_RNvXs5_NtCsfZWMcdrZHWk_10serde_json5errorNtB5_5ErrorNtNtCsd9eshbB3hfz_4core3fmt5Debug3fmt

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.144,unique,238
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.144:
	.asciz	"\000\000\000\000\000\000\000\000\000\000\000\000\000\000\000\000\001\000\000\000\000\000\000"
	.quad	_RNvXsd_NtNtCsd9eshbB3hfz_4core5alloc6layoutNtB5_11LayoutErrorNtNtB9_3fmt5Debug3fmt

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.145,unique,239
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.145:
	.asciz	"\000\000\000\000\000\000\000\000\001\000\000\000\000\000\000\000\001\000\000\000\000\000\000"
	.quad	_RNvXsd_NtNtCsd9eshbB3hfz_4core3num5errorNtB5_13ParseIntErrorNtNtB9_3fmt5Debug3fmt

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.146,unique,240
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.146:
	.quad	_RINvNtCsd9eshbB3hfz_4core3ptr9drop_glueINtNtCsdkPGTeTFVes_5alloc5boxed3BoxDNtNtB4_3any3AnyNtNtB4_6marker4SendEL_EECs8fXedXvU3Hn_27b4_30_first_alloc_min_stack
	.asciz	"\020\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_RNvXsn_NtCsdkPGTeTFVes_5alloc5boxedINtB5_3BoxDNtNtCsd9eshbB3hfz_4core3any3AnyNtNtBM_6marker4SendEL_ENtNtBM_3fmt5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.147,unique,241
anon.22844a9aef6a14b72687ebf27443efb5.147:
	.asciz	"src\\alloc_core\\small\\alloc_core_small_magazine.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.148,unique,242
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.148:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.147
	.asciz	"1\000\000\000\000\000\000\0008\002\000\000O\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.149,unique,243
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.149:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.147
	.asciz	"1\000\000\000\000\000\000\0005\002\000\0002\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.150,unique,244
anon.22844a9aef6a14b72687ebf27443efb5.150:
	.ascii	"segment_header_meta_fields: sub_live live_count underflow\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.151,unique,245
anon.22844a9aef6a14b72687ebf27443efb5.151:
	.asciz	"src\\alloc_core\\segment\\segment_directory\\segment_directory_impl.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.152,unique,246
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.152:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.151
	.asciz	"B\000\000\000\000\000\000\000\301\001\000\000\031\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.153,unique,247
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.153:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.151
	.asciz	"B\000\000\000\000\000\000\000Y\002\000\000!\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.154,unique,248
anon.22844a9aef6a14b72687ebf27443efb5.154:
	.asciz	"src\\registry\\heap_core\\free\\dealloc_own_base.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.155,unique,249
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.155:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.154
	.asciz	"/\000\000\000\000\000\000\000\277\001\000\000\037\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.156,unique,250
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.156:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.154
	.asciz	"/\000\000\000\000\000\000\000#\002\000\000'\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.157.llvm.5592491244291964665,unique,251
	.globl	anon.22844a9aef6a14b72687ebf27443efb5.157.llvm.5592491244291964665
anon.22844a9aef6a14b72687ebf27443efb5.157.llvm.5592491244291964665:
	.asciz	"src\\registry\\heap_core\\alloc\\hot.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665,unique,252
	.globl	anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.157.llvm.5592491244291964665
	.asciz	"#\000\000\000\000\000\000\000\357\000\000\000\037\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.159,unique,253
anon.22844a9aef6a14b72687ebf27443efb5.159:
	.ascii	"threads should not terminate unexpectedly"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.160,unique,254
anon.22844a9aef6a14b72687ebf27443efb5.160:
	.asciz	"<rust-toolchain-dir>\\toolchains\\stable-x86_64-pc-windows-msvc\\lib/rustlib/src/rust\\library\\std\\src\\thread\\lifecycle.rs"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.161,unique,255
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.161:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.160
	.asciz	"{\000\000\000\000\000\000\000\342\000\000\000\016\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.162,unique,256
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.162:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.160
	.asciz	"{\000\000\000\000\000\000\000\346\000\000\000\016\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.163,unique,257
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.163:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.4
	.asciz	"\203\000\000\000\000\000\000\000\320\004\000\000#\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.164,unique,258
anon.22844a9aef6a14b72687ebf27443efb5.164:
	.ascii	"assertion failed: edge.height == self.height - 1"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.165,unique,259
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.165:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.4
	.asciz	"\203\000\000\000\000\000\000\000\266\002\000\000\t\000\000"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.166,unique,260
anon.22844a9aef6a14b72687ebf27443efb5.166:
	.ascii	"fatal runtime error: thread result panicked on drop, aborting\n"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.167,unique,261
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.167:
	.asciz	"\000\000\000\000\000\000\000\000\b\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.168,unique,262
anon.22844a9aef6a14b72687ebf27443efb5.168:
	.ascii	"None"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.170,unique,263
anon.22844a9aef6a14b72687ebf27443efb5.170:
	.ascii	"Some"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.171,unique,264
anon.22844a9aef6a14b72687ebf27443efb5.171:
	.ascii	"ParseIntError"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.172,unique,265
anon.22844a9aef6a14b72687ebf27443efb5.172:
	.ascii	"kind"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.173,unique,266
anon.22844a9aef6a14b72687ebf27443efb5.173:
	.ascii	"LayoutError"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.174,unique,267
anon.22844a9aef6a14b72687ebf27443efb5.174:
	.ascii	"Empty"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.175,unique,268
anon.22844a9aef6a14b72687ebf27443efb5.175:
	.ascii	"InvalidDigit"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.176,unique,269
anon.22844a9aef6a14b72687ebf27443efb5.176:
	.ascii	"PosOverflow"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.177,unique,270
anon.22844a9aef6a14b72687ebf27443efb5.177:
	.ascii	"NegOverflow"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.178,unique,271
anon.22844a9aef6a14b72687ebf27443efb5.178:
	.ascii	"Zero"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.179,unique,272
anon.22844a9aef6a14b72687ebf27443efb5.179:
	.ascii	"NotAPowerOfTwo"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.180,unique,273
anon.22844a9aef6a14b72687ebf27443efb5.180:
	.ascii	"NotPresent"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.182,unique,274
anon.22844a9aef6a14b72687ebf27443efb5.182:
	.ascii	"NotUnicode"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.183,unique,275
anon.22844a9aef6a14b72687ebf27443efb5.183:
	.ascii	"failed to write whole buffer"

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.184,unique,276
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.184:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.183
	.ascii	"\034\000\000\000\000\000\000\000\027"
	.zero	7

	.section	.rdata,"dr",one_only,anon.22844a9aef6a14b72687ebf27443efb5.185,unique,277
	.p2align	3, 0x0
anon.22844a9aef6a14b72687ebf27443efb5.185:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.9
	.asciz	"q\000\000\000\000\000\000\000H\007\000\000$\000\000"

	.section	.data,"dw",discard,_RNvCs9hJ03s5DiqP_7___rustc22___rust_panic_type_info,unique,278
	.globl	_RNvCs9hJ03s5DiqP_7___rustc22___rust_panic_type_info
	.p2align	4, 0x0
_RNvCs9hJ03s5DiqP_7___rustc22___rust_panic_type_info:
	.quad	"??_7type_info@@6B@"
	.quad	0
	.asciz	"rust_panic"
	.zero	5

	.section	.rdata,"dr",one_only,switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack,unique,279
	.p2align	3, 0x0
switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack:
	.quad	5
	.quad	12
	.quad	11
	.quad	11
	.quad	4
	.quad	14

	.section	.rdata,"dr",one_only,switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack.138.rel,unique,280
	.p2align	2, 0x0
switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack.138.rel:
	.long	anon.22844a9aef6a14b72687ebf27443efb5.174-switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack.138.rel
	.long	anon.22844a9aef6a14b72687ebf27443efb5.175-switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack.138.rel
	.long	anon.22844a9aef6a14b72687ebf27443efb5.176-switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack.138.rel
	.long	anon.22844a9aef6a14b72687ebf27443efb5.177-switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack.138.rel
	.long	anon.22844a9aef6a14b72687ebf27443efb5.178-switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack.138.rel
	.long	anon.22844a9aef6a14b72687ebf27443efb5.179-switch.table._RNvXs1g_NtCsd9eshbB3hfz_4core3fmtRNtNtNtB8_3num5error12IntErrorKindNtB6_5Debug3fmtCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack.138.rel

	.section	.data,"dw",one_only,__imp__RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665,unique,281
	.globl	__imp__RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665
	.p2align	3, 0x0
__imp__RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665:
	.quad	_RNvCs8fXedXvU3Hn_27b4_30_first_alloc_min_stack6GLOBAL.llvm.5592491244291964665

	.section	.data,"dw",one_only,__imp_anon.22844a9aef6a14b72687ebf27443efb5.157.llvm.5592491244291964665,unique,282
	.globl	__imp_anon.22844a9aef6a14b72687ebf27443efb5.157.llvm.5592491244291964665
	.p2align	3, 0x0
__imp_anon.22844a9aef6a14b72687ebf27443efb5.157.llvm.5592491244291964665:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.157.llvm.5592491244291964665

	.section	.data,"dw",one_only,__imp_anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665,unique,283
	.globl	__imp_anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665
	.p2align	3, 0x0
__imp_anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665:
	.quad	anon.22844a9aef6a14b72687ebf27443efb5.158.llvm.5592491244291964665

	.section	.debug$S,"dr"
	.p2align	2, 0x0
	.long	4
	.long	241
	.long	.Ltmp353-.Ltmp352
.Ltmp352:
	.short	.Ltmp355-.Ltmp354
.Ltmp354:
	.short	4353
	.long	0
	.asciz	"<target-dir>\\release\\deps\\b4_30_first_alloc_min_stack-7cab0293290de0af.b4_30_first_alloc_min_stack.602daaf8bd80a59f-cgu.0.rcgu.o"
	.p2align	2, 0x0
.Ltmp355:
	.short	.Ltmp357-.Ltmp356
.Ltmp356:
	.short	4412
	.long	21
	.short	208
	.short	1
	.short	97
	.short	0
	.short	0
	.short	22016
	.short	0
	.short	0
	.short	0
	.asciz	"clang LLVM (rustc version 1.97.0 (2d8144b78 2026-07-07))"
	.p2align	2, 0x0
.Ltmp357:
.Ltmp353:
	.p2align	2, 0x0
	.cv_filechecksums
	.cv_stringtable
	.long	241
	.long	.Ltmp359-.Ltmp358
.Ltmp358:
	.short	.Ltmp361-.Ltmp360
.Ltmp360:
	.short	4428
	.long	4099
	.p2align	2, 0x0
.Ltmp361:
.Ltmp359:
	.p2align	2, 0x0
	.section	.debug$T,"dr"
	.p2align	2, 0x0
	.long	4
	.short	0x36
	.short	0x1605
	.long	0x0
	.asciz	"/rustc/2d8144b7880597b6e6d3dfd63a9a9efae3f533d3"
	.short	0x6e
	.short	0x1605
	.long	0x0
	.asciz	"/rustc/2d8144b7880597b6e6d3dfd63a9a9efae3f533d3/library\\alloc\\src\\lib.rs\\@\\alloc.9b553d5393fa9e8e-cgu.0"
	.short	0xa
	.short	0x1605
	.long	0x0
	.byte	0
	.byte	243
	.byte	242
	.byte	241
	.short	0x1a
	.short	0x1603
	.short	0x5
	.long	0x1000
	.long	0x1002
	.long	0x1001
	.long	0x1002
	.long	0x1002
	.byte	242
	.byte	241
	.globl	_fltused
