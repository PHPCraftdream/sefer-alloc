000000000004c4a0 <perf_gate_iai::alloc_clear_magazine_only_16b::wrapper>:
   4c4a0:	50                   	push   %rax
   4c4a1:	e8 2a c7 00 00       	call   58bd0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b>
   4c4a6:	58                   	pop    %rax
   4c4a7:	c3                   	ret
   4c4a8:	cc                   	int3
   4c4a9:	cc                   	int3
   4c4aa:	cc                   	int3
   4c4ab:	cc                   	int3
   4c4ac:	cc                   	int3
   4c4ad:	cc                   	int3
   4c4ae:	cc                   	int3
   4c4af:	cc                   	int3


000000000004ec80 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::wrapper>:
   4ec80:	50                   	push   %rax
   4ec81:	e8 9a e3 00 00       	call   5d020 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix>
   4ec86:	58                   	pop    %rax
   4ec87:	c3                   	ret
   4ec88:	cc                   	int3
   4ec89:	cc                   	int3
   4ec8a:	cc                   	int3
   4ec8b:	cc                   	int3
   4ec8c:	cc                   	int3
   4ec8d:	cc                   	int3
   4ec8e:	cc                   	int3
   4ec8f:	cc                   	int3


0000000000058bd0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b>:
   58bd0:	55                   	push   %rbp
   58bd1:	41 57                	push   %r15
   58bd3:	41 56                	push   %r14
   58bd5:	41 55                	push   %r13
   58bd7:	41 54                	push   %r12
   58bd9:	53                   	push   %rbx
   58bda:	48 81 ec 98 00 00 00 	sub    $0x98,%rsp
   58be1:	4c 8d 64 24 10       	lea    0x10(%rsp),%r12
   58be6:	4c 89 e7             	mov    %r12,%rdi
   58be9:	ff 15 39 35 07 00    	call   *0x73539(%rip)        # cc128 <_DYNAMIC+0x6c8>
   58bef:	4c 8b 7c 24 10       	mov    0x10(%rsp),%r15
   58bf4:	4d 85 ff             	test   %r15,%r15
   58bf7:	0f 84 b0 05 00 00    	je     591ad <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x5dd>
   58bfd:	0f 57 c0             	xorps  %xmm0,%xmm0
   58c00:	0f 29 84 24 80 00 00 	movaps %xmm0,0x80(%rsp)
   58c07:	00 
   58c08:	0f 29 44 24 70       	movaps %xmm0,0x70(%rsp)
   58c0d:	0f 29 44 24 60       	movaps %xmm0,0x60(%rsp)
   58c12:	0f 29 44 24 50       	movaps %xmm0,0x50(%rsp)
   58c17:	0f 29 44 24 40       	movaps %xmm0,0x40(%rsp)
   58c1c:	0f 29 44 24 30       	movaps %xmm0,0x30(%rsp)
   58c21:	0f 29 44 24 20       	movaps %xmm0,0x20(%rsp)
   58c26:	0f 29 44 24 10       	movaps %xmm0,0x10(%rsp)
   58c2b:	31 db                	xor    %ebx,%ebx
   58c2d:	4c 8b 35 94 35 07 00 	mov    0x73594(%rip),%r14        # cc1c8 <_DYNAMIC+0x768>
   58c34:	66 66 66 2e 0f 1f 84 	data16 data16 cs nopw 0x0(%rax,%rax,1)
   58c3b:	00 00 00 00 00 
   58c40:	41 0f b6 87 c0 04 00 	movzbl 0x4c0(%r15),%eax
   58c47:	00 
   58c48:	48 85 c0             	test   %rax,%rax
   58c4b:	74 46                	je     58c93 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0xc3>
   58c4d:	48 ff c8             	dec    %rax
   58c50:	41 88 87 c0 04 00 00 	mov    %al,0x4c0(%r15)
   58c57:	83 e0 0f             	and    $0xf,%eax
   58c5a:	49 8b 84 c7 c8 04 00 	mov    0x4c8(%r15,%rax,8),%rax
   58c61:	00 
   58c62:	48 89 c1             	mov    %rax,%rcx
   58c65:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   58c6c:	89 c2                	mov    %eax,%edx
   58c6e:	c1 ea 04             	shr    $0x4,%edx
   58c71:	81 e2 ff ff 03 00    	and    $0x3ffff,%edx
   58c77:	80 a4 11 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rdx,1)
   58c7e:	3f 
   58c7f:	48 89 44 1c 10       	mov    %rax,0x10(%rsp,%rbx,1)
   58c84:	48 83 c3 08          	add    $0x8,%rbx
   58c88:	48 81 fb 80 00 00 00 	cmp    $0x80,%rbx
   58c8f:	75 af                	jne    58c40 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x70>
   58c91:	eb 1b                	jmp    58cae <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0xde>
   58c93:	49 8d 7f 10          	lea    0x10(%r15),%rdi
   58c97:	31 f6                	xor    %esi,%esi
   58c99:	41 ff d6             	call   *%r14
   58c9c:	48 89 44 1c 10       	mov    %rax,0x10(%rsp,%rbx,1)
   58ca1:	48 83 c3 08          	add    $0x8,%rbx
   58ca5:	48 81 fb 80 00 00 00 	cmp    $0x80,%rbx
   58cac:	75 92                	jne    58c40 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x70>
   58cae:	4c 89 64 24 08       	mov    %r12,0x8(%rsp)
   58cb3:	48 8d 44 24 08       	lea    0x8(%rsp),%rax
   58cb8:	4d 8d 67 40          	lea    0x40(%r15),%r12
   58cbc:	45 31 ed             	xor    %r13d,%r13d
   58cbf:	48 8b 2d 7a 34 07 00 	mov    0x7347a(%rip),%rbp        # cc140 <_DYNAMIC+0x6e0>
   58cc6:	4c 8b 35 9b 34 07 00 	mov    0x7349b(%rip),%r14        # cc168 <_DYNAMIC+0x708>
   58ccd:	48 8b 1d 7c 34 07 00 	mov    0x7347c(%rip),%rbx        # cc150 <_DYNAMIC+0x6f0>
   58cd4:	eb 1f                	jmp    58cf5 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x125>
   58cd6:	66 2e 0f 1f 84 00 00 	cs nopw 0x0(%rax,%rax,1)
   58cdd:	00 00 00 
   58ce0:	f0 49 ff 06          	lock incq (%r14)
   58ce4:	49 83 c5 08          	add    $0x8,%r13
   58ce8:	49 81 fd 80 00 00 00 	cmp    $0x80,%r13
   58cef:	0f 84 51 02 00 00    	je     58f46 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x376>
   58cf5:	4a 8b 7c 2c 10       	mov    0x10(%rsp,%r13,1),%rdi
   58cfa:	48 85 ff             	test   %rdi,%rdi
   58cfd:	74 e5                	je     58ce4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x114>
   58cff:	48 89 f8             	mov    %rdi,%rax
   58d02:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   58d08:	49 39 87 40 01 00 00 	cmp    %rax,0x140(%r15)
   58d0f:	75 1f                	jne    58d30 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x160>
   58d11:	f0 48 ff 45 00       	lock incq 0x0(%rbp)
   58d16:	4d 8b 87 48 01 00 00 	mov    0x148(%r15),%r8
   58d1d:	41 0f b6 40 1c       	movzbl 0x1c(%r8),%eax
   58d22:	83 f8 02             	cmp    $0x2,%eax
   58d25:	74 57                	je     58d7e <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x1ae>
   58d27:	e9 3b 01 00 00       	jmp    58e67 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x297>
   58d2c:	0f 1f 40 00          	nopl   0x0(%rax)
   58d30:	48 89 c2             	mov    %rax,%rdx
   58d33:	48 c1 ea 16          	shr    $0x16,%rdx
   58d37:	89 d1                	mov    %edx,%ecx
   58d39:	83 e1 0f             	and    $0xf,%ecx
   58d3c:	c1 e1 04             	shl    $0x4,%ecx
   58d3f:	49 39 04 0c          	cmp    %rax,(%r12,%rcx,1)
   58d43:	75 7f                	jne    58dc4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x1f4>
   58d45:	4d 8b 44 0c 08       	mov    0x8(%r12,%rcx,1),%r8
   58d4a:	f0 48 ff 45 00       	lock incq 0x0(%rbp)
   58d4f:	49 39 c0             	cmp    %rax,%r8
   58d52:	75 1c                	jne    58d70 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x1a0>
   58d54:	49 89 87 40 01 00 00 	mov    %rax,0x140(%r15)
   58d5b:	4d 89 87 48 01 00 00 	mov    %r8,0x148(%r15)
   58d62:	66 66 66 66 66 2e 0f 	data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   58d69:	1f 84 00 00 00 00 00 
   58d70:	41 0f b6 40 1c       	movzbl 0x1c(%r8),%eax
   58d75:	83 f8 02             	cmp    $0x2,%eax
   58d78:	0f 85 e9 00 00 00    	jne    58e67 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x297>
   58d7e:	48 89 fe             	mov    %rdi,%rsi
   58d81:	4c 29 c6             	sub    %r8,%rsi
   58d84:	49 3b 70 38          	cmp    0x38(%r8),%rsi
   58d88:	0f 83 6c 01 00 00    	jae    58efa <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x32a>
   58d8e:	49 8b 40 30          	mov    0x30(%r8),%rax
   58d92:	49 8b 48 08          	mov    0x8(%r8),%rcx
   58d96:	4c 01 c0             	add    %r8,%rax
   58d99:	0f 82 41 ff ff ff    	jb     58ce0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   58d9f:	48 39 f8             	cmp    %rdi,%rax
   58da2:	0f 85 38 ff ff ff    	jne    58ce0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   58da8:	4c 01 c6             	add    %r8,%rsi
   58dab:	ba 08 00 00 00       	mov    $0x8,%edx
   58db0:	b9 10 00 00 00       	mov    $0x10,%ecx
   58db5:	49 8d 7f 10          	lea    0x10(%r15),%rdi
   58db9:	ff 15 a1 33 07 00    	call   *0x733a1(%rip)        # cc160 <_DYNAMIC+0x700>
   58dbf:	e9 20 ff ff ff       	jmp    58ce4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x114>
   58dc4:	4c 01 e1             	add    %r12,%rcx
   58dc7:	f0 48 ff 03          	lock incq (%rbx)
   58dcb:	48 89 c6             	mov    %rax,%rsi
   58dce:	48 c1 ee 23          	shr    $0x23,%rsi
   58dd2:	49 89 c0             	mov    %rax,%r8
   58dd5:	49 c1 e8 30          	shr    $0x30,%r8
   58dd9:	44 31 c6             	xor    %r8d,%esi
   58ddc:	31 f2                	xor    %esi,%edx
   58dde:	81 e2 ff 1f 00 00    	and    $0x1fff,%edx
   58de4:	49 8b b7 50 01 00 00 	mov    0x150(%r15),%rsi
   58deb:	49 89 d1             	mov    %rdx,%r9
   58dee:	66 90                	xchg   %ax,%ax
   58df0:	4e 8b 04 ce          	mov    (%rsi,%r9,8),%r8
   58df4:	4d 85 c0             	test   %r8,%r8
   58df7:	0f 84 fd 00 00 00    	je     58efa <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x32a>
   58dfd:	4d 89 c2             	mov    %r8,%r10
   58e00:	49 81 e2 00 00 c0 ff 	and    $0xffffffffffc00000,%r10
   58e07:	49 39 c2             	cmp    %rax,%r10
   58e0a:	74 14                	je     58e20 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x250>
   58e0c:	41 ff c1             	inc    %r9d
   58e0f:	41 81 e1 ff 1f 00 00 	and    $0x1fff,%r9d
   58e16:	49 39 d1             	cmp    %rdx,%r9
   58e19:	75 d5                	jne    58df0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x220>
   58e1b:	e9 da 00 00 00       	jmp    58efa <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x32a>
   58e20:	4c 89 c2             	mov    %r8,%rdx
   58e23:	48 81 e2 ff ff 3f 00 	and    $0x3fffff,%rdx
   58e2a:	74 26                	je     58e52 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x282>
   58e2c:	48 8d 72 ff          	lea    -0x1(%rdx),%rsi
   58e30:	45 8b 87 68 01 00 00 	mov    0x168(%r15),%r8d
   58e37:	4c 39 c6             	cmp    %r8,%rsi
   58e3a:	0f 83 ba 00 00 00    	jae    58efa <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x32a>
   58e40:	49 8b 77 30          	mov    0x30(%r15),%rsi
   58e44:	4c 8b 44 d6 f8       	mov    -0x8(%rsi,%rdx,8),%r8
   58e49:	4d 85 c0             	test   %r8,%r8
   58e4c:	0f 84 a8 00 00 00    	je     58efa <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x32a>
   58e52:	48 89 01             	mov    %rax,(%rcx)
   58e55:	4c 89 41 08          	mov    %r8,0x8(%rcx)
   58e59:	41 0f b6 40 1c       	movzbl 0x1c(%r8),%eax
   58e5e:	83 f8 02             	cmp    $0x2,%eax
   58e61:	0f 84 17 ff ff ff    	je     58d7e <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x1ae>
   58e67:	41 0f b6 8f c0 04 00 	movzbl 0x4c0(%r15),%ecx
   58e6e:	00 
   58e6f:	85 c0                	test   %eax,%eax
   58e71:	74 20                	je     58e93 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x2c3>
   58e73:	83 f8 01             	cmp    $0x1,%eax
   58e76:	0f 85 64 fe ff ff    	jne    58ce0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   58e7c:	b8 00 20 04 00       	mov    $0x42000,%eax
   58e81:	89 fe                	mov    %edi,%esi
   58e83:	81 e6 ff ff 3f 00    	and    $0x3fffff,%esi
   58e89:	39 c6                	cmp    %eax,%esi
   58e8b:	0f 82 4f fe ff ff    	jb     58ce0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   58e91:	eb 15                	jmp    58ea8 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x2d8>
   58e93:	b8 00 f0 05 00       	mov    $0x5f000,%eax
   58e98:	89 fe                	mov    %edi,%esi
   58e9a:	81 e6 ff ff 3f 00    	and    $0x3fffff,%esi
   58ea0:	39 c6                	cmp    %eax,%esi
   58ea2:	0f 82 38 fe ff ff    	jb     58ce0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   58ea8:	40 f6 c7 0f          	test   $0xf,%dil
   58eac:	0f 85 32 fe ff ff    	jne    58ce4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x114>
   58eb2:	89 f0                	mov    %esi,%eax
   58eb4:	c1 e8 04             	shr    $0x4,%eax
   58eb7:	41 0f b6 94 00 88 15 	movzbl 0x1588(%r8,%rax,1),%edx
   58ebe:	00 00 
   58ec0:	85 d2                	test   %edx,%edx
   58ec2:	74 4b                	je     58f0f <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x33f>
   58ec4:	83 fa 01             	cmp    $0x1,%edx
   58ec7:	75 54                	jne    58f1d <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x34d>
   58ec9:	49 3b 30             	cmp    (%r8),%rsi
   58ecc:	0f 83 0e fe ff ff    	jae    58ce0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   58ed2:	4c 01 c6             	add    %r8,%rsi
   58ed5:	80 f9 10             	cmp    $0x10,%cl
   58ed8:	73 51                	jae    58f2b <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x35b>
   58eda:	41 c6 84 00 88 15 00 	movb   $0x41,0x1588(%r8,%rax,1)
   58ee1:	00 41 
   58ee3:	49 89 b4 cf c8 04 00 	mov    %rsi,0x4c8(%r15,%rcx,8)
   58eea:	00 
   58eeb:	8d 41 01             	lea    0x1(%rcx),%eax
   58eee:	41 88 87 c0 04 00 00 	mov    %al,0x4c0(%r15)
   58ef5:	e9 ea fd ff ff       	jmp    58ce4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x114>
   58efa:	be 08 00 00 00       	mov    $0x8,%esi
   58eff:	ba 10 00 00 00       	mov    $0x10,%edx
   58f04:	ff 15 6e 32 07 00    	call   *0x7326e(%rip)        # cc178 <_DYNAMIC+0x718>
   58f0a:	e9 d5 fd ff ff       	jmp    58ce4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x114>
   58f0f:	49 3b 30             	cmp    (%r8),%rsi
   58f12:	0f 83 c8 fd ff ff    	jae    58ce0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   58f18:	e9 c7 fd ff ff       	jmp    58ce4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x114>
   58f1d:	80 fa 40             	cmp    $0x40,%dl
   58f20:	0f 83 ba fd ff ff    	jae    58ce0 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x110>
   58f26:	e9 b9 fd ff ff       	jmp    58ce4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x114>
   58f2b:	49 01 c0             	add    %rax,%r8
   58f2e:	49 81 c0 88 15 00 00 	add    $0x1588,%r8
   58f35:	49 8d 7f 10          	lea    0x10(%r15),%rdi
   58f39:	31 d2                	xor    %edx,%edx
   58f3b:	ff 15 3f 32 07 00    	call   *0x7323f(%rip)        # cc180 <_DYNAMIC+0x720>
   58f41:	e9 9e fd ff ff       	jmp    58ce4 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x114>
   58f46:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   58f4b:	48 85 c0             	test   %rax,%rax
   58f4e:	74 1a                	je     58f6a <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x39a>
   58f50:	48 89 c1             	mov    %rax,%rcx
   58f53:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   58f5a:	c1 e8 04             	shr    $0x4,%eax
   58f5d:	25 ff ff 03 00       	and    $0x3ffff,%eax
   58f62:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   58f69:	3f 
   58f6a:	48 8b 44 24 18       	mov    0x18(%rsp),%rax
   58f6f:	48 85 c0             	test   %rax,%rax
   58f72:	48 8d 54 24 10       	lea    0x10(%rsp),%rdx
   58f77:	74 1a                	je     58f93 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x3c3>
   58f79:	48 89 c1             	mov    %rax,%rcx
   58f7c:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   58f83:	c1 e8 04             	shr    $0x4,%eax
   58f86:	25 ff ff 03 00       	and    $0x3ffff,%eax
   58f8b:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   58f92:	3f 
   58f93:	48 8b 44 24 20       	mov    0x20(%rsp),%rax
   58f98:	48 85 c0             	test   %rax,%rax
   58f9b:	74 1a                	je     58fb7 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x3e7>
   58f9d:	48 89 c1             	mov    %rax,%rcx
   58fa0:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   58fa7:	c1 e8 04             	shr    $0x4,%eax
   58faa:	25 ff ff 03 00       	and    $0x3ffff,%eax
   58faf:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   58fb6:	3f 
   58fb7:	48 8b 44 24 28       	mov    0x28(%rsp),%rax
   58fbc:	48 85 c0             	test   %rax,%rax
   58fbf:	74 1a                	je     58fdb <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x40b>
   58fc1:	48 89 c1             	mov    %rax,%rcx
   58fc4:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   58fcb:	c1 e8 04             	shr    $0x4,%eax
   58fce:	25 ff ff 03 00       	and    $0x3ffff,%eax
   58fd3:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   58fda:	3f 
   58fdb:	48 8b 44 24 30       	mov    0x30(%rsp),%rax
   58fe0:	48 85 c0             	test   %rax,%rax
   58fe3:	74 1a                	je     58fff <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x42f>
   58fe5:	48 89 c1             	mov    %rax,%rcx
   58fe8:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   58fef:	c1 e8 04             	shr    $0x4,%eax
   58ff2:	25 ff ff 03 00       	and    $0x3ffff,%eax
   58ff7:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   58ffe:	3f 
   58fff:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   59004:	48 85 c0             	test   %rax,%rax
   59007:	74 1a                	je     59023 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x453>
   59009:	48 89 c1             	mov    %rax,%rcx
   5900c:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59013:	c1 e8 04             	shr    $0x4,%eax
   59016:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5901b:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59022:	3f 
   59023:	48 8b 44 24 40       	mov    0x40(%rsp),%rax
   59028:	48 85 c0             	test   %rax,%rax
   5902b:	74 1a                	je     59047 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x477>
   5902d:	48 89 c1             	mov    %rax,%rcx
   59030:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59037:	c1 e8 04             	shr    $0x4,%eax
   5903a:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5903f:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59046:	3f 
   59047:	48 8b 44 24 48       	mov    0x48(%rsp),%rax
   5904c:	48 85 c0             	test   %rax,%rax
   5904f:	74 1a                	je     5906b <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x49b>
   59051:	48 89 c1             	mov    %rax,%rcx
   59054:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5905b:	c1 e8 04             	shr    $0x4,%eax
   5905e:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59063:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5906a:	3f 
   5906b:	48 8b 44 24 50       	mov    0x50(%rsp),%rax
   59070:	48 85 c0             	test   %rax,%rax
   59073:	74 1a                	je     5908f <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x4bf>
   59075:	48 89 c1             	mov    %rax,%rcx
   59078:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5907f:	c1 e8 04             	shr    $0x4,%eax
   59082:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59087:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5908e:	3f 
   5908f:	48 8b 44 24 58       	mov    0x58(%rsp),%rax
   59094:	48 85 c0             	test   %rax,%rax
   59097:	74 1a                	je     590b3 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x4e3>
   59099:	48 89 c1             	mov    %rax,%rcx
   5909c:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   590a3:	c1 e8 04             	shr    $0x4,%eax
   590a6:	25 ff ff 03 00       	and    $0x3ffff,%eax
   590ab:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   590b2:	3f 
   590b3:	48 8b 44 24 60       	mov    0x60(%rsp),%rax
   590b8:	48 85 c0             	test   %rax,%rax
   590bb:	74 1a                	je     590d7 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x507>
   590bd:	48 89 c1             	mov    %rax,%rcx
   590c0:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   590c7:	c1 e8 04             	shr    $0x4,%eax
   590ca:	25 ff ff 03 00       	and    $0x3ffff,%eax
   590cf:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   590d6:	3f 
   590d7:	48 8b 44 24 68       	mov    0x68(%rsp),%rax
   590dc:	48 85 c0             	test   %rax,%rax
   590df:	74 1a                	je     590fb <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x52b>
   590e1:	48 89 c1             	mov    %rax,%rcx
   590e4:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   590eb:	c1 e8 04             	shr    $0x4,%eax
   590ee:	25 ff ff 03 00       	and    $0x3ffff,%eax
   590f3:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   590fa:	3f 
   590fb:	48 8b 44 24 70       	mov    0x70(%rsp),%rax
   59100:	48 85 c0             	test   %rax,%rax
   59103:	74 1a                	je     5911f <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x54f>
   59105:	48 89 c1             	mov    %rax,%rcx
   59108:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5910f:	c1 e8 04             	shr    $0x4,%eax
   59112:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59117:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   5911e:	3f 
   5911f:	48 8b 44 24 78       	mov    0x78(%rsp),%rax
   59124:	48 85 c0             	test   %rax,%rax
   59127:	74 1a                	je     59143 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x573>
   59129:	48 89 c1             	mov    %rax,%rcx
   5912c:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59133:	c1 e8 04             	shr    $0x4,%eax
   59136:	25 ff ff 03 00       	and    $0x3ffff,%eax
   5913b:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59142:	3f 
   59143:	48 8b 84 24 80 00 00 	mov    0x80(%rsp),%rax
   5914a:	00 
   5914b:	48 85 c0             	test   %rax,%rax
   5914e:	74 1a                	je     5916a <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x59a>
   59150:	48 89 c1             	mov    %rax,%rcx
   59153:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5915a:	c1 e8 04             	shr    $0x4,%eax
   5915d:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59162:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59169:	3f 
   5916a:	48 8b 84 24 88 00 00 	mov    0x88(%rsp),%rax
   59171:	00 
   59172:	48 85 c0             	test   %rax,%rax
   59175:	74 1a                	je     59191 <perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b+0x5c1>
   59177:	48 89 c1             	mov    %rax,%rcx
   5917a:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   59181:	c1 e8 04             	shr    $0x4,%eax
   59184:	25 ff ff 03 00       	and    $0x3ffff,%eax
   59189:	80 a4 01 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rax,1)
   59190:	3f 
   59191:	48 89 54 24 08       	mov    %rdx,0x8(%rsp)
   59196:	48 8d 44 24 08       	lea    0x8(%rsp),%rax
   5919b:	48 81 c4 98 00 00 00 	add    $0x98,%rsp
   591a2:	5b                   	pop    %rbx
   591a3:	41 5c                	pop    %r12
   591a5:	41 5d                	pop    %r13
   591a7:	41 5e                	pop    %r14
   591a9:	41 5f                	pop    %r15
   591ab:	5d                   	pop    %rbp
   591ac:	c3                   	ret
   591ad:	48 8d 3d db 8e fb ff 	lea    -0x47125(%rip),%rdi        # 1208f <panic_unwind::imp::CANARY+0x194>
   591b4:	48 8d 15 0d a2 06 00 	lea    0x6a20d(%rip),%rdx        # c33c8 <anon.07bf7b68542f29918c2fe01e2698508b.1766.llvm.13623703653819427234+0x680>
   591bb:	be 21 00 00 00       	mov    $0x21,%esi
   591c0:	ff 15 82 2c 07 00    	call   *0x72c82(%rip)        # cbe48 <_DYNAMIC+0x3e8>
   591c6:	cc                   	int3
   591c7:	cc                   	int3
   591c8:	cc                   	int3
   591c9:	cc                   	int3
   591ca:	cc                   	int3
   591cb:	cc                   	int3
   591cc:	cc                   	int3
   591cd:	cc                   	int3
   591ce:	cc                   	int3
   591cf:	cc                   	int3


000000000005d020 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix>:
   5d020:	55                   	push   %rbp
   5d021:	41 57                	push   %r15
   5d023:	41 56                	push   %r14
   5d025:	41 55                	push   %r13
   5d027:	41 54                	push   %r12
   5d029:	53                   	push   %rbx
   5d02a:	48 81 ec 98 00 00 00 	sub    $0x98,%rsp
   5d031:	4c 8d 74 24 10       	lea    0x10(%rsp),%r14
   5d036:	4c 89 f7             	mov    %r14,%rdi
   5d039:	ff 15 e9 f0 06 00    	call   *0x6f0e9(%rip)        # cc128 <_DYNAMIC+0x6c8>
   5d03f:	4c 8b 7c 24 10       	mov    0x10(%rsp),%r15
   5d044:	4d 85 ff             	test   %r15,%r15
   5d047:	0f 84 5b 03 00 00    	je     5d3a8 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x388>
   5d04d:	0f 57 c0             	xorps  %xmm0,%xmm0
   5d050:	0f 29 84 24 80 00 00 	movaps %xmm0,0x80(%rsp)
   5d057:	00 
   5d058:	0f 29 44 24 70       	movaps %xmm0,0x70(%rsp)
   5d05d:	0f 29 44 24 60       	movaps %xmm0,0x60(%rsp)
   5d062:	0f 29 44 24 50       	movaps %xmm0,0x50(%rsp)
   5d067:	0f 29 44 24 40       	movaps %xmm0,0x40(%rsp)
   5d06c:	0f 29 44 24 30       	movaps %xmm0,0x30(%rsp)
   5d071:	0f 29 44 24 20       	movaps %xmm0,0x20(%rsp)
   5d076:	0f 29 44 24 10       	movaps %xmm0,0x10(%rsp)
   5d07b:	31 db                	xor    %ebx,%ebx
   5d07d:	4c 8b 25 44 f1 06 00 	mov    0x6f144(%rip),%r12        # cc1c8 <_DYNAMIC+0x768>
   5d084:	66 66 66 2e 0f 1f 84 	data16 data16 cs nopw 0x0(%rax,%rax,1)
   5d08b:	00 00 00 00 00 
   5d090:	41 0f b6 87 c0 04 00 	movzbl 0x4c0(%r15),%eax
   5d097:	00 
   5d098:	48 85 c0             	test   %rax,%rax
   5d09b:	74 46                	je     5d0e3 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0xc3>
   5d09d:	48 ff c8             	dec    %rax
   5d0a0:	41 88 87 c0 04 00 00 	mov    %al,0x4c0(%r15)
   5d0a7:	83 e0 0f             	and    $0xf,%eax
   5d0aa:	49 8b 84 c7 c8 04 00 	mov    0x4c8(%r15,%rax,8),%rax
   5d0b1:	00 
   5d0b2:	48 89 c1             	mov    %rax,%rcx
   5d0b5:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   5d0bc:	89 c2                	mov    %eax,%edx
   5d0be:	c1 ea 04             	shr    $0x4,%edx
   5d0c1:	81 e2 ff ff 03 00    	and    $0x3ffff,%edx
   5d0c7:	80 a4 11 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rdx,1)
   5d0ce:	3f 
   5d0cf:	48 89 44 1c 10       	mov    %rax,0x10(%rsp,%rbx,1)
   5d0d4:	48 83 c3 08          	add    $0x8,%rbx
   5d0d8:	48 81 fb 80 00 00 00 	cmp    $0x80,%rbx
   5d0df:	75 af                	jne    5d090 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x70>
   5d0e1:	eb 1b                	jmp    5d0fe <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0xde>
   5d0e3:	49 8d 7f 10          	lea    0x10(%r15),%rdi
   5d0e7:	31 f6                	xor    %esi,%esi
   5d0e9:	41 ff d4             	call   *%r12
   5d0ec:	48 89 44 1c 10       	mov    %rax,0x10(%rsp,%rbx,1)
   5d0f1:	48 83 c3 08          	add    $0x8,%rbx
   5d0f5:	48 81 fb 80 00 00 00 	cmp    $0x80,%rbx
   5d0fc:	75 92                	jne    5d090 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x70>
   5d0fe:	4c 89 74 24 08       	mov    %r14,0x8(%rsp)
   5d103:	48 8d 44 24 08       	lea    0x8(%rsp),%rax
   5d108:	4d 8d 77 40          	lea    0x40(%r15),%r14
   5d10c:	45 31 e4             	xor    %r12d,%r12d
   5d10f:	4c 8b 2d 2a f0 06 00 	mov    0x6f02a(%rip),%r13        # cc140 <_DYNAMIC+0x6e0>
   5d116:	48 8b 1d 4b f0 06 00 	mov    0x6f04b(%rip),%rbx        # cc168 <_DYNAMIC+0x708>
   5d11d:	48 8b 2d 2c f0 06 00 	mov    0x6f02c(%rip),%rbp        # cc150 <_DYNAMIC+0x6f0>
   5d124:	eb 1f                	jmp    5d145 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x125>
   5d126:	66 2e 0f 1f 84 00 00 	cs nopw 0x0(%rax,%rax,1)
   5d12d:	00 00 00 
   5d130:	f0 48 ff 03          	lock incq (%rbx)
   5d134:	49 83 c4 08          	add    $0x8,%r12
   5d138:	49 81 fc 80 00 00 00 	cmp    $0x80,%r12
   5d13f:	0f 84 51 02 00 00    	je     5d396 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x376>
   5d145:	4a 8b 7c 24 10       	mov    0x10(%rsp,%r12,1),%rdi
   5d14a:	48 85 ff             	test   %rdi,%rdi
   5d14d:	74 e5                	je     5d134 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x114>
   5d14f:	48 89 f8             	mov    %rdi,%rax
   5d152:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   5d158:	49 39 87 40 01 00 00 	cmp    %rax,0x140(%r15)
   5d15f:	75 1f                	jne    5d180 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x160>
   5d161:	f0 49 ff 45 00       	lock incq 0x0(%r13)
   5d166:	4d 8b 87 48 01 00 00 	mov    0x148(%r15),%r8
   5d16d:	41 0f b6 40 1c       	movzbl 0x1c(%r8),%eax
   5d172:	83 f8 02             	cmp    $0x2,%eax
   5d175:	74 57                	je     5d1ce <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x1ae>
   5d177:	e9 3b 01 00 00       	jmp    5d2b7 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x297>
   5d17c:	0f 1f 40 00          	nopl   0x0(%rax)
   5d180:	48 89 c2             	mov    %rax,%rdx
   5d183:	48 c1 ea 16          	shr    $0x16,%rdx
   5d187:	89 d1                	mov    %edx,%ecx
   5d189:	83 e1 0f             	and    $0xf,%ecx
   5d18c:	c1 e1 04             	shl    $0x4,%ecx
   5d18f:	49 39 04 0e          	cmp    %rax,(%r14,%rcx,1)
   5d193:	75 7f                	jne    5d214 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x1f4>
   5d195:	4d 8b 44 0e 08       	mov    0x8(%r14,%rcx,1),%r8
   5d19a:	f0 49 ff 45 00       	lock incq 0x0(%r13)
   5d19f:	49 39 c0             	cmp    %rax,%r8
   5d1a2:	75 1c                	jne    5d1c0 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x1a0>
   5d1a4:	49 89 87 40 01 00 00 	mov    %rax,0x140(%r15)
   5d1ab:	4d 89 87 48 01 00 00 	mov    %r8,0x148(%r15)
   5d1b2:	66 66 66 66 66 2e 0f 	data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   5d1b9:	1f 84 00 00 00 00 00 
   5d1c0:	41 0f b6 40 1c       	movzbl 0x1c(%r8),%eax
   5d1c5:	83 f8 02             	cmp    $0x2,%eax
   5d1c8:	0f 85 e9 00 00 00    	jne    5d2b7 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x297>
   5d1ce:	48 89 fe             	mov    %rdi,%rsi
   5d1d1:	4c 29 c6             	sub    %r8,%rsi
   5d1d4:	49 3b 70 38          	cmp    0x38(%r8),%rsi
   5d1d8:	0f 83 6c 01 00 00    	jae    5d34a <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x32a>
   5d1de:	49 8b 40 30          	mov    0x30(%r8),%rax
   5d1e2:	49 8b 48 08          	mov    0x8(%r8),%rcx
   5d1e6:	4c 01 c0             	add    %r8,%rax
   5d1e9:	0f 82 41 ff ff ff    	jb     5d130 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5d1ef:	48 39 f8             	cmp    %rdi,%rax
   5d1f2:	0f 85 38 ff ff ff    	jne    5d130 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5d1f8:	4c 01 c6             	add    %r8,%rsi
   5d1fb:	ba 08 00 00 00       	mov    $0x8,%edx
   5d200:	b9 10 00 00 00       	mov    $0x10,%ecx
   5d205:	49 8d 7f 10          	lea    0x10(%r15),%rdi
   5d209:	ff 15 51 ef 06 00    	call   *0x6ef51(%rip)        # cc160 <_DYNAMIC+0x700>
   5d20f:	e9 20 ff ff ff       	jmp    5d134 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x114>
   5d214:	4c 01 f1             	add    %r14,%rcx
   5d217:	f0 48 ff 45 00       	lock incq 0x0(%rbp)
   5d21c:	48 89 c6             	mov    %rax,%rsi
   5d21f:	48 c1 ee 23          	shr    $0x23,%rsi
   5d223:	49 89 c0             	mov    %rax,%r8
   5d226:	49 c1 e8 30          	shr    $0x30,%r8
   5d22a:	44 31 c6             	xor    %r8d,%esi
   5d22d:	31 f2                	xor    %esi,%edx
   5d22f:	81 e2 ff 1f 00 00    	and    $0x1fff,%edx
   5d235:	49 8b b7 50 01 00 00 	mov    0x150(%r15),%rsi
   5d23c:	49 89 d1             	mov    %rdx,%r9
   5d23f:	90                   	nop
   5d240:	4e 8b 04 ce          	mov    (%rsi,%r9,8),%r8
   5d244:	4d 85 c0             	test   %r8,%r8
   5d247:	0f 84 fd 00 00 00    	je     5d34a <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x32a>
   5d24d:	4d 89 c2             	mov    %r8,%r10
   5d250:	49 81 e2 00 00 c0 ff 	and    $0xffffffffffc00000,%r10
   5d257:	49 39 c2             	cmp    %rax,%r10
   5d25a:	74 14                	je     5d270 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x250>
   5d25c:	41 ff c1             	inc    %r9d
   5d25f:	41 81 e1 ff 1f 00 00 	and    $0x1fff,%r9d
   5d266:	49 39 d1             	cmp    %rdx,%r9
   5d269:	75 d5                	jne    5d240 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x220>
   5d26b:	e9 da 00 00 00       	jmp    5d34a <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x32a>
   5d270:	4c 89 c2             	mov    %r8,%rdx
   5d273:	48 81 e2 ff ff 3f 00 	and    $0x3fffff,%rdx
   5d27a:	74 26                	je     5d2a2 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x282>
   5d27c:	48 8d 72 ff          	lea    -0x1(%rdx),%rsi
   5d280:	45 8b 87 68 01 00 00 	mov    0x168(%r15),%r8d
   5d287:	4c 39 c6             	cmp    %r8,%rsi
   5d28a:	0f 83 ba 00 00 00    	jae    5d34a <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x32a>
   5d290:	49 8b 77 30          	mov    0x30(%r15),%rsi
   5d294:	4c 8b 44 d6 f8       	mov    -0x8(%rsi,%rdx,8),%r8
   5d299:	4d 85 c0             	test   %r8,%r8
   5d29c:	0f 84 a8 00 00 00    	je     5d34a <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x32a>
   5d2a2:	48 89 01             	mov    %rax,(%rcx)
   5d2a5:	4c 89 41 08          	mov    %r8,0x8(%rcx)
   5d2a9:	41 0f b6 40 1c       	movzbl 0x1c(%r8),%eax
   5d2ae:	83 f8 02             	cmp    $0x2,%eax
   5d2b1:	0f 84 17 ff ff ff    	je     5d1ce <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x1ae>
   5d2b7:	41 0f b6 8f c0 04 00 	movzbl 0x4c0(%r15),%ecx
   5d2be:	00 
   5d2bf:	85 c0                	test   %eax,%eax
   5d2c1:	74 20                	je     5d2e3 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x2c3>
   5d2c3:	83 f8 01             	cmp    $0x1,%eax
   5d2c6:	0f 85 64 fe ff ff    	jne    5d130 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5d2cc:	b8 00 20 04 00       	mov    $0x42000,%eax
   5d2d1:	89 fe                	mov    %edi,%esi
   5d2d3:	81 e6 ff ff 3f 00    	and    $0x3fffff,%esi
   5d2d9:	39 c6                	cmp    %eax,%esi
   5d2db:	0f 82 4f fe ff ff    	jb     5d130 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5d2e1:	eb 15                	jmp    5d2f8 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x2d8>
   5d2e3:	b8 00 f0 05 00       	mov    $0x5f000,%eax
   5d2e8:	89 fe                	mov    %edi,%esi
   5d2ea:	81 e6 ff ff 3f 00    	and    $0x3fffff,%esi
   5d2f0:	39 c6                	cmp    %eax,%esi
   5d2f2:	0f 82 38 fe ff ff    	jb     5d130 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5d2f8:	40 f6 c7 0f          	test   $0xf,%dil
   5d2fc:	0f 85 32 fe ff ff    	jne    5d134 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x114>
   5d302:	89 f0                	mov    %esi,%eax
   5d304:	c1 e8 04             	shr    $0x4,%eax
   5d307:	41 0f b6 94 00 88 15 	movzbl 0x1588(%r8,%rax,1),%edx
   5d30e:	00 00 
   5d310:	85 d2                	test   %edx,%edx
   5d312:	74 4b                	je     5d35f <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x33f>
   5d314:	83 fa 01             	cmp    $0x1,%edx
   5d317:	75 54                	jne    5d36d <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x34d>
   5d319:	49 3b 30             	cmp    (%r8),%rsi
   5d31c:	0f 83 0e fe ff ff    	jae    5d130 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5d322:	4c 01 c6             	add    %r8,%rsi
   5d325:	80 f9 10             	cmp    $0x10,%cl
   5d328:	73 51                	jae    5d37b <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x35b>
   5d32a:	41 c6 84 00 88 15 00 	movb   $0x41,0x1588(%r8,%rax,1)
   5d331:	00 41 
   5d333:	49 89 b4 cf c8 04 00 	mov    %rsi,0x4c8(%r15,%rcx,8)
   5d33a:	00 
   5d33b:	8d 41 01             	lea    0x1(%rcx),%eax
   5d33e:	41 88 87 c0 04 00 00 	mov    %al,0x4c0(%r15)
   5d345:	e9 ea fd ff ff       	jmp    5d134 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x114>
   5d34a:	be 08 00 00 00       	mov    $0x8,%esi
   5d34f:	ba 10 00 00 00       	mov    $0x10,%edx
   5d354:	ff 15 1e ee 06 00    	call   *0x6ee1e(%rip)        # cc178 <_DYNAMIC+0x718>
   5d35a:	e9 d5 fd ff ff       	jmp    5d134 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x114>
   5d35f:	49 3b 30             	cmp    (%r8),%rsi
   5d362:	0f 83 c8 fd ff ff    	jae    5d130 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5d368:	e9 c7 fd ff ff       	jmp    5d134 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x114>
   5d36d:	80 fa 40             	cmp    $0x40,%dl
   5d370:	0f 83 ba fd ff ff    	jae    5d130 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x110>
   5d376:	e9 b9 fd ff ff       	jmp    5d134 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x114>
   5d37b:	49 01 c0             	add    %rax,%r8
   5d37e:	49 81 c0 88 15 00 00 	add    $0x1588,%r8
   5d385:	49 8d 7f 10          	lea    0x10(%r15),%rdi
   5d389:	31 d2                	xor    %edx,%edx
   5d38b:	ff 15 ef ed 06 00    	call   *0x6edef(%rip)        # cc180 <_DYNAMIC+0x720>
   5d391:	e9 9e fd ff ff       	jmp    5d134 <perf_gate_iai::alloc_clear_magazine_only_16b_prefix::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b_prefix+0x114>
   5d396:	48 81 c4 98 00 00 00 	add    $0x98,%rsp
   5d39d:	5b                   	pop    %rbx
   5d39e:	41 5c                	pop    %r12
   5d3a0:	41 5d                	pop    %r13
   5d3a2:	41 5e                	pop    %r14
   5d3a4:	41 5f                	pop    %r15
   5d3a6:	5d                   	pop    %rbp
   5d3a7:	c3                   	ret
   5d3a8:	48 8d 3d e0 4c fb ff 	lea    -0x4b320(%rip),%rdi        # 1208f <panic_unwind::imp::CANARY+0x194>
   5d3af:	48 8d 15 12 60 06 00 	lea    0x66012(%rip),%rdx        # c33c8 <anon.07bf7b68542f29918c2fe01e2698508b.1766.llvm.13623703653819427234+0x680>
   5d3b6:	be 21 00 00 00       	mov    $0x21,%esi
   5d3bb:	ff 15 87 ea 06 00    	call   *0x6ea87(%rip)        # cbe48 <_DYNAMIC+0x3e8>
   5d3c1:	cc                   	int3
   5d3c2:	cc                   	int3
   5d3c3:	cc                   	int3
   5d3c4:	cc                   	int3
   5d3c5:	cc                   	int3
   5d3c6:	cc                   	int3
   5d3c7:	cc                   	int3
   5d3c8:	cc                   	int3
   5d3c9:	cc                   	int3
   5d3ca:	cc                   	int3
   5d3cb:	cc                   	int3
   5d3cc:	cc                   	int3
   5d3cd:	cc                   	int3
   5d3ce:	cc                   	int3
   5d3cf:	cc                   	int3

