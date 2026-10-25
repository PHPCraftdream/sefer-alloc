   140006ea0:	56                   	push   %rsi
   140006ea1:	48 83 ec 20          	sub    $0x20,%rsp
   140006ea5:	48 89 ce             	mov    %rcx,%rsi
   140006ea8:	8b 05 22 0d 06 00    	mov    0x60d22(%rip),%eax        # 0x140067bd0
   140006eae:	65 48 8b 0c 25 58 00 	mov    %gs:0x58,%rcx
   140006eb5:	00 00 
   140006eb7:	48 8b 04 c1          	mov    (%rcx,%rax,8),%rax
   140006ebb:	48 8b 88 38 00 00 00 	mov    0x38(%rax),%rcx
   140006ec2:	48 8d 41 ff          	lea    -0x1(%rcx),%rax
   140006ec6:	48 83 f8 fe          	cmp    $0xfffffffffffffffe,%rax
   140006eca:	73 43                	jae    0x140006f0f
   140006ecc:	0f b6 81 a8 04 00 00 	movzbl 0x4a8(%rcx),%eax
   140006ed3:	48 85 c0             	test   %rax,%rax
   140006ed6:	74 71                	je     0x140006f49
   140006ed8:	48 ff c8             	dec    %rax
   140006edb:	88 81 a8 04 00 00    	mov    %al,0x4a8(%rcx)
   140006ee1:	83 e0 0f             	and    $0xf,%eax
   140006ee4:	48 8b 84 c1 b0 04 00 	mov    0x4b0(%rcx,%rax,8),%rax
   140006eeb:	00 
   140006eec:	48 89 c1             	mov    %rax,%rcx
   140006eef:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   140006ef6:	89 c2                	mov    %eax,%edx
   140006ef8:	c1 ea 04             	shr    $0x4,%edx
   140006efb:	81 e2 ff ff 03 00    	and    $0x3ffff,%edx
   140006f01:	80 a4 11 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rdx,1)
   140006f08:	3f 
   140006f09:	48 83 c4 20          	add    $0x20,%rsp
   140006f0d:	5e                   	pop    %rsi
   140006f0e:	c3                   	ret
   140006f0f:	48 85 c9             	test   %rcx,%rcx
   140006f12:	74 18                	je     0x140006f2c
   140006f14:	ba 08 00 00 00       	mov    $0x8,%edx
   140006f19:	41 b8 10 00 00 00    	mov    $0x10,%r8d
   140006f1f:	48 89 f1             	mov    %rsi,%rcx
   140006f22:	48 83 c4 20          	add    $0x20,%rsp
   140006f26:	5e                   	pop    %rsi
   140006f27:	e9 d4 3b 01 00       	jmp    0x14001ab00
   140006f2c:	48 89 f1             	mov    %rsi,%rcx
   140006f2f:	e8 2c 5b 03 00       	call   0x14003ca60
   140006f34:	48 83 f8 01          	cmp    $0x1,%rax
   140006f38:	74 da                	je     0x140006f14
   140006f3a:	48 89 d1             	mov    %rdx,%rcx
   140006f3d:	0f b6 81 a8 04 00 00 	movzbl 0x4a8(%rcx),%eax
   140006f44:	48 85 c0             	test   %rax,%rax
   140006f47:	75 8f                	jne    0x140006ed8
   140006f49:	31 d2                	xor    %edx,%edx
   140006f4b:	e8 b0 49 03 00       	call   0x14003b900
   140006f50:	48 85 c0             	test   %rax,%rax
   140006f53:	75 b4                	jne    0x140006f09
   140006f55:	48 8b 05 fc 6c 05 00 	mov    0x56cfc(%rip),%rax        # 0x14005dc58
   140006f5c:	48 85 c0             	test   %rax,%rax
   140006f5f:	74 14                	je     0x140006f75
   140006f61:	48 83 f8 01          	cmp    $0x1,%rax
   140006f65:	74 06                	je     0x140006f6d
   140006f67:	ff d0                	call   *%rax
   140006f69:	84 c0                	test   %al,%al
   140006f6b:	75 15                	jne    0x140006f82
   140006f6d:	31 c0                	xor    %eax,%eax
   140006f6f:	48 83 c4 20          	add    $0x20,%rsp
   140006f73:	5e                   	pop    %rsi
   140006f74:	c3                   	ret
   140006f75:	e8 76 54 03 00       	call   0x14003c3f0
   140006f7a:	48 83 f8 01          	cmp    $0x1,%rax
   140006f7e:	75 e7                	jne    0x140006f67
   140006f80:	eb eb                	jmp    0x140006f6d
   140006f82:	b9 08 00 00 00       	mov    $0x8,%ecx
   140006f87:	ba 10 00 00 00       	mov    $0x10,%edx
   140006f8c:	45 31 c0             	xor    %r8d,%r8d
   140006f8f:	48 83 c4 20          	add    $0x20,%rsp
   140006f93:	5e                   	pop    %rsi
   140006f94:	e9 d7 5b 03 00       	jmp    0x14003cb70
