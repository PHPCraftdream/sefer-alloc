0000000000030d00 <nl9_xfree::producer>:
   30d00:	test   %rsi,%rsi
   30d03:	je     30edb <nl9_xfree::producer+0x1db>
   30d09:	push   %rbp
   30d0a:	push   %r15
   30d0c:	push   %r14
   30d0e:	push   %r13
   30d10:	push   %r12
   30d12:	push   %rbx
   30d13:	push   %rax
   30d14:	mov    %rsi,%rbx
   30d17:	mov    %rdi,%r14
   30d1a:	shl    $0x3,%rbx
   30d1e:	cmp    $0x10,%rdx
   30d22:	ja     30ee0 <nl9_xfree::producer+0x1e0>
   30d28:	lea    -0x1(%rcx),%r15
   30d2c:	cmp    $0x3f2bf,%r15
   30d33:	ja     30ee0 <nl9_xfree::producer+0x1e0>
   30d39:	shr    $0x4,%r15
   30d3d:	xor    %r12d,%r12d
   30d40:	mov    0x5f809(%rip),%rbp        # 90550 <_DYNAMIC+0x5f0>
   30d47:	nopw   0x0(%rax,%rax,1)
   30d4e:	00 00 
   30d50:	mov    (%r14,%r12,1),%rdi
   30d54:	mov    $0xffffffffffffffd8,%rax
   30d5b:	mov    %fs:(%rax),%rax
   30d5f:	cmp    $0x2,%rax
   30d63:	jb     30e60 <nl9_xfree::producer+0x160>
   30d69:	mov    %rdi,%rsi
   30d6c:	and    $0xffffffffffc00000,%rsi
   30d73:	cmp    %rsi,0x130(%rax)
   30d7a:	jne    30e80 <nl9_xfree::producer+0x180>
   30d80:	mov    0x138(%rax),%r8
   30d87:	mov    %rdi,%r9
   30d8a:	sub    %r8,%r9
   30d8d:	movzbl 0x1c(%r8),%esi
   30d92:	mov    0x5f7af(%rip),%r10        # 90548 <_DYNAMIC+0x5e8>
   30d99:	cmp    (%r10,%rsi,8),%r9
   30d9d:	jb     30e40 <nl9_xfree::producer+0x140>
   30da3:	rol    $0x3c,%r9
   30da7:	mov    (%r8),%rsi
   30daa:	shr    $0x4,%rsi
   30dae:	cmp    %rsi,%r9
   30db1:	jae    30e40 <nl9_xfree::producer+0x140>
   30db7:	mov    0x5f772(%rip),%rsi        # 90530 <_DYNAMIC+0x5d0>
   30dbe:	movzbl (%rsi,%r15,1),%r11d
   30dc3:	cmp    %r11b,0x1588(%r8,%r9,1)
   30dca:	00 
   30dcb:	jne    30e40 <nl9_xfree::producer+0x140>
   30dcd:	mov    %r11,%rsi
   30dd0:	dec    %rsi
   30dd3:	mov    %rsi,%r10
   30dd6:	shl    $0x7,%r10
   30dda:	lea    (%r10,%rsi,8),%r13
   30dde:	movzbl 0x4a8(%rax,%r13,1),%r10d
   30de5:	00 00 
   30de7:	cmp    0x0(%rbp,%rsi,1),%r10b
   30dec:	jae    30ea0 <nl9_xfree::producer+0x1a0>
   30df2:	add    %r13,%rax
   30df5:	add    $0x4a8,%rax
   30dfb:	lea    0x40(%r11),%esi
   30dff:	mov    %sil,0x1588(%r8,%r9,1)
   30e06:	00 
   30e07:	and    $0xf,%r10d
   30e0b:	mov    %r10d,%esi
   30e0e:	mov    %rdi,0x8(%rax,%rsi,8)
   30e13:	lea    0x1(%r10),%esi
   30e17:	mov    %sil,(%rax)
   30e1a:	add    $0x8,%r12
   30e1e:	cmp    %r12,%rbx
   30e21:	jne    30d50 <nl9_xfree::producer+0x50>
   30e27:	jmp    30ecd <nl9_xfree::producer+0x1cd>
   30e2c:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30e33:	0f 1f 84 00 00 00 00 
   30e3a:	00 
   30e3b:	nopl   0x0(%rax,%rax,1)
   30e40:	mov    %rdx,%rsi
   30e43:	mov    %rdx,(%rsp)
   30e47:	mov    %rcx,%rdx
   30e4a:	mov    %rcx,%r13
   30e4d:	mov    %rax,%rcx
   30e50:	call   *0x5f702(%rip)        # 90558 <_DYNAMIC+0x5f8>
   30e56:	jmp    30eb9 <nl9_xfree::producer+0x1b9>
   30e58:	nopl   0x0(%rax,%rax,1)
   30e5f:	00 
   30e60:	mov    %rdx,%rsi
   30e63:	mov    %rdx,(%rsp)
   30e67:	mov    %rcx,%rdx
   30e6a:	mov    %rcx,%r13
   30e6d:	call   *0x5f6ed(%rip)        # 90560 <_DYNAMIC+0x600>
   30e73:	jmp    30eb9 <nl9_xfree::producer+0x1b9>
   30e75:	data16 cs nopw 0x0(%rax,%rax,1)
   30e7c:	00 00 00 00 
   30e80:	mov    %rdx,%rsi
   30e83:	mov    %rdx,(%rsp)
   30e87:	mov    %rcx,%rdx
   30e8a:	mov    %rcx,%r13
   30e8d:	mov    %rax,%rcx
   30e90:	call   *0x5f6d2(%rip)        # 90568 <_DYNAMIC+0x608>
   30e96:	jmp    30eb9 <nl9_xfree::producer+0x1b9>
   30e98:	nopl   0x0(%rax,%rax,1)
   30e9f:	00 
   30ea0:	mov    %rdi,%r9
   30ea3:	mov    %rax,%rdi
   30ea6:	mov    %rdx,(%rsp)
   30eaa:	mov    %r8,%rdx
   30ead:	mov    %rcx,%r13
   30eb0:	mov    %r9,%rcx
   30eb3:	call   *0x5f6b7(%rip)        # 90570 <_DYNAMIC+0x610>
   30eb9:	mov    (%rsp),%rdx
   30ebd:	mov    %r13,%rcx
   30ec0:	add    $0x8,%r12
   30ec4:	cmp    %r12,%rbx
   30ec7:	jne    30d50 <nl9_xfree::producer+0x50>
   30ecd:	add    $0x8,%rsp
   30ed1:	pop    %rbx
   30ed2:	pop    %r12
   30ed4:	pop    %r13
   30ed6:	pop    %r14
   30ed8:	pop    %r15
   30eda:	pop    %rbp
   30edb:	ret
   30edc:	nopl   0x0(%rax)
   30ee0:	xor    %r15d,%r15d
   30ee3:	mov    0x5f66e(%rip),%r13        # 90558 <_DYNAMIC+0x5f8>
   30eea:	mov    (%r14,%r15,1),%rdi
   30eee:	mov    $0xffffffffffffffd8,%rax
   30ef5:	mov    %fs:(%rax),%rax
   30ef9:	mov    %rdx,%rbp
   30efc:	mov    %rdx,%rsi
   30eff:	mov    %rcx,%r12
   30f02:	mov    %rcx,%rdx
   30f05:	cmp    $0x1,%rax
   30f09:	jbe    30f40 <nl9_xfree::producer+0x240>
   30f0b:	mov    %rax,%rcx
   30f0e:	call   *%r13
   30f11:	add    $0x8,%r15
   30f15:	cmp    %r15,%rbx
   30f18:	mov    %r12,%rcx
   30f1b:	mov    %rbp,%rdx
   30f1e:	jne    30eea <nl9_xfree::producer+0x1ea>
   30f20:	jmp    30ecd <nl9_xfree::producer+0x1cd>
   30f22:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30f29:	0f 1f 84 00 00 00 00 
   30f30:	00 
   30f31:	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   30f38:	0f 1f 84 00 00 00 00 
   30f3f:	00 
   30f40:	call   *0x5f61a(%rip)        # 90560 <_DYNAMIC+0x600>
   30f46:	jmp    30f11 <nl9_xfree::producer+0x211>
   30f48:	int3
   30f49:	int3
   30f4a:	int3
   30f4b:	int3
   30f4c:	int3
   30f4d:	int3
   30f4e:	int3
   30f4f:	int3
   30f50:	int3
   30f51:	int3
   30f52:	int3
   30f53:	int3
   30f54:	int3
   30f55:	int3
   30f56:	int3
   30f57:	int3
   30f58:	int3
   30f59:	int3
   30f5a:	int3
   30f5b:	int3
   30f5c:	int3
   30f5d:	int3
   30f5e:	int3
   30f5f:	int3
   30f60:	int3
   30f61:	int3
   30f62:	int3
   30f63:	int3
   30f64:	int3
   30f65:	int3
   30f66:	int3
   30f67:	int3
   30f68:	int3
   30f69:	int3
   30f6a:	int3
   30f6b:	int3
   30f6c:	int3
   30f6d:	int3
   30f6e:	int3
   30f6f:	int3
   30f70:	int3
   30f71:	int3
   30f72:	int3
   30f73:	int3
   30f74:	int3
   30f75:	int3
   30f76:	int3
   30f77:	int3
   30f78:	int3
   30f79:	int3
   30f7a:	int3
   30f7b:	int3
   30f7c:	int3
   30f7d:	int3
   30f7e:	int3
   30f7f:	int3
