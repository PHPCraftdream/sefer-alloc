000000000004cc30 <perf_gate_iai::alloc_clear_magazine_only_16b::wrapper>:
   4cc30:	50                   	push   %rax
   4cc31:	e8 ea ce 00 00       	call   59b20 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b>
   4cc36:	58                   	pop    %rax
   4cc37:	c3                   	ret
   4cc38:	cc                   	int3
   4cc39:	cc                   	int3
   4cc3a:	cc                   	int3
   4cc3b:	cc                   	int3
   4cc3c:	cc                   	int3
   4cc3d:	cc                   	int3
   4cc3e:	cc                   	int3
   4cc3f:	cc                   	int3


000000000004f4e0 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::wrapper>:
   4f4e0:	50                   	push   %rax
   4f4e1:	e8 3a f0 00 00       	call   5e520 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix>
   4f4e6:	58                   	pop    %rax
   4f4e7:	c3                   	ret
   4f4e8:	cc                   	int3
   4f4e9:	cc                   	int3
   4f4ea:	cc                   	int3
   4f4eb:	cc                   	int3
   4f4ec:	cc                   	int3
   4f4ed:	cc                   	int3
   4f4ee:	cc                   	int3
   4f4ef:	cc                   	int3


0000000000059b20 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b>:
   59b20:	55                   	push   %rbp
   59b21:	41 57                	push   %r15
   59b23:	41 56                	push   %r14
   59b25:	41 55                	push   %r13
   59b27:	41 54                	push   %r12
   59b29:	53                   	push   %rbx
   59b2a:	48 81 ec a8 00 00 00 	sub    $0xa8,%rsp
   59b31:	4c 8d 7c 24 20       	lea    0x20(%rsp),%r15
   59b36:	4c 89 ff             	mov    %r15,%rdi
   59b39:	ff 15 a9 3c 07 00    	call   *0x73ca9(%rip)        # cd7e8 <_DYNAMIC+0x6c8>
   59b3f:	4c 8b 64 24 20       	mov    0x20(%rsp),%r12
   59b44:	4d 85 e4             	test   %r12,%r12
   59b47:	0f 84 2e 06 00 00    	je     5a17b <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x65b>
   59b4d:	0f 57 c0             	xorps  %xmm0,%xmm0
   59b50:	0f 29 84 24 90 00 00 	movaps %xmm0,0x90(%rsp)
   59b57:	00 
   59b58:	0f 29 84 24 80 00 00 	movaps %xmm0,0x80(%rsp)
   59b5f:	00 
   59b60:	0f 29 44 24 70       	movaps %xmm0,0x70(%rsp)
   59b65:	0f 29 44 24 60       	movaps %xmm0,0x60(%rsp)
   59b6a:	0f 29 44 24 50       	movaps %xmm0,0x50(%rsp)
   59b6f:	0f 29 44 24 40       	movaps %xmm0,0x40(%rsp)
   59b74:	0f 29 44 24 30       	movaps %xmm0,0x30(%rsp)
   59b79:	0f 29 44 24 20       	movaps %xmm0,0x20(%rsp)
   59b7e:	31 db                	xor    %ebx,%ebx
   59b80:	4c 8b 35 01 3d 07 00 	mov    0x73d01(%rip),%r14        # cd888 <_DYNAMIC+0x768>
   59b87:	66 0f 1f 84 00 00 00 	nopw   0x0(%rax,%rax,1)
   59b8e:	00 00 
   59b90:	41 0f b6 84 24 c0 04 	movzbl 0x4c0(%r12),%eax
   59b97:	00 00 
   59b99:	48 85 c0             	test   %rax,%rax
   59b9c:	74 47                	je     59be5 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0xc5>
   59b9e:	48 ff c8             	dec    %rax
   59ba1:	41 88 84 24 c0 04 00 	mov    %al,0x4c0(%r12)
   59ba8:	00 
   59ba9:	83 e0 0f             	and    $0xf,%eax
   59bac:	49 8b 84 c4 c8 04 00 	mov    0x4c8(%r12,%rax,8),%rax
   59bb3:	00 
   59bb4:	48 89 c1             	mov    %rax,%rcx
   59bb7:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59bbe:	89 c2                	mov    %eax,%edx
   59bc0:	c1 ea 04             	shr    $0x4,%edx
   59bc3:	81 e2 ff ff 03 00    	and    $0x3ffff,%edx
   59bc9:	80 a4 11 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rdx,1)
   59bd0:	3f 
   59bd1:	48 89 44 1c 20       	mov    %rax,0x20(%rsp,%rbx,1)
   59bd6:	48 83 c3 08          	add    $0x8,%rbx
   59bda:	48 81 fb 80 00 00 00 	cmp    $0x80,%rbx
   59be1:	75 ad                	jne    59b90 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x70>
   59be3:	eb 1c                	jmp    59c01 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0xe1>
   59be5:	49 8d 7c 24 10       	lea    0x10(%r12),%rdi
   59bea:	31 f6                	xor    %esi,%esi
   59bec:	41 ff d6             	call   *%r14
   59bef:	48 89 44 1c 20       	mov    %rax,0x20(%rsp,%rbx,1)
   59bf4:	48 83 c3 08          	add    $0x8,%rbx
   59bf8:	48 81 fb 80 00 00 00 	cmp    $0x80,%rbx
   59bff:	75 8f                	jne    59b90 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x70>
   59c01:	4c 89 7c 24 08       	mov    %r15,0x8(%rsp)
   59c06:	48 8d 44 24 08       	lea    0x8(%rsp),%rax
   59c0b:	4d 8d 6c 24 40       	lea    0x40(%r12),%r13
   59c10:	45 31 ff             	xor    %r15d,%r15d
   59c13:	4c 8b 1d e6 3b 07 00 	mov    0x73be6(%rip),%r11        # cd800 <_DYNAMIC+0x6e0>
   59c1a:	48 8b 2d ff 3b 07 00 	mov    0x73bff(%rip),%rbp        # cd820 <_DYNAMIC+0x700>
   59c21:	eb 23                	jmp    59c46 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x126>
   59c23:	66 66 66 66 2e 0f 1f 	data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   59c2a:	84 00 00 00 00 00 
   59c30:	f0 48 ff 45 00       	lock incq 0x0(%rbp)
   59c35:	49 83 c7 08          	add    $0x8,%r15
   59c39:	49 81 ff 80 00 00 00 	cmp    $0x80,%r15
   59c40:	0f 84 c8 02 00 00    	je     59f0e <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x3ee>
   59c46:	4a 8b 7c 3c 20       	mov    0x20(%rsp,%r15,1),%rdi
   59c4b:	48 85 ff             	test   %rdi,%rdi
   59c4e:	74 e5                	je     59c35 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x115>
   59c50:	48 89 f8             	mov    %rdi,%rax
   59c53:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   59c59:	49 39 84 24 40 01 00 	cmp    %rax,0x140(%r12)
   59c60:	00 
   59c61:	75 1d                	jne    59c80 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x160>
   59c63:	f0 49 ff 03          	lock incq (%r11)
   59c67:	4d 8b 84 24 48 01 00 	mov    0x148(%r12),%r8
   59c6e:	00 
   59c6f:	41 0f b6 48 1c       	movzbl 0x1c(%r8),%ecx
   59c74:	83 f9 02             	cmp    $0x2,%ecx
   59c77:	74 55                	je     59cce <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x1ae>
   59c79:	e9 4b 01 00 00       	jmp    59dc9 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x2a9>
   59c7e:	66 90                	xchg   %ax,%ax
   59c80:	48 89 c2             	mov    %rax,%rdx
   59c83:	48 c1 ea 16          	shr    $0x16,%rdx
   59c87:	89 d1                	mov    %edx,%ecx
   59c89:	83 e1 0f             	and    $0xf,%ecx
   59c8c:	c1 e1 04             	shl    $0x4,%ecx
   59c8f:	49 39 44 0d 00       	cmp    %rax,0x0(%r13,%rcx,1)
   59c94:	0f 85 81 00 00 00    	jne    59d1b <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x1fb>
   59c9a:	4d 8b 44 0d 08       	mov    0x8(%r13,%rcx,1),%r8
   59c9f:	f0 49 ff 03          	lock incq (%r11)
   59ca3:	49 39 c0             	cmp    %rax,%r8
   59ca6:	75 18                	jne    59cc0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x1a0>
   59ca8:	49 89 84 24 40 01 00 	mov    %rax,0x140(%r12)
   59caf:	00 
   59cb0:	4d 89 84 24 48 01 00 	mov    %r8,0x148(%r12)
   59cb7:	00 
   59cb8:	0f 1f 84 00 00 00 00 	nopl   0x0(%rax,%rax,1)
   59cbf:	00 
   59cc0:	41 0f b6 48 1c       	movzbl 0x1c(%r8),%ecx
   59cc5:	83 f9 02             	cmp    $0x2,%ecx
   59cc8:	0f 85 fb 00 00 00    	jne    59dc9 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x2a9>
   59cce:	48 89 fe             	mov    %rdi,%rsi
   59cd1:	4c 29 c6             	sub    %r8,%rsi
   59cd4:	49 3b 70 38          	cmp    0x38(%r8),%rsi
   59cd8:	0f 83 80 01 00 00    	jae    59e5e <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x33e>
   59cde:	49 8b 40 30          	mov    0x30(%r8),%rax
   59ce2:	49 8b 48 08          	mov    0x8(%r8),%rcx
   59ce6:	4c 01 c0             	add    %r8,%rax
   59ce9:	0f 82 41 ff ff ff    	jb     59c30 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   59cef:	48 39 f8             	cmp    %rdi,%rax
   59cf2:	0f 85 38 ff ff ff    	jne    59c30 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   59cf8:	4c 01 c6             	add    %r8,%rsi
   59cfb:	ba 08 00 00 00       	mov    $0x8,%edx
   59d00:	b9 10 00 00 00       	mov    $0x10,%ecx
   59d05:	49 8d 7c 24 10       	lea    0x10(%r12),%rdi
   59d0a:	4c 89 db             	mov    %r11,%rbx
   59d0d:	ff 15 05 3b 07 00    	call   *0x73b05(%rip)        # cd818 <_DYNAMIC+0x6f8>
   59d13:	49 89 db             	mov    %rbx,%r11
   59d16:	e9 1a ff ff ff       	jmp    59c35 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x115>
   59d1b:	4c 01 e9             	add    %r13,%rcx
   59d1e:	48 8b 35 eb 3a 07 00 	mov    0x73aeb(%rip),%rsi        # cd810 <_DYNAMIC+0x6f0>
   59d25:	f0 48 ff 06          	lock incq (%rsi)
   59d29:	48 89 c6             	mov    %rax,%rsi
   59d2c:	48 c1 ee 23          	shr    $0x23,%rsi
   59d30:	49 89 c0             	mov    %rax,%r8
   59d33:	49 c1 e8 30          	shr    $0x30,%r8
   59d37:	44 31 c6             	xor    %r8d,%esi
   59d3a:	31 f2                	xor    %esi,%edx
   59d3c:	81 e2 ff 1f 00 00    	and    $0x1fff,%edx
   59d42:	49 8b b4 24 50 01 00 	mov    0x150(%r12),%rsi
   59d49:	00 
   59d4a:	49 89 d1             	mov    %rdx,%r9
   59d4d:	0f 1f 00             	nopl   (%rax)
   59d50:	4e 8b 04 ce          	mov    (%rsi,%r9,8),%r8
   59d54:	4d 85 c0             	test   %r8,%r8
   59d57:	0f 84 01 01 00 00    	je     59e5e <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x33e>
   59d5d:	4d 89 c2             	mov    %r8,%r10
   59d60:	49 81 e2 00 00 c0 ff 	and    $0xffffffffffc00000,%r10
   59d67:	49 39 c2             	cmp    %rax,%r10
   59d6a:	74 14                	je     59d80 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x260>
   59d6c:	41 ff c1             	inc    %r9d
   59d6f:	41 81 e1 ff 1f 00 00 	and    $0x1fff,%r9d
   59d76:	49 39 d1             	cmp    %rdx,%r9
   59d79:	75 d5                	jne    59d50 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x230>
   59d7b:	e9 de 00 00 00       	jmp    59e5e <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x33e>
   59d80:	4c 89 c2             	mov    %r8,%rdx
   59d83:	48 81 e2 ff ff 3f 00 	and    $0x3fffff,%rdx
   59d8a:	74 28                	je     59db4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x294>
   59d8c:	48 8d 72 ff          	lea    -0x1(%rdx),%rsi
   59d90:	45 8b 84 24 68 01 00 	mov    0x168(%r12),%r8d
   59d97:	00 
   59d98:	4c 39 c6             	cmp    %r8,%rsi
   59d9b:	0f 83 bd 00 00 00    	jae    59e5e <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x33e>
   59da1:	49 8b 74 24 30       	mov    0x30(%r12),%rsi
   59da6:	4c 8b 44 d6 f8       	mov    -0x8(%rsi,%rdx,8),%r8
   59dab:	4d 85 c0             	test   %r8,%r8
   59dae:	0f 84 aa 00 00 00    	je     59e5e <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x33e>
   59db4:	48 89 01             	mov    %rax,(%rcx)
   59db7:	4c 89 41 08          	mov    %r8,0x8(%rcx)
   59dbb:	41 0f b6 48 1c       	movzbl 0x1c(%r8),%ecx
   59dc0:	83 f9 02             	cmp    $0x2,%ecx
   59dc3:	0f 84 05 ff ff ff    	je     59cce <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x1ae>
   59dc9:	41 0f b6 84 24 c0 04 	movzbl 0x4c0(%r12),%eax
   59dd0:	00 00 
   59dd2:	85 c9                	test   %ecx,%ecx
   59dd4:	74 20                	je     59df6 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x2d6>
   59dd6:	83 f9 01             	cmp    $0x1,%ecx
   59dd9:	0f 85 51 fe ff ff    	jne    59c30 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   59ddf:	b9 00 20 04 00       	mov    $0x42000,%ecx
   59de4:	89 fb                	mov    %edi,%ebx
   59de6:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   59dec:	39 cb                	cmp    %ecx,%ebx
   59dee:	0f 82 3c fe ff ff    	jb     59c30 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   59df4:	eb 15                	jmp    59e0b <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x2eb>
   59df6:	b9 00 f0 05 00       	mov    $0x5f000,%ecx
   59dfb:	89 fb                	mov    %edi,%ebx
   59dfd:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   59e03:	39 cb                	cmp    %ecx,%ebx
   59e05:	0f 82 25 fe ff ff    	jb     59c30 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   59e0b:	40 f6 c7 0f          	test   $0xf,%dil
   59e0f:	0f 85 20 fe ff ff    	jne    59c35 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x115>
   59e15:	41 89 de             	mov    %ebx,%r14d
   59e18:	41 c1 ee 04          	shr    $0x4,%r14d
   59e1c:	43 0f b6 8c 30 88 15 	movzbl 0x1588(%r8,%r14,1),%ecx
   59e23:	00 00 
   59e25:	85 c9                	test   %ecx,%ecx
   59e27:	74 50                	je     59e79 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x359>
   59e29:	83 f9 01             	cmp    $0x1,%ecx
   59e2c:	75 59                	jne    59e87 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x367>
   59e2e:	49 3b 18             	cmp    (%r8),%rbx
   59e31:	0f 83 f9 fd ff ff    	jae    59c30 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   59e37:	4c 01 c3             	add    %r8,%rbx
   59e3a:	3c 0f                	cmp    $0xf,%al
   59e3c:	77 57                	ja     59e95 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x375>
   59e3e:	43 c6 84 30 88 15 00 	movb   $0x41,0x1588(%r8,%r14,1)
   59e45:	00 41 
   59e47:	49 89 9c c4 c8 04 00 	mov    %rbx,0x4c8(%r12,%rax,8)
   59e4e:	00 
   59e4f:	ff c0                	inc    %eax
   59e51:	41 88 84 24 c0 04 00 	mov    %al,0x4c0(%r12)
   59e58:	00 
   59e59:	e9 d7 fd ff ff       	jmp    59c35 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x115>
   59e5e:	be 08 00 00 00       	mov    $0x8,%esi
   59e63:	ba 10 00 00 00       	mov    $0x10,%edx
   59e68:	4c 89 db             	mov    %r11,%rbx
   59e6b:	ff 15 bf 39 07 00    	call   *0x739bf(%rip)        # cd830 <_DYNAMIC+0x710>
   59e71:	49 89 db             	mov    %rbx,%r11
   59e74:	e9 bc fd ff ff       	jmp    59c35 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x115>
   59e79:	49 3b 18             	cmp    (%r8),%rbx
   59e7c:	0f 83 ae fd ff ff    	jae    59c30 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   59e82:	e9 ae fd ff ff       	jmp    59c35 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x115>
   59e87:	80 f9 40             	cmp    $0x40,%cl
   59e8a:	0f 83 a0 fd ff ff    	jae    59c30 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   59e90:	e9 a0 fd ff ff       	jmp    59c35 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x115>
   59e95:	b9 08 00 00 00       	mov    $0x8,%ecx
   59e9a:	49 8d 7c 24 10       	lea    0x10(%r12),%rdi
   59e9f:	31 f6                	xor    %esi,%esi
   59ea1:	49 8d 94 24 c8 04 00 	lea    0x4c8(%r12),%rdx
   59ea8:	00 
   59ea9:	48 89 54 24 10       	mov    %rdx,0x10(%rsp)
   59eae:	4c 89 44 24 18       	mov    %r8,0x18(%rsp)
   59eb3:	e8 e8 0f ff ff       	call   4aea0 <<sefer_alloc::alloc_core::alloc_core::alloc_core_impl::AllocCore>::flush_class_internal>
   59eb8:	4c 8b 1d 41 39 07 00 	mov    0x73941(%rip),%r11        # cd800 <_DYNAMIC+0x6e0>
   59ebf:	49 8d 84 24 08 05 00 	lea    0x508(%r12),%rax
   59ec6:	00 
   59ec7:	0f 10 00             	movups (%rax),%xmm0
   59eca:	0f 10 48 10          	movups 0x10(%rax),%xmm1
   59ece:	0f 10 50 20          	movups 0x20(%rax),%xmm2
   59ed2:	0f 10 58 30          	movups 0x30(%rax),%xmm3
   59ed6:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   59edb:	0f 11 58 30          	movups %xmm3,0x30(%rax)
   59edf:	0f 11 50 20          	movups %xmm2,0x20(%rax)
   59ee3:	0f 11 48 10          	movups %xmm1,0x10(%rax)
   59ee7:	0f 11 00             	movups %xmm0,(%rax)
   59eea:	48 8b 44 24 18       	mov    0x18(%rsp),%rax
   59eef:	42 c6 84 30 88 15 00 	movb   $0x41,0x1588(%rax,%r14,1)
   59ef6:	00 41 
   59ef8:	49 89 9c 24 08 05 00 	mov    %rbx,0x508(%r12)
   59eff:	00 
   59f00:	41 c6 84 24 c0 04 00 	movb   $0x9,0x4c0(%r12)
   59f07:	00 09 
   59f09:	e9 27 fd ff ff       	jmp    59c35 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x115>
   59f0e:	48 8b 44 24 20       	mov    0x20(%rsp),%rax
   59f13:	48 85 c0             	test   %rax,%rax
   59f16:	74 1a                	je     59f32 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x412>
   59f18:	48 89 c1             	mov    %rax,%rcx
   59f1b:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59f22:	c1 e8 04             	shr    $0x4,%eax
   59f25:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59f2a:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59f31:	3f 
   59f32:	48 8b 44 24 28       	mov    0x28(%rsp),%rax
   59f37:	48 85 c0             	test   %rax,%rax
   59f3a:	48 8d 54 24 20       	lea    0x20(%rsp),%rdx
   59f3f:	74 1a                	je     59f5b <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x43b>
   59f41:	48 89 c1             	mov    %rax,%rcx
   59f44:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59f4b:	c1 e8 04             	shr    $0x4,%eax
   59f4e:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59f53:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59f5a:	3f 
   59f5b:	48 8b 44 24 30       	mov    0x30(%rsp),%rax
   59f60:	48 85 c0             	test   %rax,%rax
   59f63:	74 1a                	je     59f7f <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x45f>
   59f65:	48 89 c1             	mov    %rax,%rcx
   59f68:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59f6f:	c1 e8 04             	shr    $0x4,%eax
   59f72:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59f77:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59f7e:	3f 
   59f7f:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   59f84:	48 85 c0             	test   %rax,%rax
   59f87:	74 1a                	je     59fa3 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x483>
   59f89:	48 89 c1             	mov    %rax,%rcx
   59f8c:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59f93:	c1 e8 04             	shr    $0x4,%eax
   59f96:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59f9b:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59fa2:	3f 
   59fa3:	48 8b 44 24 40       	mov    0x40(%rsp),%rax
   59fa8:	48 85 c0             	test   %rax,%rax
   59fab:	74 1a                	je     59fc7 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x4a7>
   59fad:	48 89 c1             	mov    %rax,%rcx
   59fb0:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59fb7:	c1 e8 04             	shr    $0x4,%eax
   59fba:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59fbf:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59fc6:	3f 
   59fc7:	48 8b 44 24 48       	mov    0x48(%rsp),%rax
   59fcc:	48 85 c0             	test   %rax,%rax
   59fcf:	74 1a                	je     59feb <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x4cb>
   59fd1:	48 89 c1             	mov    %rax,%rcx
   59fd4:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59fdb:	c1 e8 04             	shr    $0x4,%eax
   59fde:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59fe3:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59fea:	3f 
   59feb:	48 8b 44 24 50       	mov    0x50(%rsp),%rax
   59ff0:	48 85 c0             	test   %rax,%rax
   59ff3:	74 1a                	je     5a00f <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x4ef>
   59ff5:	48 89 c1             	mov    %rax,%rcx
   59ff8:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59fff:	c1 e8 04             	shr    $0x4,%eax
   5a002:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a007:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a00e:	3f 
   5a00f:	48 8b 44 24 58       	mov    0x58(%rsp),%rax
   5a014:	48 85 c0             	test   %rax,%rax
   5a017:	74 1a                	je     5a033 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x513>
   5a019:	48 89 c1             	mov    %rax,%rcx
   5a01c:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5a023:	c1 e8 04             	shr    $0x4,%eax
   5a026:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a02b:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a032:	3f 
   5a033:	48 8b 44 24 60       	mov    0x60(%rsp),%rax
   5a038:	48 85 c0             	test   %rax,%rax
   5a03b:	74 1a                	je     5a057 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x537>
   5a03d:	48 89 c1             	mov    %rax,%rcx
   5a040:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5a047:	c1 e8 04             	shr    $0x4,%eax
   5a04a:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a04f:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a056:	3f 
   5a057:	48 8b 44 24 68       	mov    0x68(%rsp),%rax
   5a05c:	48 85 c0             	test   %rax,%rax
   5a05f:	74 1a                	je     5a07b <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x55b>
   5a061:	48 89 c1             	mov    %rax,%rcx
   5a064:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5a06b:	c1 e8 04             	shr    $0x4,%eax
   5a06e:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a073:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a07a:	3f 
   5a07b:	48 8b 44 24 70       	mov    0x70(%rsp),%rax
   5a080:	48 85 c0             	test   %rax,%rax
   5a083:	74 1a                	je     5a09f <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x57f>
   5a085:	48 89 c1             	mov    %rax,%rcx
   5a088:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5a08f:	c1 e8 04             	shr    $0x4,%eax
   5a092:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a097:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a09e:	3f 
   5a09f:	48 8b 44 24 78       	mov    0x78(%rsp),%rax
   5a0a4:	48 85 c0             	test   %rax,%rax
   5a0a7:	74 1a                	je     5a0c3 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x5a3>
   5a0a9:	48 89 c1             	mov    %rax,%rcx
   5a0ac:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5a0b3:	c1 e8 04             	shr    $0x4,%eax
   5a0b6:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a0bb:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a0c2:	3f 
   5a0c3:	48 8b 84 24 80 00 00 	mov    0x80(%rsp),%rax
   5a0ca:	00 
   5a0cb:	48 85 c0             	test   %rax,%rax
   5a0ce:	74 1a                	je     5a0ea <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x5ca>
   5a0d0:	48 89 c1             	mov    %rax,%rcx
   5a0d3:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5a0da:	c1 e8 04             	shr    $0x4,%eax
   5a0dd:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a0e2:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a0e9:	3f 
   5a0ea:	48 8b 84 24 88 00 00 	mov    0x88(%rsp),%rax
   5a0f1:	00 
   5a0f2:	48 85 c0             	test   %rax,%rax
   5a0f5:	74 1a                	je     5a111 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x5f1>
   5a0f7:	48 89 c1             	mov    %rax,%rcx
   5a0fa:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5a101:	c1 e8 04             	shr    $0x4,%eax
   5a104:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a109:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a110:	3f 
   5a111:	48 8b 84 24 90 00 00 	mov    0x90(%rsp),%rax
   5a118:	00 
   5a119:	48 85 c0             	test   %rax,%rax
   5a11c:	74 1a                	je     5a138 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x618>
   5a11e:	48 89 c1             	mov    %rax,%rcx
   5a121:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5a128:	c1 e8 04             	shr    $0x4,%eax
   5a12b:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a130:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a137:	3f 
   5a138:	48 8b 84 24 98 00 00 	mov    0x98(%rsp),%rax
   5a13f:	00 
   5a140:	48 85 c0             	test   %rax,%rax
   5a143:	74 1a                	je     5a15f <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x63f>
   5a145:	48 89 c1             	mov    %rax,%rcx
   5a148:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5a14f:	c1 e8 04             	shr    $0x4,%eax
   5a152:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5a157:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5a15e:	3f 
   5a15f:	48 89 54 24 08       	mov    %rdx,0x8(%rsp)
   5a164:	48 8d 44 24 08       	lea    0x8(%rsp),%rax
   5a169:	48 81 c4 a8 00 00 00 	add    $0xa8,%rsp
   5a170:	5b                   	pop    %rbx
   5a171:	41 5c                	pop    %r12
   5a173:	41 5d                	pop    %r13
   5a175:	41 5e                	pop    %r14
   5a177:	41 5f                	pop    %r15
   5a179:	5d                   	pop    %rbp
   5a17a:	c3                   	ret
   5a17b:	48 8d 3d 0d 7f fb ff 	lea    -0x480f3(%rip),%rdi        # 1208f <panic_unwind::imp::CANARY+0x194>
   5a182:	48 8d 15 ff a8 06 00 	lea    0x6a8ff(%rip),%rdx        # c4a88 <anon.07bf7b68542f29918c2fe01e2698508b.1766.llvm.13623703653819427234+0x680>
   5a189:	be 21 00 00 00       	mov    $0x21,%esi
   5a18e:	ff 15 74 33 07 00    	call   *0x73374(%rip)        # cd508 <_DYNAMIC+0x3e8>
   5a194:	cc                   	int3
   5a195:	cc                   	int3
   5a196:	cc                   	int3
   5a197:	cc                   	int3
   5a198:	cc                   	int3
   5a199:	cc                   	int3
   5a19a:	cc                   	int3
   5a19b:	cc                   	int3
   5a19c:	cc                   	int3
   5a19d:	cc                   	int3
   5a19e:	cc                   	int3
   5a19f:	cc                   	int3


000000000005e520 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix>:
   5e520:	55                   	push   %rbp
   5e521:	41 57                	push   %r15
   5e523:	41 56                	push   %r14
   5e525:	41 55                	push   %r13
   5e527:	41 54                	push   %r12
   5e529:	53                   	push   %rbx
   5e52a:	48 81 ec a8 00 00 00 	sub    $0xa8,%rsp
   5e531:	4c 8d 7c 24 20       	lea    0x20(%rsp),%r15
   5e536:	4c 89 ff             	mov    %r15,%rdi
   5e539:	ff 15 a9 f2 06 00    	call   *0x6f2a9(%rip)        # cd7e8 <_DYNAMIC+0x6c8>
   5e53f:	4c 8b 64 24 20       	mov    0x20(%rsp),%r12
   5e544:	4d 85 e4             	test   %r12,%r12
   5e547:	0f 84 d3 03 00 00    	je     5e920 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x400>
   5e54d:	0f 57 c0             	xorps  %xmm0,%xmm0
   5e550:	0f 29 84 24 90 00 00 	movaps %xmm0,0x90(%rsp)
   5e557:	00 
   5e558:	0f 29 84 24 80 00 00 	movaps %xmm0,0x80(%rsp)
   5e55f:	00 
   5e560:	0f 29 44 24 70       	movaps %xmm0,0x70(%rsp)
   5e565:	0f 29 44 24 60       	movaps %xmm0,0x60(%rsp)
   5e56a:	0f 29 44 24 50       	movaps %xmm0,0x50(%rsp)
   5e56f:	0f 29 44 24 40       	movaps %xmm0,0x40(%rsp)
   5e574:	0f 29 44 24 30       	movaps %xmm0,0x30(%rsp)
   5e579:	0f 29 44 24 20       	movaps %xmm0,0x20(%rsp)
   5e57e:	31 db                	xor    %ebx,%ebx
   5e580:	4c 8b 35 01 f3 06 00 	mov    0x6f301(%rip),%r14        # cd888 <_DYNAMIC+0x768>
   5e587:	66 0f 1f 84 00 00 00 	nopw   0x0(%rax,%rax,1)
   5e58e:	00 00 
   5e590:	41 0f b6 84 24 c0 04 	movzbl 0x4c0(%r12),%eax
   5e597:	00 00 
   5e599:	48 85 c0             	test   %rax,%rax
   5e59c:	74 47                	je     5e5e5 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0xc5>
   5e59e:	48 ff c8             	dec    %rax
   5e5a1:	41 88 84 24 c0 04 00 	mov    %al,0x4c0(%r12)
   5e5a8:	00 
   5e5a9:	83 e0 0f             	and    $0xf,%eax
   5e5ac:	49 8b 84 c4 c8 04 00 	mov    0x4c8(%r12,%rax,8),%rax
   5e5b3:	00 
   5e5b4:	48 89 c1             	mov    %rax,%rcx
   5e5b7:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5e5be:	89 c2                	mov    %eax,%edx
   5e5c0:	c1 ea 04             	shr    $0x4,%edx
   5e5c3:	81 e2 ff ff 03 00    	and    $0x3ffff,%edx
   5e5c9:	80 a4 11 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rdx,1)
   5e5d0:	3f 
   5e5d1:	48 89 44 1c 20       	mov    %rax,0x20(%rsp,%rbx,1)
   5e5d6:	48 83 c3 08          	add    $0x8,%rbx
   5e5da:	48 81 fb 80 00 00 00 	cmp    $0x80,%rbx
   5e5e1:	75 ad                	jne    5e590 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x70>
   5e5e3:	eb 1c                	jmp    5e601 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0xe1>
   5e5e5:	49 8d 7c 24 10       	lea    0x10(%r12),%rdi
   5e5ea:	31 f6                	xor    %esi,%esi
   5e5ec:	41 ff d6             	call   *%r14
   5e5ef:	48 89 44 1c 20       	mov    %rax,0x20(%rsp,%rbx,1)
   5e5f4:	48 83 c3 08          	add    $0x8,%rbx
   5e5f8:	48 81 fb 80 00 00 00 	cmp    $0x80,%rbx
   5e5ff:	75 8f                	jne    5e590 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x70>
   5e601:	4c 89 7c 24 18       	mov    %r15,0x18(%rsp)
   5e606:	48 8d 44 24 18       	lea    0x18(%rsp),%rax
   5e60b:	4d 8d 7c 24 40       	lea    0x40(%r12),%r15
   5e610:	31 ed                	xor    %ebp,%ebp
   5e612:	4c 8b 1d e7 f1 06 00 	mov    0x6f1e7(%rip),%r11        # cd800 <_DYNAMIC+0x6e0>
   5e619:	4c 8b 2d 00 f2 06 00 	mov    0x6f200(%rip),%r13        # cd820 <_DYNAMIC+0x700>
   5e620:	eb 24                	jmp    5e646 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x126>
   5e622:	66 66 66 66 66 2e 0f 	data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   5e629:	1f 84 00 00 00 00 00 
   5e630:	f0 49 ff 45 00       	lock incq 0x0(%r13)
   5e635:	48 83 c5 08          	add    $0x8,%rbp
   5e639:	48 81 fd 80 00 00 00 	cmp    $0x80,%rbp
   5e640:	0f 84 c8 02 00 00    	je     5e90e <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x3ee>
   5e646:	48 8b 7c 2c 20       	mov    0x20(%rsp,%rbp,1),%rdi
   5e64b:	48 85 ff             	test   %rdi,%rdi
   5e64e:	74 e5                	je     5e635 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x115>
   5e650:	48 89 f8             	mov    %rdi,%rax
   5e653:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   5e659:	49 39 84 24 40 01 00 	cmp    %rax,0x140(%r12)
   5e660:	00 
   5e661:	75 1d                	jne    5e680 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x160>
   5e663:	f0 49 ff 03          	lock incq (%r11)
   5e667:	4d 8b 84 24 48 01 00 	mov    0x148(%r12),%r8
   5e66e:	00 
   5e66f:	41 0f b6 48 1c       	movzbl 0x1c(%r8),%ecx
   5e674:	83 f9 02             	cmp    $0x2,%ecx
   5e677:	74 55                	je     5e6ce <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x1ae>
   5e679:	e9 4b 01 00 00       	jmp    5e7c9 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x2a9>
   5e67e:	66 90                	xchg   %ax,%ax
   5e680:	48 89 c2             	mov    %rax,%rdx
   5e683:	48 c1 ea 16          	shr    $0x16,%rdx
   5e687:	89 d1                	mov    %edx,%ecx
   5e689:	83 e1 0f             	and    $0xf,%ecx
   5e68c:	c1 e1 04             	shl    $0x4,%ecx
   5e68f:	49 39 04 0f          	cmp    %rax,(%r15,%rcx,1)
   5e693:	0f 85 82 00 00 00    	jne    5e71b <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x1fb>
   5e699:	4d 8b 44 0f 08       	mov    0x8(%r15,%rcx,1),%r8
   5e69e:	f0 49 ff 03          	lock incq (%r11)
   5e6a2:	49 39 c0             	cmp    %rax,%r8
   5e6a5:	75 19                	jne    5e6c0 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x1a0>
   5e6a7:	49 89 84 24 40 01 00 	mov    %rax,0x140(%r12)
   5e6ae:	00 
   5e6af:	4d 89 84 24 48 01 00 	mov    %r8,0x148(%r12)
   5e6b6:	00 
   5e6b7:	66 0f 1f 84 00 00 00 	nopw   0x0(%rax,%rax,1)
   5e6be:	00 00 
   5e6c0:	41 0f b6 48 1c       	movzbl 0x1c(%r8),%ecx
   5e6c5:	83 f9 02             	cmp    $0x2,%ecx
   5e6c8:	0f 85 fb 00 00 00    	jne    5e7c9 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x2a9>
   5e6ce:	48 89 fe             	mov    %rdi,%rsi
   5e6d1:	4c 29 c6             	sub    %r8,%rsi
   5e6d4:	49 3b 70 38          	cmp    0x38(%r8),%rsi
   5e6d8:	0f 83 80 01 00 00    	jae    5e85e <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x33e>
   5e6de:	49 8b 40 30          	mov    0x30(%r8),%rax
   5e6e2:	49 8b 48 08          	mov    0x8(%r8),%rcx
   5e6e6:	4c 01 c0             	add    %r8,%rax
   5e6e9:	0f 82 41 ff ff ff    	jb     5e630 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5e6ef:	48 39 f8             	cmp    %rdi,%rax
   5e6f2:	0f 85 38 ff ff ff    	jne    5e630 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5e6f8:	4c 01 c6             	add    %r8,%rsi
   5e6fb:	ba 08 00 00 00       	mov    $0x8,%edx
   5e700:	b9 10 00 00 00       	mov    $0x10,%ecx
   5e705:	49 8d 7c 24 10       	lea    0x10(%r12),%rdi
   5e70a:	4c 89 db             	mov    %r11,%rbx
   5e70d:	ff 15 05 f1 06 00    	call   *0x6f105(%rip)        # cd818 <_DYNAMIC+0x6f8>
   5e713:	49 89 db             	mov    %rbx,%r11
   5e716:	e9 1a ff ff ff       	jmp    5e635 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x115>
   5e71b:	4c 01 f9             	add    %r15,%rcx
   5e71e:	48 8b 35 eb f0 06 00 	mov    0x6f0eb(%rip),%rsi        # cd810 <_DYNAMIC+0x6f0>
   5e725:	f0 48 ff 06          	lock incq (%rsi)
   5e729:	48 89 c6             	mov    %rax,%rsi
   5e72c:	48 c1 ee 23          	shr    $0x23,%rsi
   5e730:	49 89 c0             	mov    %rax,%r8
   5e733:	49 c1 e8 30          	shr    $0x30,%r8
   5e737:	44 31 c6             	xor    %r8d,%esi
   5e73a:	31 f2                	xor    %esi,%edx
   5e73c:	81 e2 ff 1f 00 00    	and    $0x1fff,%edx
   5e742:	49 8b b4 24 50 01 00 	mov    0x150(%r12),%rsi
   5e749:	00 
   5e74a:	49 89 d1             	mov    %rdx,%r9
   5e74d:	0f 1f 00             	nopl   (%rax)
   5e750:	4e 8b 04 ce          	mov    (%rsi,%r9,8),%r8
   5e754:	4d 85 c0             	test   %r8,%r8
   5e757:	0f 84 01 01 00 00    	je     5e85e <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x33e>
   5e75d:	4d 89 c2             	mov    %r8,%r10
   5e760:	49 81 e2 00 00 c0 ff 	and    $0xffffffffffc00000,%r10
   5e767:	49 39 c2             	cmp    %rax,%r10
   5e76a:	74 14                	je     5e780 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x260>
   5e76c:	41 ff c1             	inc    %r9d
   5e76f:	41 81 e1 ff 1f 00 00 	and    $0x1fff,%r9d
   5e776:	49 39 d1             	cmp    %rdx,%r9
   5e779:	75 d5                	jne    5e750 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x230>
   5e77b:	e9 de 00 00 00       	jmp    5e85e <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x33e>
   5e780:	4c 89 c2             	mov    %r8,%rdx
   5e783:	48 81 e2 ff ff 3f 00 	and    $0x3fffff,%rdx
   5e78a:	74 28                	je     5e7b4 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x294>
   5e78c:	48 8d 72 ff          	lea    -0x1(%rdx),%rsi
   5e790:	45 8b 84 24 68 01 00 	mov    0x168(%r12),%r8d
   5e797:	00 
   5e798:	4c 39 c6             	cmp    %r8,%rsi
   5e79b:	0f 83 bd 00 00 00    	jae    5e85e <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x33e>
   5e7a1:	49 8b 74 24 30       	mov    0x30(%r12),%rsi
   5e7a6:	4c 8b 44 d6 f8       	mov    -0x8(%rsi,%rdx,8),%r8
   5e7ab:	4d 85 c0             	test   %r8,%r8
   5e7ae:	0f 84 aa 00 00 00    	je     5e85e <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x33e>
   5e7b4:	48 89 01             	mov    %rax,(%rcx)
   5e7b7:	4c 89 41 08          	mov    %r8,0x8(%rcx)
   5e7bb:	41 0f b6 48 1c       	movzbl 0x1c(%r8),%ecx
   5e7c0:	83 f9 02             	cmp    $0x2,%ecx
   5e7c3:	0f 84 05 ff ff ff    	je     5e6ce <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x1ae>
   5e7c9:	41 0f b6 84 24 c0 04 	movzbl 0x4c0(%r12),%eax
   5e7d0:	00 00 
   5e7d2:	85 c9                	test   %ecx,%ecx
   5e7d4:	74 20                	je     5e7f6 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x2d6>
   5e7d6:	83 f9 01             	cmp    $0x1,%ecx
   5e7d9:	0f 85 51 fe ff ff    	jne    5e630 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5e7df:	b9 00 20 04 00       	mov    $0x42000,%ecx
   5e7e4:	89 fb                	mov    %edi,%ebx
   5e7e6:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   5e7ec:	39 cb                	cmp    %ecx,%ebx
   5e7ee:	0f 82 3c fe ff ff    	jb     5e630 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5e7f4:	eb 15                	jmp    5e80b <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x2eb>
   5e7f6:	b9 00 f0 05 00       	mov    $0x5f000,%ecx
   5e7fb:	89 fb                	mov    %edi,%ebx
   5e7fd:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   5e803:	39 cb                	cmp    %ecx,%ebx
   5e805:	0f 82 25 fe ff ff    	jb     5e630 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5e80b:	40 f6 c7 0f          	test   $0xf,%dil
   5e80f:	0f 85 20 fe ff ff    	jne    5e635 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x115>
   5e815:	41 89 de             	mov    %ebx,%r14d
   5e818:	41 c1 ee 04          	shr    $0x4,%r14d
   5e81c:	43 0f b6 8c 30 88 15 	movzbl 0x1588(%r8,%r14,1),%ecx
   5e823:	00 00 
   5e825:	85 c9                	test   %ecx,%ecx
   5e827:	74 50                	je     5e879 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x359>
   5e829:	83 f9 01             	cmp    $0x1,%ecx
   5e82c:	75 59                	jne    5e887 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x367>
   5e82e:	49 3b 18             	cmp    (%r8),%rbx
   5e831:	0f 83 f9 fd ff ff    	jae    5e630 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5e837:	4c 01 c3             	add    %r8,%rbx
   5e83a:	3c 0f                	cmp    $0xf,%al
   5e83c:	77 57                	ja     5e895 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x375>
   5e83e:	43 c6 84 30 88 15 00 	movb   $0x41,0x1588(%r8,%r14,1)
   5e845:	00 41 
   5e847:	49 89 9c c4 c8 04 00 	mov    %rbx,0x4c8(%r12,%rax,8)
   5e84e:	00 
   5e84f:	ff c0                	inc    %eax
   5e851:	41 88 84 24 c0 04 00 	mov    %al,0x4c0(%r12)
   5e858:	00 
   5e859:	e9 d7 fd ff ff       	jmp    5e635 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x115>
   5e85e:	be 08 00 00 00       	mov    $0x8,%esi
   5e863:	ba 10 00 00 00       	mov    $0x10,%edx
   5e868:	4c 89 db             	mov    %r11,%rbx
   5e86b:	ff 15 bf ef 06 00    	call   *0x6efbf(%rip)        # cd830 <_DYNAMIC+0x710>
   5e871:	49 89 db             	mov    %rbx,%r11
   5e874:	e9 bc fd ff ff       	jmp    5e635 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x115>
   5e879:	49 3b 18             	cmp    (%r8),%rbx
   5e87c:	0f 83 ae fd ff ff    	jae    5e630 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5e882:	e9 ae fd ff ff       	jmp    5e635 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x115>
   5e887:	80 f9 40             	cmp    $0x40,%cl
   5e88a:	0f 83 a0 fd ff ff    	jae    5e630 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5e890:	e9 a0 fd ff ff       	jmp    5e635 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x115>
   5e895:	b9 08 00 00 00       	mov    $0x8,%ecx
   5e89a:	49 8d 7c 24 10       	lea    0x10(%r12),%rdi
   5e89f:	31 f6                	xor    %esi,%esi
   5e8a1:	49 8d 94 24 c8 04 00 	lea    0x4c8(%r12),%rdx
   5e8a8:	00 
   5e8a9:	48 89 54 24 08       	mov    %rdx,0x8(%rsp)
   5e8ae:	4c 89 44 24 10       	mov    %r8,0x10(%rsp)
   5e8b3:	e8 e8 c5 fe ff       	call   4aea0 <<sefer_alloc::alloc_core::alloc_core::alloc_core_impl::AllocCore>::flush_class_internal>
   5e8b8:	4c 8b 1d 41 ef 06 00 	mov    0x6ef41(%rip),%r11        # cd800 <_DYNAMIC+0x6e0>
   5e8bf:	49 8d 84 24 08 05 00 	lea    0x508(%r12),%rax
   5e8c6:	00 
   5e8c7:	0f 10 00             	movups (%rax),%xmm0
   5e8ca:	0f 10 48 10          	movups 0x10(%rax),%xmm1
   5e8ce:	0f 10 50 20          	movups 0x20(%rax),%xmm2
   5e8d2:	0f 10 58 30          	movups 0x30(%rax),%xmm3
   5e8d6:	48 8b 44 24 08       	mov    0x8(%rsp),%rax
   5e8db:	0f 11 58 30          	movups %xmm3,0x30(%rax)
   5e8df:	0f 11 50 20          	movups %xmm2,0x20(%rax)
   5e8e3:	0f 11 48 10          	movups %xmm1,0x10(%rax)
   5e8e7:	0f 11 00             	movups %xmm0,(%rax)
   5e8ea:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   5e8ef:	42 c6 84 30 88 15 00 	movb   $0x41,0x1588(%rax,%r14,1)
   5e8f6:	00 41 
   5e8f8:	49 89 9c 24 08 05 00 	mov    %rbx,0x508(%r12)
   5e8ff:	00 
   5e900:	41 c6 84 24 c0 04 00 	movb   $0x9,0x4c0(%r12)
   5e907:	00 09 
   5e909:	e9 27 fd ff ff       	jmp    5e635 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x115>
   5e90e:	48 81 c4 a8 00 00 00 	add    $0xa8,%rsp
   5e915:	5b                   	pop    %rbx
   5e916:	41 5c                	pop    %r12
   5e918:	41 5d                	pop    %r13
   5e91a:	41 5e                	pop    %r14
   5e91c:	41 5f                	pop    %r15
   5e91e:	5d                   	pop    %rbp
   5e91f:	c3                   	ret
   5e920:	48 8d 3d 68 37 fb ff 	lea    -0x4c898(%rip),%rdi        # 1208f <panic_unwind::imp::CANARY+0x194>
   5e927:	48 8d 15 5a 61 06 00 	lea    0x6615a(%rip),%rdx        # c4a88 <anon.07bf7b68542f29918c2fe01e2698508b.1766.llvm.13623703653819427234+0x680>
   5e92e:	be 21 00 00 00       	mov    $0x21,%esi
   5e933:	ff 15 cf eb 06 00    	call   *0x6ebcf(%rip)        # cd508 <_DYNAMIC+0x3e8>
   5e939:	cc                   	int3
   5e93a:	cc                   	int3
   5e93b:	cc                   	int3
   5e93c:	cc                   	int3
   5e93d:	cc                   	int3
   5e93e:	cc                   	int3
   5e93f:	cc                   	int3

