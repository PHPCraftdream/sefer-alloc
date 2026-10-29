   1400064d0:	41 56                	push   %r14
   1400064d2:	56                   	push   %rsi
   1400064d3:	57                   	push   %rdi
   1400064d4:	53                   	push   %rbx
   1400064d5:	48 83 ec 38          	sub    $0x38,%rsp
   1400064d9:	48 89 d1             	mov    %rdx,%rcx
   1400064dc:	8b 05 ee 16 06 00    	mov    0x616ee(%rip),%eax        # 0x140067bd0
   1400064e2:	65 48 8b 14 25 58 00 	mov    %gs:0x58,%rdx
   1400064e9:	00 00 
   1400064eb:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   1400064ef:	48 8b b0 38 00 00 00 	mov    0x38(%rax),%rsi
   1400064f6:	48 8d 46 ff          	lea    -0x1(%rsi),%rax
   1400064fa:	48 83 f8 fd          	cmp    $0xfffffffffffffffd,%rax
   1400064fe:	0f 87 ee 01 00 00    	ja     0x1400066f2
   140006504:	48 89 c8             	mov    %rcx,%rax
   140006507:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   14000650d:	48 39 86 30 01 00 00 	cmp    %rax,0x130(%rsi)
   140006514:	75 15                	jne    0x14000652b
   140006516:	48 8b be 38 01 00 00 	mov    0x138(%rsi),%rdi
   14000651d:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   140006521:	83 fa 02             	cmp    $0x2,%edx
   140006524:	74 41                	je     0x140006567
   140006526:	e9 3b 01 00 00       	jmp    0x140006666
   14000652b:	49 89 c0             	mov    %rax,%r8
   14000652e:	49 c1 e8 16          	shr    $0x16,%r8
   140006532:	44 89 c2             	mov    %r8d,%edx
   140006535:	83 e2 0f             	and    $0xf,%edx
   140006538:	c1 e2 04             	shl    $0x4,%edx
   14000653b:	48 39 44 16 30       	cmp    %rax,0x30(%rsi,%rdx,1)
   140006540:	75 74                	jne    0x1400065b6
   140006542:	48 8b 7c 16 38       	mov    0x38(%rsi,%rdx,1),%rdi
   140006547:	48 39 c7             	cmp    %rax,%rdi
   14000654a:	75 0e                	jne    0x14000655a
   14000654c:	48 89 86 30 01 00 00 	mov    %rax,0x130(%rsi)
   140006553:	48 89 be 38 01 00 00 	mov    %rdi,0x138(%rsi)
   14000655a:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   14000655e:	83 fa 02             	cmp    $0x2,%edx
   140006561:	0f 85 ff 00 00 00    	jne    0x140006666
   140006567:	48 89 ca             	mov    %rcx,%rdx
   14000656a:	48 29 fa             	sub    %rdi,%rdx
   14000656d:	48 3b 57 38          	cmp    0x38(%rdi),%rdx
   140006571:	0f 83 7b 01 00 00    	jae    0x1400066f2
   140006577:	48 8b 47 30          	mov    0x30(%rdi),%rax
   14000657b:	48 01 f8             	add    %rdi,%rax
   14000657e:	4c 8b 47 08          	mov    0x8(%rdi),%r8
   140006582:	41 0f 92 c0          	setb   %r8b
   140006586:	48 39 c8             	cmp    %rcx,%rax
   140006589:	0f 95 c0             	setne  %al
   14000658c:	44 08 c0             	or     %r8b,%al
   14000658f:	0f 85 d5 01 00 00    	jne    0x14000676a
   140006595:	48 01 fa             	add    %rdi,%rdx
   140006598:	48 89 7c 24 20       	mov    %rdi,0x20(%rsp)
   14000659d:	41 b8 08 00 00 00    	mov    $0x8,%r8d
   1400065a3:	41 b9 10 00 00 00    	mov    $0x10,%r9d
   1400065a9:	48 89 f1             	mov    %rsi,%rcx
   1400065ac:	e8 ef 67 03 00       	call   0x14003cda0
   1400065b1:	e9 b4 01 00 00       	jmp    0x14000676a
   1400065b6:	48 01 f2             	add    %rsi,%rdx
   1400065b9:	48 83 c2 30          	add    $0x30,%rdx
   1400065bd:	49 89 c1             	mov    %rax,%r9
   1400065c0:	49 c1 e9 23          	shr    $0x23,%r9
   1400065c4:	49 89 c2             	mov    %rax,%r10
   1400065c7:	49 c1 ea 30          	shr    $0x30,%r10
   1400065cb:	45 31 d1             	xor    %r10d,%r9d
   1400065ce:	45 31 c8             	xor    %r9d,%r8d
   1400065d1:	41 81 e0 ff 1f 00 00 	and    $0x1fff,%r8d
   1400065d8:	4c 8b 8e 40 01 00 00 	mov    0x140(%rsi),%r9
   1400065df:	4d 89 c2             	mov    %r8,%r10
   1400065e2:	66 66 66 66 66 2e 0f 	data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   1400065e9:	1f 84 00 00 00 00 00 
   1400065f0:	4b 8b 3c d1          	mov    (%r9,%r10,8),%rdi
   1400065f4:	48 85 ff             	test   %rdi,%rdi
   1400065f7:	0f 84 f5 00 00 00    	je     0x1400066f2
   1400065fd:	49 89 fb             	mov    %rdi,%r11
   140006600:	49 81 e3 00 00 c0 ff 	and    $0xffffffffffc00000,%r11
   140006607:	49 39 c3             	cmp    %rax,%r11
   14000660a:	74 14                	je     0x140006620
   14000660c:	41 ff c2             	inc    %r10d
   14000660f:	41 81 e2 ff 1f 00 00 	and    $0x1fff,%r10d
   140006616:	4d 39 c2             	cmp    %r8,%r10
   140006619:	75 d5                	jne    0x1400065f0
   14000661b:	e9 d2 00 00 00       	jmp    0x1400066f2
   140006620:	49 89 f8             	mov    %rdi,%r8
   140006623:	49 81 e0 ff ff 3f 00 	and    $0x3fffff,%r8
   14000662a:	74 26                	je     0x140006652
   14000662c:	4d 8d 48 ff          	lea    -0x1(%r8),%r9
   140006630:	44 8b 96 58 01 00 00 	mov    0x158(%rsi),%r10d
   140006637:	4d 39 d1             	cmp    %r10,%r9
   14000663a:	0f 83 b2 00 00 00    	jae    0x1400066f2
   140006640:	4c 8b 4e 20          	mov    0x20(%rsi),%r9
   140006644:	4b 8b 7c c1 f8       	mov    -0x8(%r9,%r8,8),%rdi
   140006649:	48 85 ff             	test   %rdi,%rdi
   14000664c:	0f 84 a0 00 00 00    	je     0x1400066f2
   140006652:	48 89 02             	mov    %rax,(%rdx)
   140006655:	48 89 7a 08          	mov    %rdi,0x8(%rdx)
   140006659:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   14000665d:	83 fa 02             	cmp    $0x2,%edx
   140006660:	0f 84 01 ff ff ff    	je     0x140006567
   140006666:	0f b6 86 a8 04 00 00 	movzbl 0x4a8(%rsi),%eax
   14000666d:	85 d2                	test   %edx,%edx
   14000666f:	74 1f                	je     0x140006690
   140006671:	83 fa 01             	cmp    $0x1,%edx
   140006674:	0f 85 f0 00 00 00    	jne    0x14000676a
   14000667a:	ba 00 20 04 00       	mov    $0x42000,%edx
   14000667f:	89 cb                	mov    %ecx,%ebx
   140006681:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   140006687:	39 d3                	cmp    %edx,%ebx
   140006689:	73 1a                	jae    0x1400066a5
   14000668b:	e9 da 00 00 00       	jmp    0x14000676a
   140006690:	ba 00 f0 05 00       	mov    $0x5f000,%edx
   140006695:	89 cb                	mov    %ecx,%ebx
   140006697:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   14000669d:	39 d3                	cmp    %edx,%ebx
   14000669f:	0f 82 c5 00 00 00    	jb     0x14000676a
   1400066a5:	f6 c1 0f             	test   $0xf,%cl
   1400066a8:	0f 85 bc 00 00 00    	jne    0x14000676a
   1400066ae:	41 89 de             	mov    %ebx,%r14d
   1400066b1:	41 c1 ee 04          	shr    $0x4,%r14d
   1400066b5:	42 80 bc 37 88 15 00 	cmpb   $0x1,0x1588(%rdi,%r14,1)
   1400066bc:	00 01 
   1400066be:	0f 85 a6 00 00 00    	jne    0x14000676a
   1400066c4:	48 3b 1f             	cmp    (%rdi),%rbx
   1400066c7:	0f 83 9d 00 00 00    	jae    0x14000676a
   1400066cd:	48 01 fb             	add    %rdi,%rbx
   1400066d0:	3c 0f                	cmp    $0xf,%al
   1400066d2:	77 30                	ja     0x140006704
   1400066d4:	0f b6 c8             	movzbl %al,%ecx
   1400066d7:	42 c6 84 37 88 15 00 	movb   $0x41,0x1588(%rdi,%r14,1)
   1400066de:	00 41 
   1400066e0:	48 89 9c ce b0 04 00 	mov    %rbx,0x4b0(%rsi,%rcx,8)
   1400066e7:	00 
   1400066e8:	fe c0                	inc    %al
   1400066ea:	88 86 a8 04 00 00    	mov    %al,0x4a8(%rsi)
   1400066f0:	eb 78                	jmp    0x14000676a
   1400066f2:	ba 08 00 00 00       	mov    $0x8,%edx
   1400066f7:	41 b8 10 00 00 00    	mov    $0x10,%r8d
   1400066fd:	e8 8e 3d 03 00       	call   0x14003a490
   140006702:	eb 66                	jmp    0x14000676a
   140006704:	4c 8d 86 b0 04 00 00 	lea    0x4b0(%rsi),%r8
   14000670b:	41 b9 08 00 00 00    	mov    $0x8,%r9d
   140006711:	48 89 f1             	mov    %rsi,%rcx
   140006714:	31 d2                	xor    %edx,%edx
   140006716:	e8 c5 2d 00 00       	call   0x1400094e0
   14000671b:	0f 10 86 f0 04 00 00 	movups 0x4f0(%rsi),%xmm0
   140006722:	0f 10 8e 00 05 00 00 	movups 0x500(%rsi),%xmm1
   140006729:	0f 10 96 10 05 00 00 	movups 0x510(%rsi),%xmm2
   140006730:	0f 10 9e 20 05 00 00 	movups 0x520(%rsi),%xmm3
   140006737:	0f 11 9e e0 04 00 00 	movups %xmm3,0x4e0(%rsi)
   14000673e:	0f 11 96 d0 04 00 00 	movups %xmm2,0x4d0(%rsi)
   140006745:	0f 11 8e c0 04 00 00 	movups %xmm1,0x4c0(%rsi)
   14000674c:	0f 11 86 b0 04 00 00 	movups %xmm0,0x4b0(%rsi)
   140006753:	42 c6 84 37 88 15 00 	movb   $0x41,0x1588(%rdi,%r14,1)
   14000675a:	00 41 
   14000675c:	48 89 9e f0 04 00 00 	mov    %rbx,0x4f0(%rsi)
   140006763:	c6 86 a8 04 00 00 09 	movb   $0x9,0x4a8(%rsi)
   14000676a:	48 c7 44 24 30 10 00 	movq   $0x10,0x30(%rsp)
   140006771:	00 00 
   140006773:	48 8d 44 24 30       	lea    0x30(%rsp),%rax
   140006778:	48 83 c4 38          	add    $0x38,%rsp
   14000677c:	5b                   	pop    %rbx
   14000677d:	5f                   	pop    %rdi
   14000677e:	5e                   	pop    %rsi
   14000677f:	41 5e                	pop    %r14
   140006781:	c3                   	ret
