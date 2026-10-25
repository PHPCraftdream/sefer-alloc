   140006ea0:	48 89 c8             	mov    %rcx,%rax
   140006ea3:	8b 0d 27 0d 06 00    	mov    0x60d27(%rip),%ecx        # 0x140067bd0
   140006ea9:	65 48 8b 14 25 58 00 	mov    %gs:0x58,%rdx
   140006eb0:	00 00 
   140006eb2:	48 8b 0c ca          	mov    (%rdx,%rcx,8),%rcx
   140006eb6:	48 8b 89 38 00 00 00 	mov    0x38(%rcx),%rcx
   140006ebd:	48 8d 51 ff          	lea    -0x1(%rcx),%rdx
   140006ec1:	48 83 fa fe          	cmp    $0xfffffffffffffffe,%rdx
   140006ec5:	73 54                	jae    0x140006f1b
   140006ec7:	0f b6 81 40 06 00 00 	movzbl 0x640(%rcx),%eax
   140006ece:	48 85 c0             	test   %rax,%rax
   140006ed1:	74 32                	je     0x140006f05
   140006ed3:	48 ff c8             	dec    %rax
   140006ed6:	88 81 40 06 00 00    	mov    %al,0x640(%rcx)
   140006edc:	83 e0 0f             	and    $0xf,%eax
   140006edf:	48 8b 84 c1 48 06 00 	mov    0x648(%rcx,%rax,8),%rax
   140006ee6:	00 
   140006ee7:	89 c1                	mov    %eax,%ecx
   140006ee9:	48 89 c2             	mov    %rax,%rdx
   140006eec:	48 81 e2 00 00 c0 ff 	and    $0xffffffffffc00000,%rdx
   140006ef3:	c1 e9 04             	shr    $0x4,%ecx
   140006ef6:	81 e1 ff ff 03 00    	and    $0x3ffff,%ecx
   140006efc:	80 a4 0a 88 15 00 00 	andb   $0x3f,0x1588(%rdx,%rcx,1)
   140006f03:	3f 
   140006f04:	c3                   	ret
   140006f05:	ba 08 00 00 00       	mov    $0x8,%edx
   140006f0a:	41 b8 40 00 00 00    	mov    $0x40,%r8d
   140006f10:	41 b9 03 00 00 00    	mov    $0x3,%r9d
   140006f16:	e9 e5 5d 03 00       	jmp    0x14003cd00
   140006f1b:	ba 08 00 00 00       	mov    $0x8,%edx
   140006f20:	41 b8 40 00 00 00    	mov    $0x40,%r8d
   140006f26:	48 89 c1             	mov    %rax,%rcx
   140006f29:	e9 92 5b 03 00       	jmp    0x14003cac0
