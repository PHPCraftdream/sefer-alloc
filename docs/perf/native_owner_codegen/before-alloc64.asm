   140006fb0:	56                   	push   %rsi
   140006fb1:	48 83 ec 20          	sub    $0x20,%rsp
   140006fb5:	48 89 ce             	mov    %rcx,%rsi
   140006fb8:	8b 05 12 0c 06 00    	mov    0x60c12(%rip),%eax        # 0x140067bd0
   140006fbe:	65 48 8b 0c 25 58 00 	mov    %gs:0x58,%rcx
   140006fc5:	00 00 
   140006fc7:	48 8b 04 c1          	mov    (%rcx,%rax,8),%rax
   140006fcb:	48 8b 88 38 00 00 00 	mov    0x38(%rax),%rcx
   140006fd2:	48 8d 41 ff          	lea    -0x1(%rcx),%rax
   140006fd6:	48 83 f8 fe          	cmp    $0xfffffffffffffffe,%rax
   140006fda:	73 43                	jae    0x14000701f
   140006fdc:	0f b6 81 40 06 00 00 	movzbl 0x640(%rcx),%eax
   140006fe3:	48 85 c0             	test   %rax,%rax
   140006fe6:	74 71                	je     0x140007059
   140006fe8:	48 ff c8             	dec    %rax
   140006feb:	88 81 40 06 00 00    	mov    %al,0x640(%rcx)
   140006ff1:	83 e0 0f             	and    $0xf,%eax
   140006ff4:	48 8b 84 c1 48 06 00 	mov    0x648(%rcx,%rax,8),%rax
   140006ffb:	00 
   140006ffc:	48 89 c1             	mov    %rax,%rcx
   140006fff:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   140007006:	89 c2                	mov    %eax,%edx
   140007008:	c1 ea 04             	shr    $0x4,%edx
   14000700b:	81 e2 ff ff 03 00    	and    $0x3ffff,%edx
   140007011:	80 a4 11 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rdx,1)
   140007018:	3f 
   140007019:	48 83 c4 20          	add    $0x20,%rsp
   14000701d:	5e                   	pop    %rsi
   14000701e:	c3                   	ret
   14000701f:	48 85 c9             	test   %rcx,%rcx
   140007022:	74 18                	je     0x14000703c
   140007024:	ba 08 00 00 00       	mov    $0x8,%edx
   140007029:	41 b8 40 00 00 00    	mov    $0x40,%r8d
   14000702f:	48 89 f1             	mov    %rsi,%rcx
   140007032:	48 83 c4 20          	add    $0x20,%rsp
   140007036:	5e                   	pop    %rsi
   140007037:	e9 c4 3a 01 00       	jmp    0x14001ab00
   14000703c:	48 89 f1             	mov    %rsi,%rcx
   14000703f:	e8 1c 5a 03 00       	call   0x14003ca60
   140007044:	48 83 f8 01          	cmp    $0x1,%rax
   140007048:	74 da                	je     0x140007024
   14000704a:	48 89 d1             	mov    %rdx,%rcx
   14000704d:	0f b6 81 40 06 00 00 	movzbl 0x640(%rcx),%eax
   140007054:	48 85 c0             	test   %rax,%rax
   140007057:	75 8f                	jne    0x140006fe8
   140007059:	ba 03 00 00 00       	mov    $0x3,%edx
   14000705e:	e8 9d 48 03 00       	call   0x14003b900
   140007063:	48 85 c0             	test   %rax,%rax
   140007066:	75 b1                	jne    0x140007019
   140007068:	48 8b 05 e9 6b 05 00 	mov    0x56be9(%rip),%rax        # 0x14005dc58
   14000706f:	48 85 c0             	test   %rax,%rax
   140007072:	74 14                	je     0x140007088
   140007074:	48 83 f8 01          	cmp    $0x1,%rax
   140007078:	74 06                	je     0x140007080
   14000707a:	ff d0                	call   *%rax
   14000707c:	84 c0                	test   %al,%al
   14000707e:	75 15                	jne    0x140007095
   140007080:	31 c0                	xor    %eax,%eax
   140007082:	48 83 c4 20          	add    $0x20,%rsp
   140007086:	5e                   	pop    %rsi
   140007087:	c3                   	ret
   140007088:	e8 63 53 03 00       	call   0x14003c3f0
   14000708d:	48 83 f8 01          	cmp    $0x1,%rax
   140007091:	75 e7                	jne    0x14000707a
   140007093:	eb eb                	jmp    0x140007080
   140007095:	b9 08 00 00 00       	mov    $0x8,%ecx
   14000709a:	ba 40 00 00 00       	mov    $0x40,%edx
   14000709f:	45 31 c0             	xor    %r8d,%r8d
   1400070a2:	48 83 c4 20          	add    $0x20,%rsp
   1400070a6:	5e                   	pop    %rsi
   1400070a7:	e9 c4 5a 03 00       	jmp    0x14003cb70
