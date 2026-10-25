   140006850:	41 56                	push   %r14
   140006852:	56                   	push   %rsi
   140006853:	57                   	push   %rdi
   140006854:	53                   	push   %rbx
   140006855:	48 83 ec 38          	sub    $0x38,%rsp
   140006859:	48 89 d1             	mov    %rdx,%rcx
   14000685c:	8b 05 6e 13 06 00    	mov    0x6136e(%rip),%eax        # 0x140067bd0
   140006862:	65 48 8b 14 25 58 00 	mov    %gs:0x58,%rdx
   140006869:	00 00 
   14000686b:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   14000686f:	48 8b b0 38 00 00 00 	mov    0x38(%rax),%rsi
   140006876:	48 8d 46 ff          	lea    -0x1(%rsi),%rax
   14000687a:	48 83 f8 fd          	cmp    $0xfffffffffffffffd,%rax
   14000687e:	0f 87 ee 01 00 00    	ja     0x140006a72
   140006884:	48 89 c8             	mov    %rcx,%rax
   140006887:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   14000688d:	48 39 86 30 01 00 00 	cmp    %rax,0x130(%rsi)
   140006894:	75 15                	jne    0x1400068ab
   140006896:	48 8b be 38 01 00 00 	mov    0x138(%rsi),%rdi
   14000689d:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   1400068a1:	83 fa 02             	cmp    $0x2,%edx
   1400068a4:	74 41                	je     0x1400068e7
   1400068a6:	e9 3b 01 00 00       	jmp    0x1400069e6
   1400068ab:	49 89 c0             	mov    %rax,%r8
   1400068ae:	49 c1 e8 16          	shr    $0x16,%r8
   1400068b2:	44 89 c2             	mov    %r8d,%edx
   1400068b5:	83 e2 0f             	and    $0xf,%edx
   1400068b8:	c1 e2 04             	shl    $0x4,%edx
   1400068bb:	48 39 44 16 30       	cmp    %rax,0x30(%rsi,%rdx,1)
   1400068c0:	75 74                	jne    0x140006936
   1400068c2:	48 8b 7c 16 38       	mov    0x38(%rsi,%rdx,1),%rdi
   1400068c7:	48 39 c7             	cmp    %rax,%rdi
   1400068ca:	75 0e                	jne    0x1400068da
   1400068cc:	48 89 86 30 01 00 00 	mov    %rax,0x130(%rsi)
   1400068d3:	48 89 be 38 01 00 00 	mov    %rdi,0x138(%rsi)
   1400068da:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   1400068de:	83 fa 02             	cmp    $0x2,%edx
   1400068e1:	0f 85 ff 00 00 00    	jne    0x1400069e6
   1400068e7:	48 89 ca             	mov    %rcx,%rdx
   1400068ea:	48 29 fa             	sub    %rdi,%rdx
   1400068ed:	48 3b 57 38          	cmp    0x38(%rdi),%rdx
   1400068f1:	0f 83 7b 01 00 00    	jae    0x140006a72
   1400068f7:	48 8b 47 30          	mov    0x30(%rdi),%rax
   1400068fb:	48 01 f8             	add    %rdi,%rax
   1400068fe:	4c 8b 47 08          	mov    0x8(%rdi),%r8
   140006902:	41 0f 92 c0          	setb   %r8b
   140006906:	48 39 c8             	cmp    %rcx,%rax
   140006909:	0f 95 c0             	setne  %al
   14000690c:	44 08 c0             	or     %r8b,%al
   14000690f:	0f 85 d8 01 00 00    	jne    0x140006aed
   140006915:	48 01 fa             	add    %rdi,%rdx
   140006918:	48 89 7c 24 20       	mov    %rdi,0x20(%rsp)
   14000691d:	41 b8 08 00 00 00    	mov    $0x8,%r8d
   140006923:	41 b9 40 00 00 00    	mov    $0x40,%r9d
   140006929:	48 89 f1             	mov    %rsi,%rcx
   14000692c:	e8 bf 62 03 00       	call   0x14003cbf0
   140006931:	e9 b7 01 00 00       	jmp    0x140006aed
   140006936:	48 01 f2             	add    %rsi,%rdx
   140006939:	48 83 c2 30          	add    $0x30,%rdx
   14000693d:	49 89 c1             	mov    %rax,%r9
   140006940:	49 c1 e9 23          	shr    $0x23,%r9
   140006944:	49 89 c2             	mov    %rax,%r10
   140006947:	49 c1 ea 30          	shr    $0x30,%r10
   14000694b:	45 31 d1             	xor    %r10d,%r9d
   14000694e:	45 31 c8             	xor    %r9d,%r8d
   140006951:	41 81 e0 ff 1f 00 00 	and    $0x1fff,%r8d
   140006958:	4c 8b 8e 40 01 00 00 	mov    0x140(%rsi),%r9
   14000695f:	4d 89 c2             	mov    %r8,%r10
   140006962:	66 66 66 66 66 2e 0f 	data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   140006969:	1f 84 00 00 00 00 00 
   140006970:	4b 8b 3c d1          	mov    (%r9,%r10,8),%rdi
   140006974:	48 85 ff             	test   %rdi,%rdi
   140006977:	0f 84 f5 00 00 00    	je     0x140006a72
   14000697d:	49 89 fb             	mov    %rdi,%r11
   140006980:	49 81 e3 00 00 c0 ff 	and    $0xffffffffffc00000,%r11
   140006987:	49 39 c3             	cmp    %rax,%r11
   14000698a:	74 14                	je     0x1400069a0
   14000698c:	41 ff c2             	inc    %r10d
   14000698f:	41 81 e2 ff 1f 00 00 	and    $0x1fff,%r10d
   140006996:	4d 39 c2             	cmp    %r8,%r10
   140006999:	75 d5                	jne    0x140006970
   14000699b:	e9 d2 00 00 00       	jmp    0x140006a72
   1400069a0:	49 89 f8             	mov    %rdi,%r8
   1400069a3:	49 81 e0 ff ff 3f 00 	and    $0x3fffff,%r8
   1400069aa:	74 26                	je     0x1400069d2
   1400069ac:	4d 8d 48 ff          	lea    -0x1(%r8),%r9
   1400069b0:	44 8b 96 58 01 00 00 	mov    0x158(%rsi),%r10d
   1400069b7:	4d 39 d1             	cmp    %r10,%r9
   1400069ba:	0f 83 b2 00 00 00    	jae    0x140006a72
   1400069c0:	4c 8b 4e 20          	mov    0x20(%rsi),%r9
   1400069c4:	4b 8b 7c c1 f8       	mov    -0x8(%r9,%r8,8),%rdi
   1400069c9:	48 85 ff             	test   %rdi,%rdi
   1400069cc:	0f 84 a0 00 00 00    	je     0x140006a72
   1400069d2:	48 89 02             	mov    %rax,(%rdx)
   1400069d5:	48 89 7a 08          	mov    %rdi,0x8(%rdx)
   1400069d9:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   1400069dd:	83 fa 02             	cmp    $0x2,%edx
   1400069e0:	0f 84 01 ff ff ff    	je     0x1400068e7
   1400069e6:	0f b6 86 40 06 00 00 	movzbl 0x640(%rsi),%eax
   1400069ed:	85 d2                	test   %edx,%edx
   1400069ef:	74 1f                	je     0x140006a10
   1400069f1:	83 fa 01             	cmp    $0x1,%edx
   1400069f4:	0f 85 f3 00 00 00    	jne    0x140006aed
   1400069fa:	ba 00 20 04 00       	mov    $0x42000,%edx
   1400069ff:	89 cb                	mov    %ecx,%ebx
   140006a01:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   140006a07:	39 d3                	cmp    %edx,%ebx
   140006a09:	73 1a                	jae    0x140006a25
   140006a0b:	e9 dd 00 00 00       	jmp    0x140006aed
   140006a10:	ba 00 f0 05 00       	mov    $0x5f000,%edx
   140006a15:	89 cb                	mov    %ecx,%ebx
   140006a17:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   140006a1d:	39 d3                	cmp    %edx,%ebx
   140006a1f:	0f 82 c8 00 00 00    	jb     0x140006aed
   140006a25:	f6 c1 0f             	test   $0xf,%cl
   140006a28:	0f 85 bf 00 00 00    	jne    0x140006aed
   140006a2e:	41 89 de             	mov    %ebx,%r14d
   140006a31:	41 c1 ee 04          	shr    $0x4,%r14d
   140006a35:	42 80 bc 37 88 15 00 	cmpb   $0x4,0x1588(%rdi,%r14,1)
   140006a3c:	00 04 
   140006a3e:	0f 85 a9 00 00 00    	jne    0x140006aed
   140006a44:	48 3b 1f             	cmp    (%rdi),%rbx
   140006a47:	0f 83 a0 00 00 00    	jae    0x140006aed
   140006a4d:	48 01 fb             	add    %rdi,%rbx
   140006a50:	3c 0f                	cmp    $0xf,%al
   140006a52:	77 30                	ja     0x140006a84
   140006a54:	0f b6 c8             	movzbl %al,%ecx
   140006a57:	42 c6 84 37 88 15 00 	movb   $0x44,0x1588(%rdi,%r14,1)
   140006a5e:	00 44 
   140006a60:	48 89 9c ce 48 06 00 	mov    %rbx,0x648(%rsi,%rcx,8)
   140006a67:	00 
   140006a68:	fe c0                	inc    %al
   140006a6a:	88 86 40 06 00 00    	mov    %al,0x640(%rsi)
   140006a70:	eb 7b                	jmp    0x140006aed
   140006a72:	ba 08 00 00 00       	mov    $0x8,%edx
   140006a77:	41 b8 40 00 00 00    	mov    $0x40,%r8d
   140006a7d:	e8 8e 3b 03 00       	call   0x14003a610
   140006a82:	eb 69                	jmp    0x140006aed
   140006a84:	4c 8d 86 48 06 00 00 	lea    0x648(%rsi),%r8
   140006a8b:	ba 03 00 00 00       	mov    $0x3,%edx
   140006a90:	41 b9 08 00 00 00    	mov    $0x8,%r9d
   140006a96:	48 89 f1             	mov    %rsi,%rcx
   140006a99:	e8 c2 2b 00 00       	call   0x140009660
   140006a9e:	0f 10 86 88 06 00 00 	movups 0x688(%rsi),%xmm0
   140006aa5:	0f 10 8e 98 06 00 00 	movups 0x698(%rsi),%xmm1
   140006aac:	0f 10 96 a8 06 00 00 	movups 0x6a8(%rsi),%xmm2
   140006ab3:	0f 10 9e b8 06 00 00 	movups 0x6b8(%rsi),%xmm3
   140006aba:	0f 11 9e 78 06 00 00 	movups %xmm3,0x678(%rsi)
   140006ac1:	0f 11 96 68 06 00 00 	movups %xmm2,0x668(%rsi)
   140006ac8:	0f 11 8e 58 06 00 00 	movups %xmm1,0x658(%rsi)
   140006acf:	0f 11 86 48 06 00 00 	movups %xmm0,0x648(%rsi)
   140006ad6:	42 c6 84 37 88 15 00 	movb   $0x44,0x1588(%rdi,%r14,1)
   140006add:	00 44 
   140006adf:	48 89 9e 88 06 00 00 	mov    %rbx,0x688(%rsi)
   140006ae6:	c6 86 40 06 00 00 09 	movb   $0x9,0x640(%rsi)
   140006aed:	48 c7 44 24 30 40 00 	movq   $0x40,0x30(%rsp)
   140006af4:	00 00 
   140006af6:	48 8d 44 24 30       	lea    0x30(%rsp),%rax
   140006afb:	48 83 c4 38          	add    $0x38,%rsp
   140006aff:	5b                   	pop    %rbx
   140006b00:	5f                   	pop    %rdi
   140006b01:	5e                   	pop    %rsi
   140006b02:	41 5e                	pop    %r14
   140006b04:	c3                   	ret
