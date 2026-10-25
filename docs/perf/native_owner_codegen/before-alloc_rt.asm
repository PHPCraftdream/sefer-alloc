   140006340:	56                   	push   %rsi
   140006341:	57                   	push   %rdi
   140006342:	53                   	push   %rbx
   140006343:	48 83 ec 30          	sub    $0x30,%rsp
   140006347:	48 89 ce             	mov    %rcx,%rsi
   14000634a:	8b 05 80 18 06 00    	mov    0x61880(%rip),%eax        # 0x140067bd0
   140006350:	65 48 8b 0c 25 58 00 	mov    %gs:0x58,%rcx
   140006357:	00 00 
   140006359:	48 8b 04 c1          	mov    (%rcx,%rax,8),%rax
   14000635d:	48 8b 88 38 00 00 00 	mov    0x38(%rax),%rcx
   140006364:	48 8d 41 ff          	lea    -0x1(%rcx),%rax
   140006368:	48 83 f8 fe          	cmp    $0xfffffffffffffffe,%rax
   14000636c:	0f 83 9b 00 00 00    	jae    0x14000640d
   140006372:	49 83 f8 11          	cmp    $0x11,%r8
   140006376:	b8 10 00 00 00       	mov    $0x10,%eax
   14000637b:	49 0f 43 c0          	cmovae %r8,%rax
   14000637f:	48 83 fa 11          	cmp    $0x11,%rdx
   140006383:	0f 83 9c 00 00 00    	jae    0x140006425
   140006389:	48 ff c8             	dec    %rax
   14000638c:	48 3d bf f2 03 00    	cmp    $0x3f2bf,%rax
   140006392:	0f 87 fa 00 00 00    	ja     0x140006492
   140006398:	48 c1 e8 04          	shr    $0x4,%rax
   14000639c:	4c 8d 0d b5 ac 03 00 	lea    0x3acb5(%rip),%r9        # 0x140041058
   1400063a3:	42 0f b6 04 08       	movzbl (%rax,%r9,1),%eax
   1400063a8:	48 83 f8 31          	cmp    $0x31,%rax
   1400063ac:	0f 83 7c 01 00 00    	jae    0x14000652e
   1400063b2:	41 89 c1             	mov    %eax,%r9d
   1400063b5:	41 c1 e1 07          	shl    $0x7,%r9d
   1400063b9:	4d 8d 14 c1          	lea    (%r9,%rax,8),%r10
   1400063bd:	46 0f b6 8c 11 a8 04 	movzbl 0x4a8(%rcx,%r10,1),%r9d
   1400063c4:	00 00 
   1400063c6:	4d 85 c9             	test   %r9,%r9
   1400063c9:	0f 84 24 01 00 00    	je     0x1400064f3
   1400063cf:	4a 8d 04 11          	lea    (%rcx,%r10,1),%rax
   1400063d3:	48 05 a8 04 00 00    	add    $0x4a8,%rax
   1400063d9:	49 ff c9             	dec    %r9
   1400063dc:	44 88 08             	mov    %r9b,(%rax)
   1400063df:	41 83 e1 0f          	and    $0xf,%r9d
   1400063e3:	4a 8b 44 c8 08       	mov    0x8(%rax,%r9,8),%rax
   1400063e8:	48 89 c1             	mov    %rax,%rcx
   1400063eb:	48 81 e1 00 00 c0 ff 	and    $0xffffffffffc00000,%rcx
   1400063f2:	89 c2                	mov    %eax,%edx
   1400063f4:	c1 ea 04             	shr    $0x4,%edx
   1400063f7:	81 e2 ff ff 03 00    	and    $0x3ffff,%edx
   1400063fd:	80 a4 11 88 15 00 00 	andb   $0x3f,0x1588(%rcx,%rdx,1)
   140006404:	3f 
   140006405:	48 83 c4 30          	add    $0x30,%rsp
   140006409:	5b                   	pop    %rbx
   14000640a:	5f                   	pop    %rdi
   14000640b:	5e                   	pop    %rsi
   14000640c:	c3                   	ret
   14000640d:	48 85 c9             	test   %rcx,%rcx
   140006410:	0f 84 b7 00 00 00    	je     0x1400064cd
   140006416:	48 89 f1             	mov    %rsi,%rcx
   140006419:	48 83 c4 30          	add    $0x30,%rsp
   14000641d:	5b                   	pop    %rbx
   14000641e:	5f                   	pop    %rdi
   14000641f:	5e                   	pop    %rsi
   140006420:	e9 db 46 01 00       	jmp    0x14001ab00
   140006425:	48 39 d0             	cmp    %rdx,%rax
   140006428:	48 0f 46 c2          	cmovbe %rdx,%rax
   14000642c:	48 ff c8             	dec    %rax
   14000642f:	48 3d bf f2 03 00    	cmp    $0x3f2bf,%rax
   140006435:	77 5b                	ja     0x140006492
   140006437:	48 c1 e8 04          	shr    $0x4,%rax
   14000643b:	4c 8d 0d 6e ff 03 00 	lea    0x3ff6e(%rip),%r9        # 0x1400463b0
   140006442:	46 0f b6 9c 08 94 01 	movzbl 0x194(%rax,%r9,1),%r11d
   140006449:	00 00 
   14000644b:	41 80 fb 30          	cmp    $0x30,%r11b
   14000644f:	77 41                	ja     0x140006492
   140006451:	4c 8d 52 ff          	lea    -0x1(%rdx),%r10
   140006455:	66 66 2e 0f 1f 84 00 	data16 cs nopw 0x0(%rax,%rax,1)
   14000645c:	00 00 00 00 
   140006460:	41 0f b6 c3          	movzbl %r11b,%eax
   140006464:	4d 8b 1c c1          	mov    (%r9,%rax,8),%r11
   140006468:	4d 85 d3             	test   %r10,%r11
   14000646b:	0f 84 41 ff ff ff    	je     0x1400063b2
   140006471:	4d 09 d3             	or     %r10,%r11
   140006474:	49 81 fb bf f2 03 00 	cmp    $0x3f2bf,%r11
   14000647b:	77 15                	ja     0x140006492
   14000647d:	49 c1 eb 04          	shr    $0x4,%r11
   140006481:	47 0f b6 9c 0b 94 01 	movzbl 0x194(%r11,%r9,1),%r11d
   140006488:	00 00 
   14000648a:	41 80 fb 31          	cmp    $0x31,%r11b
   14000648e:	72 d0                	jb     0x140006460
   140006490:	eb 00                	jmp    0x140006492
   140006492:	48 89 44 24 20       	mov    %rax,0x20(%rsp)
   140006497:	48 89 d6             	mov    %rdx,%rsi
   14000649a:	4c 89 c7             	mov    %r8,%rdi
   14000649d:	45 31 c9             	xor    %r9d,%r9d
   1400064a0:	e8 2b 55 03 00       	call   0x14003b9d0
   1400064a5:	48 85 c0             	test   %rax,%rax
   1400064a8:	0f 85 57 ff ff ff    	jne    0x140006405
   1400064ae:	48 8b 05 a3 77 05 00 	mov    0x577a3(%rip),%rax        # 0x14005dc58
   1400064b5:	48 85 c0             	test   %rax,%rax
   1400064b8:	74 52                	je     0x14000650c
   1400064ba:	48 83 f8 01          	cmp    $0x1,%rax
   1400064be:	74 06                	je     0x1400064c6
   1400064c0:	ff d0                	call   *%rax
   1400064c2:	84 c0                	test   %al,%al
   1400064c4:	75 53                	jne    0x140006519
   1400064c6:	31 c0                	xor    %eax,%eax
   1400064c8:	e9 38 ff ff ff       	jmp    0x140006405
   1400064cd:	48 89 f1             	mov    %rsi,%rcx
   1400064d0:	4c 89 c3             	mov    %r8,%rbx
   1400064d3:	48 89 d7             	mov    %rdx,%rdi
   1400064d6:	e8 85 65 03 00       	call   0x14003ca60
   1400064db:	49 89 d8             	mov    %rbx,%r8
   1400064de:	48 89 d1             	mov    %rdx,%rcx
   1400064e1:	48 89 fa             	mov    %rdi,%rdx
   1400064e4:	48 83 f8 01          	cmp    $0x1,%rax
   1400064e8:	0f 85 84 fe ff ff    	jne    0x140006372
   1400064ee:	e9 23 ff ff ff       	jmp    0x140006416
   1400064f3:	48 89 d6             	mov    %rdx,%rsi
   1400064f6:	4c 89 c7             	mov    %r8,%rdi
   1400064f9:	48 89 c2             	mov    %rax,%rdx
   1400064fc:	e8 ff 53 03 00       	call   0x14003b900
   140006501:	48 85 c0             	test   %rax,%rax
   140006504:	0f 85 fb fe ff ff    	jne    0x140006405
   14000650a:	eb a2                	jmp    0x1400064ae
   14000650c:	e8 df 5e 03 00       	call   0x14003c3f0
   140006511:	48 83 f8 01          	cmp    $0x1,%rax
   140006515:	74 af                	je     0x1400064c6
   140006517:	eb a7                	jmp    0x1400064c0
   140006519:	48 89 f1             	mov    %rsi,%rcx
   14000651c:	48 89 fa             	mov    %rdi,%rdx
   14000651f:	45 31 c0             	xor    %r8d,%r8d
   140006522:	48 83 c4 30          	add    $0x30,%rsp
   140006526:	5b                   	pop    %rbx
   140006527:	5f                   	pop    %rdi
   140006528:	5e                   	pop    %rsi
   140006529:	e9 42 66 03 00       	jmp    0x14003cb70
   14000652e:	4c 8d 05 ab 9d 03 00 	lea    0x39dab(%rip),%r8        # 0x1400402e0
   140006535:	ba 31 00 00 00       	mov    $0x31,%edx
   14000653a:	48 89 c1             	mov    %rax,%rcx
   14000653d:	e8 11 39 03 00       	call   0x140039e53
   140006542:	0f 0b                	ud2
