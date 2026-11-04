0000000000030ac0 <nl9_xfree::producer>:
   30ac0:	test   %rsi,%rsi
   30ac3:	je     30ce6 <nl9_xfree::producer+0x226>
   30ac9:	push   %rbp
   30aca:	push   %r15
   30acc:	push   %r14
   30ace:	push   %r13
   30ad0:	push   %r12
   30ad2:	push   %rbx
   30ad3:	sub    $0x18,%rsp
   30ad7:	mov    %rsi,%rbx
   30ada:	shl    $0x3,%rbx
   30ade:	cmp    $0x10,%rdx
   30ae2:	ja     30d00 <nl9_xfree::producer+0x240>
   30ae8:	lea    -0x1(%rcx),%r14
   30aec:	cmp    $0x3f2bf,%r14
   30af3:	ja     30d00 <nl9_xfree::producer+0x240>
   30af9:	shr    $0x4,%r14
   30afd:	xor    %r15d,%r15d
   30b00:	mov    0x5ee29(%rip),%r13        # 8f930 <_DYNAMIC+0x5d0>
   30b07:	mov    0x5ee3a(%rip),%rbp        # 8f948 <_DYNAMIC+0x5e8>
   30b0e:	mov    0x5ee3b(%rip),%r12        # 8f950 <_DYNAMIC+0x5f0>
   30b15:	data16 cs nopw 0x0(%rax,%rax,1)
   30b1c:	00 00 00 00 
   30b20:	mov    (%rdi,%r15,1),%rax
   30b24:	mov    $0xffffffffffffffd8,%rsi
   30b2b:	mov    %fs:(%rsi),%r8
   30b2f:	cmp    $0x2,%r8
   30b33:	jb     30be0 <nl9_xfree::producer+0x120>
   30b39:	mov    %rax,%rsi
   30b3c:	and    $0xffffffffffc00000,%rsi
   30b43:	cmp    %rsi,0x130(%r8)
   30b4a:	jne    30c20 <nl9_xfree::producer+0x160>
   30b50:	mov    0x138(%r8),%r9
   30b57:	mov    %rax,%rsi
   30b5a:	sub    %r9,%rsi
   30b5d:	movzbl 0x1c(%r9),%r10d
   30b62:	cmp    0x0(%rbp,%r10,8),%rsi
   30b67:	jb     30c60 <nl9_xfree::producer+0x1a0>
   30b6d:	movzbl 0x0(%r13,%r14,1),%esi
   30b73:	dec    %rsi
   30b76:	mov    %rsi,%r10
   30b79:	shl    $0x7,%r10
   30b7d:	lea    (%r10,%rsi,8),%r11
   30b81:	movzbl 0x4a8(%r8,%r11,1),%r10d
   30b88:	00 00 
   30b8a:	cmp    (%r12,%rsi,1),%r10b
   30b8e:	jae    30ca0 <nl9_xfree::producer+0x1e0>
   30b94:	lea    (%r8,%r11,1),%rsi
   30b98:	add    $0x4a8,%rsi
   30b9f:	and    $0xf,%r10d
   30ba3:	mov    %r10d,%r8d
   30ba6:	mov    %rax,0x8(%rsi,%r8,8)
   30bab:	lea    0x1(%r10),%eax
   30baf:	mov    %al,(%rsi)
   30bb1:	add    $0x8,%r15
   30bb5:	cmp    %r15,%rbx
   30bb8:	jne    30b20 <nl9_xfree::producer+0x60>
   30bbe:	jmp    30cd8 <nl9_xfree::producer+0x218>
   30bc3:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30bca:	0f 1f 84 00 00 00 00 
   30bd1:	00 
   30bd2:	data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30bd9:	1f 84 00 00 00 00 00 
   30be0:	mov    %rdi,0x10(%rsp)
   30be5:	mov    %rax,%rdi
   30be8:	mov    %rdx,%rsi
   30beb:	mov    %rdx,0x8(%rsp)
   30bf0:	mov    %rcx,%rdx
   30bf3:	mov    %rcx,(%rsp)
   30bf7:	call   *0x5ed63(%rip)        # 8f960 <_DYNAMIC+0x600>
   30bfd:	jmp    30cbd <nl9_xfree::producer+0x1fd>
   30c02:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30c09:	0f 1f 84 00 00 00 00 
   30c10:	00 
   30c11:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30c18:	0f 1f 84 00 00 00 00 
   30c1f:	00 
   30c20:	mov    %rdi,0x10(%rsp)
   30c25:	mov    %rax,%rdi
   30c28:	mov    %rdx,%rsi
   30c2b:	mov    %rdx,0x8(%rsp)
   30c30:	mov    %rcx,%rdx
   30c33:	mov    %rcx,(%rsp)
   30c37:	mov    %r8,%rcx
   30c3a:	call   *0x5ed28(%rip)        # 8f968 <_DYNAMIC+0x608>
   30c40:	jmp    30cbd <nl9_xfree::producer+0x1fd>
   30c42:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30c49:	0f 1f 84 00 00 00 00 
   30c50:	00 
   30c51:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30c58:	0f 1f 84 00 00 00 00 
   30c5f:	00 
   30c60:	mov    %rdi,0x10(%rsp)
   30c65:	mov    %rax,%rdi
   30c68:	mov    %rdx,%rsi
   30c6b:	mov    %rdx,0x8(%rsp)
   30c70:	mov    %rcx,%rdx
   30c73:	mov    %rcx,(%rsp)
   30c77:	mov    %r8,%rcx
   30c7a:	call   *0x5ecd8(%rip)        # 8f958 <_DYNAMIC+0x5f8>
   30c80:	jmp    30cbd <nl9_xfree::producer+0x1fd>
   30c82:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30c89:	0f 1f 84 00 00 00 00 
   30c90:	00 
   30c91:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30c98:	0f 1f 84 00 00 00 00 
   30c9f:	00 
   30ca0:	mov    %rdi,0x10(%rsp)
   30ca5:	mov    %r8,%rdi
   30ca8:	mov    %rdx,0x8(%rsp)
   30cad:	mov    %r9,%rdx
   30cb0:	mov    %rcx,(%rsp)
   30cb4:	mov    %rax,%rcx
   30cb7:	call   *0x5ecb3(%rip)        # 8f970 <_DYNAMIC+0x610>
   30cbd:	mov    0x10(%rsp),%rdi
   30cc2:	mov    0x8(%rsp),%rdx
   30cc7:	mov    (%rsp),%rcx
   30ccb:	add    $0x8,%r15
   30ccf:	cmp    %r15,%rbx
   30cd2:	jne    30b20 <nl9_xfree::producer+0x60>
   30cd8:	add    $0x18,%rsp
   30cdc:	pop    %rbx
   30cdd:	pop    %r12
   30cdf:	pop    %r13
   30ce1:	pop    %r14
   30ce3:	pop    %r15
   30ce5:	pop    %rbp
   30ce6:	ret
   30ce7:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30cee:	0f 1f 84 00 00 00 00 
   30cf5:	00 
   30cf6:	cs nopw 0x0(%rax,%rax,1)
   30cfd:	00 00 00 
   30d00:	xor    %r14d,%r14d
   30d03:	mov    0x5ec4e(%rip),%r12        # 8f958 <_DYNAMIC+0x5f8>
   30d0a:	mov    %rdi,%rbp
   30d0d:	mov    (%rdi,%r14,1),%rdi
   30d11:	mov    $0xffffffffffffffd8,%rax
   30d18:	mov    %fs:(%rax),%rax
   30d1c:	mov    %rdx,%r13
   30d1f:	mov    %rdx,%rsi
   30d22:	mov    %rcx,%r15
   30d25:	mov    %rcx,%rdx
   30d28:	cmp    $0x1,%rax
   30d2c:	jbe    30d60 <nl9_xfree::producer+0x2a0>
   30d2e:	mov    %rax,%rcx
   30d31:	call   *%r12
   30d34:	add    $0x8,%r14
   30d38:	cmp    %r14,%rbx
   30d3b:	mov    %r15,%rcx
   30d3e:	mov    %r13,%rdx
   30d41:	mov    %rbp,%rdi
   30d44:	jne    30d0a <nl9_xfree::producer+0x24a>
   30d46:	jmp    30cd8 <nl9_xfree::producer+0x218>
   30d48:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30d4f:	0f 1f 84 00 00 00 00 
   30d56:	00 
   30d57:	nopw   0x0(%rax,%rax,1)
   30d5e:	00 00 
   30d60:	call   *0x5ebfa(%rip)        # 8f960 <_DYNAMIC+0x600>
   30d66:	jmp    30d34 <nl9_xfree::producer+0x274>
   30d68:	int3
   30d69:	int3
   30d6a:	int3
   30d6b:	int3
   30d6c:	int3
   30d6d:	int3
   30d6e:	int3
   30d6f:	int3
   30d70:	int3
   30d71:	int3
   30d72:	int3
   30d73:	int3
   30d74:	int3
   30d75:	int3
   30d76:	int3
   30d77:	int3
   30d78:	int3
   30d79:	int3
   30d7a:	int3
   30d7b:	int3
   30d7c:	int3
   30d7d:	int3
   30d7e:	int3
   30d7f:	int3
