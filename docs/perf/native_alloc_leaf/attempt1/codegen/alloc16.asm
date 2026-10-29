   140006e00:	48 89 c8             	mov    %rcx,%rax
   140006e03:	8b 0d c7 0d 06 00    	mov    0x60dc7(%rip),%ecx        # 0x140067bd0
   140006e09:	65 48 8b 14 25 58 00 	mov    %gs:0x58,%rdx
   140006e10:	00 00 
   140006e12:	48 8b 0c ca          	mov    (%rdx,%rcx,8),%rcx
   140006e16:	48 8b 89 38 00 00 00 	mov    0x38(%rcx),%rcx
   140006e1d:	48 8d 51 ff          	lea    -0x1(%rcx),%rdx
   140006e21:	48 83 fa fe          	cmp    $0xfffffffffffffffe,%rdx
   140006e25:	73 51                	jae    0x140006e78
   140006e27:	0f b6 81 a8 04 00 00 	movzbl 0x4a8(%rcx),%eax
   140006e2e:	48 85 c0             	test   %rax,%rax
   140006e31:	74 32                	je     0x140006e65
   140006e33:	48 ff c8             	dec    %rax
   140006e36:	88 81 a8 04 00 00    	mov    %al,0x4a8(%rcx)
   140006e3c:	83 e0 0f             	and    $0xf,%eax
   140006e3f:	48 8b 84 c1 b0 04 00 	mov    0x4b0(%rcx,%rax,8),%rax
   140006e46:	00 
   140006e47:	89 c1                	mov    %eax,%ecx
   140006e49:	48 89 c2             	mov    %rax,%rdx
   140006e4c:	48 81 e2 00 00 c0 ff 	and    $0xffffffffffc00000,%rdx
   140006e53:	c1 e9 04             	shr    $0x4,%ecx
   140006e56:	81 e1 ff ff 03 00    	and    $0x3ffff,%ecx
   140006e5c:	80 a4 0a 88 15 00 00 	andb   $0x3f,0x1588(%rdx,%rcx,1)
   140006e63:	3f 
   140006e64:	c3                   	ret
   140006e65:	ba 08 00 00 00       	mov    $0x8,%edx
   140006e6a:	41 b8 10 00 00 00    	mov    $0x10,%r8d
   140006e70:	45 31 c9             	xor    %r9d,%r9d
   140006e73:	e9 88 5e 03 00       	jmp    0x14003cd00
   140006e78:	ba 08 00 00 00       	mov    $0x8,%edx
   140006e7d:	41 b8 10 00 00 00    	mov    $0x10,%r8d
   140006e83:	48 89 c1             	mov    %rax,%rcx
   140006e86:	e9 35 5c 03 00       	jmp    0x14003cac0
