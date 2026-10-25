   140006570:	41 56                	push   %r14
   140006572:	56                   	push   %rsi
   140006573:	57                   	push   %rdi
   140006574:	53                   	push   %rbx
   140006575:	48 83 ec 38          	sub    $0x38,%rsp
   140006579:	48 89 d1             	mov    %rdx,%rcx
   14000657c:	8b 05 4e 16 06 00    	mov    0x6164e(%rip),%eax        # 0x140067bd0
   140006582:	65 48 8b 14 25 58 00 	mov    %gs:0x58,%rdx
   140006589:	00 00 
   14000658b:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   14000658f:	48 8b b0 38 00 00 00 	mov    0x38(%rax),%rsi
   140006596:	48 8d 46 ff          	lea    -0x1(%rsi),%rax
   14000659a:	48 83 f8 fd          	cmp    $0xfffffffffffffffd,%rax
   14000659e:	0f 87 ee 01 00 00    	ja     0x140006792
   1400065a4:	48 89 c8             	mov    %rcx,%rax
   1400065a7:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   1400065ad:	48 39 86 30 01 00 00 	cmp    %rax,0x130(%rsi)
   1400065b4:	75 15                	jne    0x1400065cb
   1400065b6:	48 8b be 38 01 00 00 	mov    0x138(%rsi),%rdi
   1400065bd:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   1400065c1:	83 fa 02             	cmp    $0x2,%edx
   1400065c4:	74 41                	je     0x140006607
   1400065c6:	e9 3b 01 00 00       	jmp    0x140006706
   1400065cb:	49 89 c0             	mov    %rax,%r8
   1400065ce:	49 c1 e8 16          	shr    $0x16,%r8
   1400065d2:	44 89 c2             	mov    %r8d,%edx
   1400065d5:	83 e2 0f             	and    $0xf,%edx
   1400065d8:	c1 e2 04             	shl    $0x4,%edx
   1400065db:	48 39 44 16 30       	cmp    %rax,0x30(%rsi,%rdx,1)
   1400065e0:	75 74                	jne    0x140006656
   1400065e2:	48 8b 7c 16 38       	mov    0x38(%rsi,%rdx,1),%rdi
   1400065e7:	48 39 c7             	cmp    %rax,%rdi
   1400065ea:	75 0e                	jne    0x1400065fa
   1400065ec:	48 89 86 30 01 00 00 	mov    %rax,0x130(%rsi)
   1400065f3:	48 89 be 38 01 00 00 	mov    %rdi,0x138(%rsi)
   1400065fa:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   1400065fe:	83 fa 02             	cmp    $0x2,%edx
   140006601:	0f 85 ff 00 00 00    	jne    0x140006706
   140006607:	48 89 ca             	mov    %rcx,%rdx
   14000660a:	48 29 fa             	sub    %rdi,%rdx
   14000660d:	48 3b 57 38          	cmp    0x38(%rdi),%rdx
   140006611:	0f 83 7b 01 00 00    	jae    0x140006792
   140006617:	48 8b 47 30          	mov    0x30(%rdi),%rax
   14000661b:	48 01 f8             	add    %rdi,%rax
   14000661e:	4c 8b 47 08          	mov    0x8(%rdi),%r8
   140006622:	41 0f 92 c0          	setb   %r8b
   140006626:	48 39 c8             	cmp    %rcx,%rax
   140006629:	0f 95 c0             	setne  %al
   14000662c:	44 08 c0             	or     %r8b,%al
   14000662f:	0f 85 d5 01 00 00    	jne    0x14000680a
   140006635:	48 01 fa             	add    %rdi,%rdx
   140006638:	48 89 7c 24 20       	mov    %rdi,0x20(%rsp)
   14000663d:	41 b8 08 00 00 00    	mov    $0x8,%r8d
   140006643:	41 b9 10 00 00 00    	mov    $0x10,%r9d
   140006649:	48 89 f1             	mov    %rsi,%rcx
   14000664c:	e8 9f 65 03 00       	call   0x14003cbf0
   140006651:	e9 b4 01 00 00       	jmp    0x14000680a
   140006656:	48 01 f2             	add    %rsi,%rdx
   140006659:	48 83 c2 30          	add    $0x30,%rdx
   14000665d:	49 89 c1             	mov    %rax,%r9
   140006660:	49 c1 e9 23          	shr    $0x23,%r9
   140006664:	49 89 c2             	mov    %rax,%r10
   140006667:	49 c1 ea 30          	shr    $0x30,%r10
   14000666b:	45 31 d1             	xor    %r10d,%r9d
   14000666e:	45 31 c8             	xor    %r9d,%r8d
   140006671:	41 81 e0 ff 1f 00 00 	and    $0x1fff,%r8d
   140006678:	4c 8b 8e 40 01 00 00 	mov    0x140(%rsi),%r9
   14000667f:	4d 89 c2             	mov    %r8,%r10
   140006682:	66 66 66 66 66 2e 0f 	data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   140006689:	1f 84 00 00 00 00 00 
   140006690:	4b 8b 3c d1          	mov    (%r9,%r10,8),%rdi
   140006694:	48 85 ff             	test   %rdi,%rdi
   140006697:	0f 84 f5 00 00 00    	je     0x140006792
   14000669d:	49 89 fb             	mov    %rdi,%r11
   1400066a0:	49 81 e3 00 00 c0 ff 	and    $0xffffffffffc00000,%r11
   1400066a7:	49 39 c3             	cmp    %rax,%r11
   1400066aa:	74 14                	je     0x1400066c0
   1400066ac:	41 ff c2             	inc    %r10d
   1400066af:	41 81 e2 ff 1f 00 00 	and    $0x1fff,%r10d
   1400066b6:	4d 39 c2             	cmp    %r8,%r10
   1400066b9:	75 d5                	jne    0x140006690
   1400066bb:	e9 d2 00 00 00       	jmp    0x140006792
   1400066c0:	49 89 f8             	mov    %rdi,%r8
   1400066c3:	49 81 e0 ff ff 3f 00 	and    $0x3fffff,%r8
   1400066ca:	74 26                	je     0x1400066f2
   1400066cc:	4d 8d 48 ff          	lea    -0x1(%r8),%r9
   1400066d0:	44 8b 96 58 01 00 00 	mov    0x158(%rsi),%r10d
   1400066d7:	4d 39 d1             	cmp    %r10,%r9
   1400066da:	0f 83 b2 00 00 00    	jae    0x140006792
   1400066e0:	4c 8b 4e 20          	mov    0x20(%rsi),%r9
   1400066e4:	4b 8b 7c c1 f8       	mov    -0x8(%r9,%r8,8),%rdi
   1400066e9:	48 85 ff             	test   %rdi,%rdi
   1400066ec:	0f 84 a0 00 00 00    	je     0x140006792
   1400066f2:	48 89 02             	mov    %rax,(%rdx)
   1400066f5:	48 89 7a 08          	mov    %rdi,0x8(%rdx)
   1400066f9:	0f b6 57 1c          	movzbl 0x1c(%rdi),%edx
   1400066fd:	83 fa 02             	cmp    $0x2,%edx
   140006700:	0f 84 01 ff ff ff    	je     0x140006607
   140006706:	0f b6 86 a8 04 00 00 	movzbl 0x4a8(%rsi),%eax
   14000670d:	85 d2                	test   %edx,%edx
   14000670f:	74 1f                	je     0x140006730
   140006711:	83 fa 01             	cmp    $0x1,%edx
   140006714:	0f 85 f0 00 00 00    	jne    0x14000680a
   14000671a:	ba 00 20 04 00       	mov    $0x42000,%edx
   14000671f:	89 cb                	mov    %ecx,%ebx
   140006721:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   140006727:	39 d3                	cmp    %edx,%ebx
   140006729:	73 1a                	jae    0x140006745
   14000672b:	e9 da 00 00 00       	jmp    0x14000680a
   140006730:	ba 00 f0 05 00       	mov    $0x5f000,%edx
   140006735:	89 cb                	mov    %ecx,%ebx
   140006737:	81 e3 ff ff 3f 00    	and    $0x3fffff,%ebx
   14000673d:	39 d3                	cmp    %edx,%ebx
   14000673f:	0f 82 c5 00 00 00    	jb     0x14000680a
   140006745:	f6 c1 0f             	test   $0xf,%cl
   140006748:	0f 85 bc 00 00 00    	jne    0x14000680a
   14000674e:	41 89 de             	mov    %ebx,%r14d
   140006751:	41 c1 ee 04          	shr    $0x4,%r14d
   140006755:	42 80 bc 37 88 15 00 	cmpb   $0x1,0x1588(%rdi,%r14,1)
   14000675c:	00 01 
   14000675e:	0f 85 a6 00 00 00    	jne    0x14000680a
   140006764:	48 3b 1f             	cmp    (%rdi),%rbx
   140006767:	0f 83 9d 00 00 00    	jae    0x14000680a
   14000676d:	48 01 fb             	add    %rdi,%rbx
   140006770:	3c 0f                	cmp    $0xf,%al
   140006772:	77 30                	ja     0x1400067a4
   140006774:	0f b6 c8             	movzbl %al,%ecx
   140006777:	42 c6 84 37 88 15 00 	movb   $0x41,0x1588(%rdi,%r14,1)
   14000677e:	00 41 
   140006780:	48 89 9c ce b0 04 00 	mov    %rbx,0x4b0(%rsi,%rcx,8)
   140006787:	00 
   140006788:	fe c0                	inc    %al
   14000678a:	88 86 a8 04 00 00    	mov    %al,0x4a8(%rsi)
   140006790:	eb 78                	jmp    0x14000680a
   140006792:	ba 08 00 00 00       	mov    $0x8,%edx
   140006797:	41 b8 10 00 00 00    	mov    $0x10,%r8d
   14000679d:	e8 6e 3e 03 00       	call   0x14003a610
   1400067a2:	eb 66                	jmp    0x14000680a
   1400067a4:	4c 8d 86 b0 04 00 00 	lea    0x4b0(%rsi),%r8
   1400067ab:	41 b9 08 00 00 00    	mov    $0x8,%r9d
   1400067b1:	48 89 f1             	mov    %rsi,%rcx
   1400067b4:	31 d2                	xor    %edx,%edx
   1400067b6:	e8 a5 2e 00 00       	call   0x140009660
   1400067bb:	0f 10 86 f0 04 00 00 	movups 0x4f0(%rsi),%xmm0
   1400067c2:	0f 10 8e 00 05 00 00 	movups 0x500(%rsi),%xmm1
   1400067c9:	0f 10 96 10 05 00 00 	movups 0x510(%rsi),%xmm2
   1400067d0:	0f 10 9e 20 05 00 00 	movups 0x520(%rsi),%xmm3
   1400067d7:	0f 11 9e e0 04 00 00 	movups %xmm3,0x4e0(%rsi)
   1400067de:	0f 11 96 d0 04 00 00 	movups %xmm2,0x4d0(%rsi)
   1400067e5:	0f 11 8e c0 04 00 00 	movups %xmm1,0x4c0(%rsi)
   1400067ec:	0f 11 86 b0 04 00 00 	movups %xmm0,0x4b0(%rsi)
   1400067f3:	42 c6 84 37 88 15 00 	movb   $0x41,0x1588(%rdi,%r14,1)
   1400067fa:	00 41 
   1400067fc:	48 89 9e f0 04 00 00 	mov    %rbx,0x4f0(%rsi)
   140006803:	c6 86 a8 04 00 00 09 	movb   $0x9,0x4a8(%rsi)
   14000680a:	48 c7 44 24 30 10 00 	movq   $0x10,0x30(%rsp)
   140006811:	00 00 
   140006813:	48 8d 44 24 30       	lea    0x30(%rsp),%rax
   140006818:	48 83 c4 38          	add    $0x38,%rsp
   14000681c:	5b                   	pop    %rbx
   14000681d:	5f                   	pop    %rdi
   14000681e:	5e                   	pop    %rsi
   14000681f:	41 5e                	pop    %r14
   140006821:	c3                   	ret
