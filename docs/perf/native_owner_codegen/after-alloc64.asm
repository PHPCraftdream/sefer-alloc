   140006ef0:	56                   	push   %rsi
   140006ef1:	48 83 ec 20          	sub    $0x20,%rsp
   140006ef5:	48 89 ce             	mov    %rcx,%rsi
   140006ef8:	8b 05 d2 0c 06 00    	mov    0x60cd2(%rip),%eax        # 0x140067bd0
   140006efe:	65 48 8b 0c 25 58 00 	mov    %gs:0x58,%rcx
   140006f05:	00 00 
   140006f07:	48 8b 04 c1          	mov    (%rcx,%rax,8),%rax
   140006f0b:	48 8b 88 38 00 00 00 	mov    0x38(%rax),%rcx
   140006f12:	48 8d 41 ff          	lea    -0x1(%rcx),%rax
   140006f16:	48 83 f8 fe          	cmp    $0xfffffffffffffffe,%rax
   140006f1a:	73 43                	jae    0x140006f5f
   140006f1c:	0f b6 81 40 06 00 00 	movzbl 0x640(%rcx),%eax
   140006f23:	48 85 c0             	test   %rax,%rax
   140006f26:	74 71                	je     0x140006f99
   140006f28:	48 ff c8             	dec    %rax
   140006f2b:	88 81 40 06 00 00    	mov    %al,0x640(%rcx)
   140006f31:	83 e0 0f             	and    $0xf,%eax
   140006f34:	48 8b 84 c1 48 06 00 	mov    0x648(%rcx,%rax,8),%rax
   140006f3b:	00 
   140006f3c:	48 89 c1             	mov    %rax,%rcx
   140006f3f:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   140006f46:	89 c2                	mov    %eax,%edx
   140006f48:	c1 ea 04             	shr    $0x4,%edx
   140006f4b:	81 e2 ff ff 03 00    	and    $0x3ffff,%edx
   140006f51:	80 a4 11 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rdx,1)
   140006f58:	3f 
   140006f59:	48 83 c4 20          	add    $0x20,%rsp
   140006f5d:	5e                   	pop    %rsi
   140006f5e:	c3                   	ret
   140006f5f:	48 85 c9             	test   %rcx,%rcx
   140006f62:	74 18                	je     0x140006f7c
   140006f64:	ba 08 00 00 00       	mov    $0x8,%edx
   140006f69:	41 b8 40 00 00 00    	mov    $0x40,%r8d
   140006f6f:	48 89 f1             	mov    %rsi,%rcx
   140006f72:	48 83 c4 20          	add    $0x20,%rsp
   140006f76:	5e                   	pop    %rsi
   140006f77:	e9 04 36 01 00       	jmp    0x14001a580
   140006f7c:	48 89 f1             	mov    %rsi,%rcx
   140006f7f:	e8 cc 55 03 00       	call   0x14003c550
   140006f84:	48 83 f8 01          	cmp    $0x1,%rax
   140006f88:	74 da                	je     0x140006f64
   140006f8a:	48 89 d1             	mov    %rdx,%rcx
   140006f8d:	0f b6 81 40 06 00 00 	movzbl 0x640(%rcx),%eax
   140006f94:	48 85 c0             	test   %rax,%rax
   140006f97:	75 8f                	jne    0x140006f28
   140006f99:	ba 03 00 00 00       	mov    $0x3,%edx
   140006f9e:	e8 4d 44 03 00       	call   0x14003b3f0
   140006fa3:	48 85 c0             	test   %rax,%rax
   140006fa6:	75 b1                	jne    0x140006f59
   140006fa8:	48 8b 05 a9 6c 05 00 	mov    0x56ca9(%rip),%rax        # 0x14005dc58
   140006faf:	48 85 c0             	test   %rax,%rax
   140006fb2:	74 14                	je     0x140006fc8
   140006fb4:	48 83 f8 01          	cmp    $0x1,%rax
   140006fb8:	74 06                	je     0x140006fc0
   140006fba:	ff d0                	call   *%rax
   140006fbc:	84 c0                	test   %al,%al
   140006fbe:	75 15                	jne    0x140006fd5
   140006fc0:	31 c0                	xor    %eax,%eax
   140006fc2:	48 83 c4 20          	add    $0x20,%rsp
   140006fc6:	5e                   	pop    %rsi
   140006fc7:	c3                   	ret
   140006fc8:	e8 13 4f 03 00       	call   0x14003bee0
   140006fcd:	48 83 f8 01          	cmp    $0x1,%rax
   140006fd1:	75 e7                	jne    0x140006fba
   140006fd3:	eb eb                	jmp    0x140006fc0
   140006fd5:	b9 08 00 00 00       	mov    $0x8,%ecx
   140006fda:	ba 40 00 00 00       	mov    $0x40,%edx
   140006fdf:	45 31 c0             	xor    %r8d,%r8d
   140006fe2:	48 83 c4 20          	add    $0x20,%rsp
   140006fe6:	5e                   	pop    %rsi
   140006fe7:	e9 74 56 03 00       	jmp    0x14003c660
