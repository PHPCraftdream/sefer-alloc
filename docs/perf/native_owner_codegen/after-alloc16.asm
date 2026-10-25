   140006de0:	56                   	push   %rsi
   140006de1:	48 83 ec 20          	sub    $0x20,%rsp
   140006de5:	48 89 ce             	mov    %rcx,%rsi
   140006de8:	8b 05 e2 0d 06 00    	mov    0x60de2(%rip),%eax        # 0x140067bd0
   140006dee:	65 48 8b 0c 25 58 00 	mov    %gs:0x58,%rcx
   140006df5:	00 00 
   140006df7:	48 8b 04 c1          	mov    (%rcx,%rax,8),%rax
   140006dfb:	48 8b 88 38 00 00 00 	mov    0x38(%rax),%rcx
   140006e02:	48 8d 41 ff          	lea    -0x1(%rcx),%rax
   140006e06:	48 83 f8 fe          	cmp    $0xfffffffffffffffe,%rax
   140006e0a:	73 43                	jae    0x140006e4f
   140006e0c:	0f b6 81 a8 04 00 00 	movzbl 0x4a8(%rcx),%eax
   140006e13:	48 85 c0             	test   %rax,%rax
   140006e16:	74 71                	je     0x140006e89
   140006e18:	48 ff c8             	dec    %rax
   140006e1b:	88 81 a8 04 00 00    	mov    %al,0x4a8(%rcx)
   140006e21:	83 e0 0f             	and    $0xf,%eax
   140006e24:	48 8b 84 c1 b0 04 00 	mov    0x4b0(%rcx,%rax,8),%rax
   140006e2b:	00 
   140006e2c:	48 89 c1             	mov    %rax,%rcx
   140006e2f:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   140006e36:	89 c2                	mov    %eax,%edx
   140006e38:	c1 ea 04             	shr    $0x4,%edx
   140006e3b:	81 e2 ff ff 03 00    	and    $0x3ffff,%edx
   140006e41:	80 a4 11 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rdx,1)
   140006e48:	3f 
   140006e49:	48 83 c4 20          	add    $0x20,%rsp
   140006e4d:	5e                   	pop    %rsi
   140006e4e:	c3                   	ret
   140006e4f:	48 85 c9             	test   %rcx,%rcx
   140006e52:	74 18                	je     0x140006e6c
   140006e54:	ba 08 00 00 00       	mov    $0x8,%edx
   140006e59:	41 b8 10 00 00 00    	mov    $0x10,%r8d
   140006e5f:	48 89 f1             	mov    %rsi,%rcx
   140006e62:	48 83 c4 20          	add    $0x20,%rsp
   140006e66:	5e                   	pop    %rsi
   140006e67:	e9 14 37 01 00       	jmp    0x14001a580
   140006e6c:	48 89 f1             	mov    %rsi,%rcx
   140006e6f:	e8 dc 56 03 00       	call   0x14003c550
   140006e74:	48 83 f8 01          	cmp    $0x1,%rax
   140006e78:	74 da                	je     0x140006e54
   140006e7a:	48 89 d1             	mov    %rdx,%rcx
   140006e7d:	0f b6 81 a8 04 00 00 	movzbl 0x4a8(%rcx),%eax
   140006e84:	48 85 c0             	test   %rax,%rax
   140006e87:	75 8f                	jne    0x140006e18
   140006e89:	31 d2                	xor    %edx,%edx
   140006e8b:	e8 60 45 03 00       	call   0x14003b3f0
   140006e90:	48 85 c0             	test   %rax,%rax
   140006e93:	75 b4                	jne    0x140006e49
   140006e95:	48 8b 05 bc 6d 05 00 	mov    0x56dbc(%rip),%rax        # 0x14005dc58
   140006e9c:	48 85 c0             	test   %rax,%rax
   140006e9f:	74 14                	je     0x140006eb5
   140006ea1:	48 83 f8 01          	cmp    $0x1,%rax
   140006ea5:	74 06                	je     0x140006ead
   140006ea7:	ff d0                	call   *%rax
   140006ea9:	84 c0                	test   %al,%al
   140006eab:	75 15                	jne    0x140006ec2
   140006ead:	31 c0                	xor    %eax,%eax
   140006eaf:	48 83 c4 20          	add    $0x20,%rsp
   140006eb3:	5e                   	pop    %rsi
   140006eb4:	c3                   	ret
   140006eb5:	e8 26 50 03 00       	call   0x14003bee0
   140006eba:	48 83 f8 01          	cmp    $0x1,%rax
   140006ebe:	75 e7                	jne    0x140006ea7
   140006ec0:	eb eb                	jmp    0x140006ead
   140006ec2:	b9 08 00 00 00       	mov    $0x8,%ecx
   140006ec7:	ba 10 00 00 00       	mov    $0x10,%edx
   140006ecc:	45 31 c0             	xor    %r8d,%r8d
   140006ecf:	48 83 c4 20          	add    $0x20,%rsp
   140006ed3:	5e                   	pop    %rsi
   140006ed4:	e9 87 57 03 00       	jmp    0x14003c660
