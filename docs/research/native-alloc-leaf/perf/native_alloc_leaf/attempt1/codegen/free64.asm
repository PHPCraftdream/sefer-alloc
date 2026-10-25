   1400067b0:	41 56                	push   %r14
   1400067b2:	56                   	push   %rsi
   1400067b3:	57                   	push   %rdi
   1400067b4:	53                   	push   %rbx
   1400067b5:	48 83 ec 38          	sub    $0x38,%rsp
   1400067b9:	48 89 d1             	mov    %rdx,%rcx
   1400067bc:	8b 05 0e 14 06 00    	mov    0x6140e(%rip),%eax        # 0x140067bd0
   1400067c2:	65 48 8b 14 25 58 00 	mov    %gs:0x58,%rdx
   1400067c9:	00 00 
   1400067cb:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   1400067cf:	48 8b b0 38 00 00 00 	mov    0x38(%rax),%rsi
   1400067d6:	48 8d 46 ff          	lea    -0x1(%rsi),%rax
   1400067da:	48 83 f8 fd          	cmp    $0xfffffffffffffffd,%rax
   1400067de:	0f 87 ee 01 00 00    	ja     0x1400069d2
   1400067e4:	48 89 c8             	mov    %rcx,%rax
   1400067e7:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   1400067ed:	48 39 86 30 01 00 00 	cmp    %rax,0x130(%rsi)
   1400067f4:	75 15                	jne    0x14000680b
   1400067f6:	48 8b be 38 01 00 00 	mov    0x138(%rsi),%rdi
   1400067fd:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   140006801:	83 fa 02             	cmp    $0x2,%edx
   140006804:	74 41                	je     0x140006847
   140006806:	e9 3b 01 00 00       	jmp    0x140006946
   14000680b:	49 89 c0             	mov    %rax,%r8
   14000680e:	49 c1 e8 16          	shr    $0x16,%r8
   140006812:	44 89 c2             	mov    %r8d,%edx
   140006815:	83 e2 0f             	and    $0xf,%edx
   140006818:	c1 e2 04             	shl    $0x4,%edx
   14000681b:	48 39 44 16 30       	cmp    %rax,0x30(%rsi,%rdx,1)
   140006820:	75 74                	jne    0x140006896
   140006822:	48 8b 7c 16 38       	mov    0x38(%rsi,%rdx,1),%rdi
   140006827:	48 39 c7             	cmp    %rax,%rdi
   14000682a:	75 0e                	jne    0x14000683a
   14000682c:	48 89 86 30 01 00 00 	mov    %rax,0x130(%rsi)
   140006833:	48 89 be 38 01 00 00 	mov    %rdi,0x138(%rsi)
   14000683a:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   14000683e:	83 fa 02             	cmp    $0x2,%edx
   140006841:	0f 85 ff 00 00 00    	jne    0x140006946
   140006847:	48 89 ca             	mov    %rcx,%rdx
   14000684a:	48 29 fa             	sub    %rdi,%rdx
   14000684d:	48 3b 57 38          	cmp    0x38(%rdi),%rdx
   140006851:	0f 83 7b 01 00 00    	jae    0x1400069d2
   140006857:	48 8b 47 30          	mov    0x30(%rdi),%rax
   14000685b:	48 01 f8             	add    %rdi,%rax
   14000685e:	4c 8b 47 08          	mov    0x8(%rdi),%r8
   140006862:	41 0f 92 c0          	setb   %r8b
   140006866:	48 39 c8             	cmp    %rcx,%rax
   140006869:	0f 95 c0             	setne  %al
   14000686c:	44 08 c0             	or     %r8b,%al
   14000686f:	0f 85 d8 01 00 00    	jne    0x140006a4d
   140006875:	48 01 fa             	add    %rdi,%rdx
   140006878:	48 89 7c 24 20       	mov    %rdi,0x20(%rsp)
   14000687d:	41 b8 08 00 00 00    	mov    $0x8,%r8d
   140006883:	41 b9 40 00 00 00    	mov    $0x40,%r9d
   140006889:	48 89 f1             	mov    %rsi,%rcx
   14000688c:	e8 0f 65 03 00       	call   0x14003cda0
   140006891:	e9 b7 01 00 00       	jmp    0x140006a4d
   140006896:	48 01 f2             	add    %rsi,%rdx
   140006899:	48 83 c2 30          	add    $0x30,%rdx
   14000689d:	49 89 c1             	mov    %rax,%r9
   1400068a0:	49 c1 e9 23          	shr    $0x23,%r9
   1400068a4:	49 89 c2             	mov    %rax,%r10
   1400068a7:	49 c1 ea 30          	shr    $0x30,%r10
   1400068ab:	45 31 d1             	xor    %r10d,%r9d
   1400068ae:	45 31 c8             	xor    %r9d,%r8d
   1400068b1:	41 81 e0 ff 1f 00 00 	and    $0x1fff,%r8d
   1400068b8:	4c 8b 8e 40 01 00 00 	mov    0x140(%rsi),%r9
   1400068bf:	4d 89 c2             	mov    %r8,%r10
   1400068c2:	66 66 66 66 66 2e 0f 	data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   1400068c9:	1f 84 00 00 00 00 00 
   1400068d0:	4b 8b 3c d1          	mov    (%r9,%r10,8),%rdi
   1400068d4:	48 85 ff             	test   %rdi,%rdi
   1400068d7:	0f 84 f5 00 00 00    	je     0x1400069d2
   1400068dd:	49 89 fb             	mov    %rdi,%r11
   1400068e0:	49 81 e3 00 00 c0 ff 	and    $0xffffffffffc00000,%r11
   1400068e7:	49 39 c3             	cmp    %rax,%r11
   1400068ea:	74 14                	je     0x140006900
   1400068ec:	41 ff c2             	inc    %r10d
   1400068ef:	41 81 e2 ff 1f 00 00 	and    $0x1fff,%r10d
   1400068f6:	4d 39 c2             	cmp    %r8,%r10
   1400068f9:	75 d5                	jne    0x1400068d0
   1400068fb:	e9 d2 00 00 00       	jmp    0x1400069d2
   140006900:	49 89 f8             	mov    %rdi,%r8
   140006903:	49 81 e0 ff ff 3f 00 	and    $0x3fffff,%r8
   14000690a:	74 26                	je     0x140006932
   14000690c:	4d 8d 48 ff          	lea    -0x1(%r8),%r9
   140006910:	44 8b 96 58 01 00 00 	mov    0x158(%rsi),%r10d
   140006917:	4d 39 d1             	cmp    %r10,%r9
   14000691a:	0f 83 b2 00 00 00    	jae    0x1400069d2
   140006920:	4c 8b 4e 20          	mov    0x20(%rsi),%r9
   140006924:	4b 8b 7c c1 f8       	mov    -0x8(%r9,%r8,8),%rdi
   140006929:	48 85 ff             	test   %rdi,%rdi
   14000692c:	0f 84 a0 00 00 00    	je     0x1400069d2
   140006932:	48 89 02             	mov    %rax,(%rdx)
   140006935:	48 89 7a 08          	mov    %rdi,0x8(%rdx)
   140006939:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   14000693d:	83 fa 02             	cmp    $0x2,%edx
   140006940:	0f 84 01 ff ff ff    	je     0x140006847
   140006946:	0f b6 86 40 06 00 00 	movzbl 0x640(%rsi),%eax
   14000694d:	85 d2                	test   %edx,%edx
   14000694f:	74 1f                	je     0x140006970
   140006951:	83 fa 01             	cmp    $0x1,%edx
   140006954:	0f 85 f3 00 00 00    	jne    0x140006a4d
   14000695a:	ba 00 20 04 00       	mov    $0x42000,%edx
   14000695f:	89 cb                	mov    %ecx,%ebx
   140006961:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   140006967:	39 d3                	cmp    %edx,%ebx
   140006969:	73 1a                	jae    0x140006985
   14000696b:	e9 dd 00 00 00       	jmp    0x140006a4d
   140006970:	ba 00 f0 05 00       	mov    $0x5f000,%edx
   140006975:	89 cb                	mov    %ecx,%ebx
   140006977:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   14000697d:	39 d3                	cmp    %edx,%ebx
   14000697f:	0f 82 c8 00 00 00    	jb     0x140006a4d
   140006985:	f6 c1 0f             	test   $0xf,%cl
   140006988:	0f 85 bf 00 00 00    	jne    0x140006a4d
   14000698e:	41 89 de             	mov    %ebx,%r14d
   140006991:	41 c1 ee 04          	shr    $0x4,%r14d
   140006995:	42 80 bc 37 88 15 00 	cmpb   $0x4,0x1588(%rdi,%r14,1)
   14000699c:	00 04 
   14000699e:	0f 85 a9 00 00 00    	jne    0x140006a4d
   1400069a4:	48 3b 1f             	cmp    (%rdi),%rbx
   1400069a7:	0f 83 a0 00 00 00    	jae    0x140006a4d
   1400069ad:	48 01 fb             	add    %rdi,%rbx
   1400069b0:	3c 0f                	cmp    $0xf,%al
   1400069b2:	77 30                	ja     0x1400069e4
   1400069b4:	0f b6 c8             	movzbl %al,%ecx
   1400069b7:	42 c6 84 37 88 15 00 	movb   $0x44,0x1588(%rdi,%r14,1)
   1400069be:	00 44 
   1400069c0:	48 89 9c ce 48 06 00 	mov    %rbx,0x648(%rsi,%rcx,8)
   1400069c7:	00 
   1400069c8:	fe c0                	inc    %al
   1400069ca:	88 86 40 06 00 00    	mov    %al,0x640(%rsi)
   1400069d0:	eb 7b                	jmp    0x140006a4d
   1400069d2:	ba 08 00 00 00       	mov    $0x8,%edx
   1400069d7:	41 b8 40 00 00 00    	mov    $0x40,%r8d
   1400069dd:	e8 ae 3a 03 00       	call   0x14003a490
   1400069e2:	eb 69                	jmp    0x140006a4d
   1400069e4:	4c 8d 86 48 06 00 00 	lea    0x648(%rsi),%r8
   1400069eb:	ba 03 00 00 00       	mov    $0x3,%edx
   1400069f0:	41 b9 08 00 00 00    	mov    $0x8,%r9d
   1400069f6:	48 89 f1             	mov    %rsi,%rcx
   1400069f9:	e8 e2 2a 00 00       	call   0x1400094e0
   1400069fe:	0f 10 86 88 06 00 00 	movups 0x688(%rsi),%xmm0
   140006a05:	0f 10 8e 98 06 00 00 	movups 0x698(%rsi),%xmm1
   140006a0c:	0f 10 96 a8 06 00 00 	movups 0x6a8(%rsi),%xmm2
   140006a13:	0f 10 9e b8 06 00 00 	movups 0x6b8(%rsi),%xmm3
   140006a1a:	0f 11 9e 78 06 00 00 	movups %xmm3,0x678(%rsi)
   140006a21:	0f 11 96 68 06 00 00 	movups %xmm2,0x668(%rsi)
   140006a28:	0f 11 8e 58 06 00 00 	movups %xmm1,0x658(%rsi)
   140006a2f:	0f 11 86 48 06 00 00 	movups %xmm0,0x648(%rsi)
   140006a36:	42 c6 84 37 88 15 00 	movb   $0x44,0x1588(%rdi,%r14,1)
   140006a3d:	00 44 
   140006a3f:	48 89 9e 88 06 00 00 	mov    %rbx,0x688(%rsi)
   140006a46:	c6 86 40 06 00 00 09 	movb   $0x9,0x640(%rsi)
   140006a4d:	48 c7 44 24 30 40 00 	movq   $0x40,0x30(%rsp)
   140006a54:	00 00 
   140006a56:	48 8d 44 24 30       	lea    0x30(%rsp),%rax
   140006a5b:	48 83 c4 38          	add    $0x38,%rsp
   140006a5f:	5b                   	pop    %rbx
   140006a60:	5f                   	pop    %rdi
   140006a61:	5e                   	pop    %rsi
   140006a62:	41 5e                	pop    %r14
   140006a64:	c3                   	ret
