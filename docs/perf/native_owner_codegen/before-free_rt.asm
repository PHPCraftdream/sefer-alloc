   1400070d0:	41 57                	push   %r15
   1400070d2:	41 56                	push   %r14
   1400070d4:	41 55                	push   %r13
   1400070d6:	41 54                	push   %r12
   1400070d8:	56                   	push   %rsi
   1400070d9:	57                   	push   %rdi
   1400070da:	55                   	push   %rbp
   1400070db:	53                   	push   %rbx
   1400070dc:	48 83 ec 38          	sub    $0x38,%rsp
   1400070e0:	8b 05 ea 0a 06 00    	mov    0x60aea(%rip),%eax        # 0x140067bd0
   1400070e6:	65 4c 8b 0c 25 58 00 	mov    %gs:0x58,%r9
   1400070ed:	00 00 
   1400070ef:	49 8b 04 c1          	mov    (%r9,%rax,8),%rax
   1400070f3:	48 8b b0 38 00 00 00 	mov    0x38(%rax),%rsi
   1400070fa:	48 8d 46 ff          	lea    -0x1(%rsi),%rax
   1400070fe:	48 83 f8 fd          	cmp    $0xfffffffffffffffd,%rax
   140007102:	0f 87 da 03 00 00    	ja     0x1400074e2
   140007108:	48 89 c8             	mov    %rcx,%rax
   14000710b:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   140007111:	48 39 86 30 01 00 00 	cmp    %rax,0x130(%rsi)
   140007118:	75 1d                	jne    0x140007137
   14000711a:	48 8b be 38 01 00 00 	mov    0x138(%rsi),%rdi
   140007121:	44 0f b6 4f 1c       	movzbl 0x1c(%rdi),%r9d
   140007126:	41 83 f9 02          	cmp    $0x2,%r9d
   14000712a:	74 4b                	je     0x140007177
   14000712c:	81 e1 ff ff 3f 00    	and    $0x3fffff,%ecx
   140007132:	49 89 cf             	mov    %rcx,%r15
   140007135:	eb 50                	jmp    0x140007187
   140007137:	49 89 c2             	mov    %rax,%r10
   14000713a:	49 c1 ea 16          	shr    $0x16,%r10
   14000713e:	45 89 d1             	mov    %r10d,%r9d
   140007141:	41 83 e1 0f          	and    $0xf,%r9d
   140007145:	41 c1 e1 04          	shl    $0x4,%r9d
   140007149:	4a 39 44 0e 30       	cmp    %rax,0x30(%rsi,%r9,1)
   14000714e:	0f 85 65 01 00 00    	jne    0x1400072b9
   140007154:	4a 8b 7c 0e 38       	mov    0x38(%rsi,%r9,1),%rdi
   140007159:	48 39 c7             	cmp    %rax,%rdi
   14000715c:	75 0e                	jne    0x14000716c
   14000715e:	48 89 86 30 01 00 00 	mov    %rax,0x130(%rsi)
   140007165:	48 89 be 38 01 00 00 	mov    %rdi,0x138(%rsi)
   14000716c:	44 0f b6 4f 1c       	movzbl 0x1c(%rdi),%r9d
   140007171:	41 83 f9 02          	cmp    $0x2,%r9d
   140007175:	75 b5                	jne    0x14000712c
   140007177:	49 89 cf             	mov    %rcx,%r15
   14000717a:	49 29 ff             	sub    %rdi,%r15
   14000717d:	4c 3b 7f 38          	cmp    0x38(%rdi),%r15
   140007181:	0f 83 5b 03 00 00    	jae    0x1400074e2
   140007187:	4a 8d 1c 3f          	lea    (%rdi,%r15,1),%rbx
   14000718b:	49 83 f8 11          	cmp    $0x11,%r8
   14000718f:	b8 10 00 00 00       	mov    $0x10,%eax
   140007194:	49 0f 43 c0          	cmovae %r8,%rax
   140007198:	48 83 fa 11          	cmp    $0x11,%rdx
   14000719c:	73 75                	jae    0x140007213
   14000719e:	48 8d 48 ff          	lea    -0x1(%rax),%rcx
   1400071a2:	48 81 f9 bf f2 03 00 	cmp    $0x3f2bf,%rcx
   1400071a9:	0f 87 d1 00 00 00    	ja     0x140007280
   1400071af:	48 c1 e9 04          	shr    $0x4,%rcx
   1400071b3:	48 8d 05 9e 9e 03 00 	lea    0x39e9e(%rip),%rax        # 0x140041058
   1400071ba:	44 0f b6 34 01       	movzbl (%rcx,%rax,1),%r14d
   1400071bf:	49 83 fe 31          	cmp    $0x31,%r14
   1400071c3:	0f 83 a2 05 00 00    	jae    0x14000776b
   1400071c9:	44 89 f0             	mov    %r14d,%eax
   1400071cc:	c1 e0 07             	shl    $0x7,%eax
   1400071cf:	4a 8d 0c f0          	lea    (%rax,%r14,8),%rcx
   1400071d3:	0f b6 84 0e a8 04 00 	movzbl 0x4a8(%rsi,%rcx,1),%eax
   1400071da:	00 
   1400071db:	45 84 c9             	test   %r9b,%r9b
   1400071de:	0f 84 46 03 00 00    	je     0x14000752a
   1400071e4:	45 0f b6 c9          	movzbl %r9b,%r9d
   1400071e8:	41 83 f9 02          	cmp    $0x2,%r9d
   1400071ec:	0f 84 00 03 00 00    	je     0x1400074f2
   1400071f2:	41 83 f9 01          	cmp    $0x1,%r9d
   1400071f6:	0f 85 41 05 00 00    	jne    0x14000773d
   1400071fc:	41 b9 00 20 04 00    	mov    $0x42000,%r9d
   140007202:	44 89 fa             	mov    %r15d,%edx
   140007205:	4c 39 ca             	cmp    %r9,%rdx
   140007208:	0f 83 2e 03 00 00    	jae    0x14000753c
   14000720e:	e9 2a 05 00 00       	jmp    0x14000773d
   140007213:	48 39 d0             	cmp    %rdx,%rax
   140007216:	49 89 d2             	mov    %rdx,%r10
   140007219:	4c 0f 47 d0          	cmova  %rax,%r10
   14000721d:	49 ff ca             	dec    %r10
   140007220:	49 81 fa bf f2 03 00 	cmp    $0x3f2bf,%r10
   140007227:	77 57                	ja     0x140007280
   140007229:	49 c1 ea 04          	shr    $0x4,%r10
   14000722d:	48 8d 0d 7c f1 03 00 	lea    0x3f17c(%rip),%rcx        # 0x1400463b0
   140007234:	45 0f b6 9c 0a 94 01 	movzbl 0x194(%r10,%rcx,1),%r11d
   14000723b:	00 00 
   14000723d:	41 80 fb 30          	cmp    $0x30,%r11b
   140007241:	77 3d                	ja     0x140007280
   140007243:	4c 8d 52 ff          	lea    -0x1(%rdx),%r10
   140007247:	66 0f 1f 84 00 00 00 	nopw   0x0(%rax,%rax,1)
   14000724e:	00 00 
   140007250:	45 0f b6 f3          	movzbl %r11b,%r14d
   140007254:	4e 8b 1c f1          	mov    (%rcx,%r14,8),%r11
   140007258:	4d 85 d3             	test   %r10,%r11
   14000725b:	0f 84 68 ff ff ff    	je     0x1400071c9
   140007261:	4d 09 d3             	or     %r10,%r11
   140007264:	49 81 fb bf f2 03 00 	cmp    $0x3f2bf,%r11
   14000726b:	77 13                	ja     0x140007280
   14000726d:	49 c1 eb 04          	shr    $0x4,%r11
   140007271:	45 0f b6 9c 0b 94 01 	movzbl 0x194(%r11,%rcx,1),%r11d
   140007278:	00 00 
   14000727a:	41 80 fb 31          	cmp    $0x31,%r11b
   14000727e:	72 d0                	jb     0x140007250
   140007280:	41 83 f9 02          	cmp    $0x2,%r9d
   140007284:	0f 83 e0 00 00 00    	jae    0x14000736a
   14000728a:	48 83 fa 11          	cmp    $0x11,%rdx
   14000728e:	0f 83 f2 00 00 00    	jae    0x140007386
   140007294:	48 ff c8             	dec    %rax
   140007297:	48 3d bf f2 03 00    	cmp    $0x3f2bf,%rax
   14000729d:	0f 87 4e 01 00 00    	ja     0x1400073f1
   1400072a3:	48 c1 e8 04          	shr    $0x4,%rax
   1400072a7:	48 8d 0d aa 9d 03 00 	lea    0x39daa(%rip),%rcx        # 0x140041058
   1400072ae:	0f b6 04 08          	movzbl (%rax,%rcx,1),%eax
   1400072b2:	b1 01                	mov    $0x1,%cl
   1400072b4:	e9 3a 01 00 00       	jmp    0x1400073f3
   1400072b9:	49 01 f1             	add    %rsi,%r9
   1400072bc:	49 83 c1 30          	add    $0x30,%r9
   1400072c0:	49 89 c3             	mov    %rax,%r11
   1400072c3:	49 c1 eb 23          	shr    $0x23,%r11
   1400072c7:	48 89 c7             	mov    %rax,%rdi
   1400072ca:	48 c1 ef 30          	shr    $0x30,%rdi
   1400072ce:	41 31 fb             	xor    %edi,%r11d
   1400072d1:	45 31 da             	xor    %r11d,%r10d
   1400072d4:	41 81 e2 ff 1f 00 00 	and    $0x1fff,%r10d
   1400072db:	4c 8b 9e 40 01 00 00 	mov    0x140(%rsi),%r11
   1400072e2:	4c 89 d3             	mov    %r10,%rbx
   1400072e5:	66 66 2e 0f 1f 84 00 	data16 cs nopw 0x0(%rax,%rax,1)
   1400072ec:	00 00 00 00 
   1400072f0:	49 8b 3c db          	mov    (%r11,%rbx,8),%rdi
   1400072f4:	48 85 ff             	test   %rdi,%rdi
   1400072f7:	0f 84 e5 01 00 00    	je     0x1400074e2
   1400072fd:	49 89 fe             	mov    %rdi,%r14
   140007300:	49 81 e6 00 00 c0 ff 	and    $0xffffffffffc00000,%r14
   140007307:	49 39 c6             	cmp    %rax,%r14
   14000730a:	74 12                	je     0x14000731e
   14000730c:	ff c3                	inc    %ebx
   14000730e:	81 e3 ff 1f 00 00    	and    $0x1fff,%ebx
   140007314:	4c 39 d3             	cmp    %r10,%rbx
   140007317:	75 d7                	jne    0x1400072f0
   140007319:	e9 c4 01 00 00       	jmp    0x1400074e2
   14000731e:	49 89 fa             	mov    %rdi,%r10
   140007321:	49 81 e2 ff ff 3f 00 	and    $0x3fffff,%r10
   140007328:	74 25                	je     0x14000734f
   14000732a:	4d 8d 5a ff          	lea    -0x1(%r10),%r11
   14000732e:	8b be 58 01 00 00    	mov    0x158(%rsi),%edi
   140007334:	49 39 fb             	cmp    %rdi,%r11
   140007337:	0f 83 a5 01 00 00    	jae    0x1400074e2
   14000733d:	4c 8b 5e 20          	mov    0x20(%rsi),%r11
   140007341:	4b 8b 7c d3 f8       	mov    -0x8(%r11,%r10,8),%rdi
   140007346:	48 85 ff             	test   %rdi,%rdi
   140007349:	0f 84 93 01 00 00    	je     0x1400074e2
   14000734f:	49 89 01             	mov    %rax,(%r9)
   140007352:	49 89 79 08          	mov    %rdi,0x8(%r9)
   140007356:	44 0f b6 4f 1c       	movzbl 0x1c(%rdi),%r9d
   14000735b:	41 83 f9 02          	cmp    $0x2,%r9d
   14000735f:	0f 84 12 fe ff ff    	je     0x140007177
   140007365:	e9 c2 fd ff ff       	jmp    0x14000712c
   14000736a:	0f 85 cd 03 00 00    	jne    0x14000773d
   140007370:	48 89 f1             	mov    %rsi,%rcx
   140007373:	48 89 da             	mov    %rbx,%rdx
   140007376:	4c 89 c6             	mov    %r8,%rsi
   140007379:	49 89 f8             	mov    %rdi,%r8
   14000737c:	e8 bf 42 03 00       	call   0x14003b640
   140007381:	e9 64 01 00 00       	jmp    0x1400074ea
   140007386:	48 39 d0             	cmp    %rdx,%rax
   140007389:	48 0f 46 c2          	cmovbe %rdx,%rax
   14000738d:	48 ff c8             	dec    %rax
   140007390:	48 3d bf f2 03 00    	cmp    $0x3f2bf,%rax
   140007396:	77 59                	ja     0x1400073f1
   140007398:	48 c1 e8 04          	shr    $0x4,%rax
   14000739c:	4c 8d 15 0d f0 03 00 	lea    0x3f00d(%rip),%r10        # 0x1400463b0
   1400073a3:	46 0f b6 9c 10 94 01 	movzbl 0x194(%rax,%r10,1),%r11d
   1400073aa:	00 00 
   1400073ac:	41 80 fb 30          	cmp    $0x30,%r11b
   1400073b0:	77 3f                	ja     0x1400073f1
   1400073b2:	48 ff ca             	dec    %rdx
   1400073b5:	66 66 2e 0f 1f 84 00 	data16 cs nopw 0x0(%rax,%rax,1)
   1400073bc:	00 00 00 00 
   1400073c0:	41 0f b6 c3          	movzbl %r11b,%eax
   1400073c4:	4d 8b 1c c2          	mov    (%r10,%rax,8),%r11
   1400073c8:	49 85 d3             	test   %rdx,%r11
   1400073cb:	0f 94 c1             	sete   %cl
   1400073ce:	74 23                	je     0x1400073f3
   1400073d0:	49 09 d3             	or     %rdx,%r11
   1400073d3:	49 81 fb bf f2 03 00 	cmp    $0x3f2bf,%r11
   1400073da:	77 17                	ja     0x1400073f3
   1400073dc:	49 c1 eb 04          	shr    $0x4,%r11
   1400073e0:	47 0f b6 9c 13 94 01 	movzbl 0x194(%r11,%r10,1),%r11d
   1400073e7:	00 00 
   1400073e9:	41 80 fb 31          	cmp    $0x31,%r11b
   1400073ed:	72 d1                	jb     0x1400073c0
   1400073ef:	eb 02                	jmp    0x1400073f3
   1400073f1:	31 c9                	xor    %ecx,%ecx
   1400073f3:	41 83 f9 01          	cmp    $0x1,%r9d
   1400073f7:	74 6c                	je     0x140007465
   1400073f9:	45 85 c9             	test   %r9d,%r9d
   1400073fc:	0f 85 3b 03 00 00    	jne    0x14000773d
   140007402:	84 c9                	test   %cl,%cl
   140007404:	0f 84 33 03 00 00    	je     0x14000773d
   14000740a:	44 89 f9             	mov    %r15d,%ecx
   14000740d:	48 81 f9 00 f0 05 00 	cmp    $0x5f000,%rcx
   140007414:	0f 82 23 03 00 00    	jb     0x14000773d
   14000741a:	48 3b 0f             	cmp    (%rdi),%rcx
   14000741d:	0f 83 1a 03 00 00    	jae    0x14000773d
   140007423:	41 f6 c7 0f          	test   $0xf,%r15b
   140007427:	0f 85 10 03 00 00    	jne    0x14000773d
   14000742d:	44 89 fa             	mov    %r15d,%edx
   140007430:	c1 ea 04             	shr    $0x4,%edx
   140007433:	44 8d 48 01          	lea    0x1(%rax),%r9d
   140007437:	44 38 8c 17 88 15 00 	cmp    %r9b,0x1588(%rdi,%rdx,1)
   14000743e:	00 
   14000743f:	0f 85 f8 02 00 00    	jne    0x14000773d
   140007445:	48 83 f8 30          	cmp    $0x30,%rax
   140007449:	0f 86 27 02 00 00    	jbe    0x140007676
   14000744f:	48 c7 04 0f 00 00 00 	movq   $0x0,(%rdi,%rcx,1)
   140007456:	00 
   140007457:	41 80 c9 80          	or     $0x80,%r9b
   14000745b:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   140007462:	00 
   140007463:	eb 65                	jmp    0x1400074ca
   140007465:	84 c9                	test   %cl,%cl
   140007467:	0f 84 d0 02 00 00    	je     0x14000773d
   14000746d:	44 89 f9             	mov    %r15d,%ecx
   140007470:	48 81 f9 00 20 04 00 	cmp    $0x42000,%rcx
   140007477:	0f 82 c0 02 00 00    	jb     0x14000773d
   14000747d:	48 3b 0f             	cmp    (%rdi),%rcx
   140007480:	0f 83 b7 02 00 00    	jae    0x14000773d
   140007486:	41 f6 c7 0f          	test   $0xf,%r15b
   14000748a:	0f 85 ad 02 00 00    	jne    0x14000773d
   140007490:	44 89 fa             	mov    %r15d,%edx
   140007493:	c1 ea 04             	shr    $0x4,%edx
   140007496:	44 0f b6 8c 17 88 15 	movzbl 0x1588(%rdi,%rdx,1),%r9d
   14000749d:	00 00 
   14000749f:	44 8d 50 01          	lea    0x1(%rax),%r10d
   1400074a3:	45 38 d1             	cmp    %r10b,%r9b
   1400074a6:	0f 85 91 02 00 00    	jne    0x14000773d
   1400074ac:	48 83 f8 30          	cmp    $0x30,%rax
   1400074b0:	0f 86 f9 01 00 00    	jbe    0x1400076af
   1400074b6:	48 c7 04 0f 00 00 00 	movq   $0x0,(%rdi,%rcx,1)
   1400074bd:	00 
   1400074be:	41 80 c9 80          	or     $0x80,%r9b
   1400074c2:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   1400074c9:	00 
   1400074ca:	8b 4f 14             	mov    0x14(%rdi),%ecx
   1400074cd:	85 c9                	test   %ecx,%ecx
   1400074cf:	0f 84 83 02 00 00    	je     0x140007758
   1400074d5:	4c 89 c3             	mov    %r8,%rbx
   1400074d8:	ff c9                	dec    %ecx
   1400074da:	89 4f 14             	mov    %ecx,0x14(%rdi)
   1400074dd:	e9 16 02 00 00       	jmp    0x1400076f8
   1400074e2:	4c 89 c6             	mov    %r8,%rsi
   1400074e5:	e8 26 31 03 00       	call   0x14003a610
   1400074ea:	49 89 f0             	mov    %rsi,%r8
   1400074ed:	e9 4b 02 00 00       	jmp    0x14000773d
   1400074f2:	48 8b 47 30          	mov    0x30(%rdi),%rax
   1400074f6:	48 8b 4f 08          	mov    0x8(%rdi),%rcx
   1400074fa:	48 01 f8             	add    %rdi,%rax
   1400074fd:	0f 82 3a 02 00 00    	jb     0x14000773d
   140007503:	48 39 d8             	cmp    %rbx,%rax
   140007506:	0f 85 31 02 00 00    	jne    0x14000773d
   14000750c:	48 89 7c 24 20       	mov    %rdi,0x20(%rsp)
   140007511:	48 89 f1             	mov    %rsi,%rcx
   140007514:	48 89 d0             	mov    %rdx,%rax
   140007517:	48 89 da             	mov    %rbx,%rdx
   14000751a:	4c 89 c6             	mov    %r8,%rsi
   14000751d:	49 89 c0             	mov    %rax,%r8
   140007520:	49 89 f1             	mov    %rsi,%r9
   140007523:	e8 c8 56 03 00       	call   0x14003cbf0
   140007528:	eb c0                	jmp    0x1400074ea
   14000752a:	41 b9 00 f0 05 00    	mov    $0x5f000,%r9d
   140007530:	44 89 fa             	mov    %r15d,%edx
   140007533:	4c 39 ca             	cmp    %r9,%rdx
   140007536:	0f 82 01 02 00 00    	jb     0x14000773d
   14000753c:	41 f6 c7 0f          	test   $0xf,%r15b
   140007540:	0f 85 f7 01 00 00    	jne    0x14000773d
   140007546:	41 c1 ef 04          	shr    $0x4,%r15d
   14000754a:	42 0f b6 ac 3f 88 15 	movzbl 0x1588(%rdi,%r15,1),%ebp
   140007551:	00 00 
   140007553:	45 8d 4e 01          	lea    0x1(%r14),%r9d
   140007557:	44 38 cd             	cmp    %r9b,%bpl
   14000755a:	0f 85 dd 01 00 00    	jne    0x14000773d
   140007560:	48 3b 17             	cmp    (%rdi),%rdx
   140007563:	0f 83 d4 01 00 00    	jae    0x14000773d
   140007569:	4c 8d 24 0e          	lea    (%rsi,%rcx,1),%r12
   14000756d:	49 81 c4 a8 04 00 00 	add    $0x4a8,%r12
   140007574:	44 0f b6 e8          	movzbl %al,%r13d
   140007578:	48 8d 0d 06 da 03 00 	lea    0x3da06(%rip),%rcx        # 0x140044f85
   14000757f:	45 3a 2c 0e          	cmp    (%r14,%rcx,1),%r13b
   140007583:	73 20                	jae    0x1400075a5
   140007585:	40 80 cd 40          	or     $0x40,%bpl
   140007589:	42 88 ac 3f 88 15 00 	mov    %bpl,0x1588(%rdi,%r15,1)
   140007590:	00 
   140007591:	41 83 e5 0f          	and    $0xf,%r13d
   140007595:	4b 89 5c ec 08       	mov    %rbx,0x8(%r12,%r13,8)
   14000759a:	fe c0                	inc    %al
   14000759c:	41 88 04 24          	mov    %al,(%r12)
   1400075a0:	e9 98 01 00 00       	jmp    0x14000773d
   1400075a5:	41 8d 46 e6          	lea    -0x1a(%r14),%eax
   1400075a9:	83 f8 17             	cmp    $0x17,%eax
   1400075ac:	73 4c                	jae    0x1400075fa
   1400075ae:	4c 89 44 24 30       	mov    %r8,0x30(%rsp)
   1400075b3:	49 8d 4d ff          	lea    -0x1(%r13),%rcx
   1400075b7:	48 83 f9 10          	cmp    $0x10,%rcx
   1400075bb:	0f 83 c0 01 00 00    	jae    0x140007781
   1400075c1:	4b 8b 04 ec          	mov    (%r12,%r13,8),%rax
   1400075c5:	48 89 44 24 28       	mov    %rax,0x28(%rsp)
   1400075ca:	4c 8d 44 24 28       	lea    0x28(%rsp),%r8
   1400075cf:	41 b9 01 00 00 00    	mov    $0x1,%r9d
   1400075d5:	48 89 f1             	mov    %rsi,%rcx
   1400075d8:	4c 89 f2             	mov    %r14,%rdx
   1400075db:	e8 80 20 00 00       	call   0x140009660
   1400075e0:	4b 89 1c ec          	mov    %rbx,(%r12,%r13,8)
   1400075e4:	40 80 cd 40          	or     $0x40,%bpl
   1400075e8:	42 88 ac 3f 88 15 00 	mov    %bpl,0x1588(%rdi,%r15,1)
   1400075ef:	00 
   1400075f0:	4c 8b 44 24 30       	mov    0x30(%rsp),%r8
   1400075f5:	e9 43 01 00 00       	jmp    0x14000773d
   1400075fa:	49 8d 44 24 08       	lea    0x8(%r12),%rax
   1400075ff:	41 b9 08 00 00 00    	mov    $0x8,%r9d
   140007605:	48 89 f1             	mov    %rsi,%rcx
   140007608:	4c 89 f2             	mov    %r14,%rdx
   14000760b:	4d 89 c5             	mov    %r8,%r13
   14000760e:	49 89 c0             	mov    %rax,%r8
   140007611:	e8 4a 20 00 00       	call   0x140009660
   140007616:	4d 89 e8             	mov    %r13,%r8
   140007619:	44 89 f0             	mov    %r14d,%eax
   14000761c:	c1 e0 07             	shl    $0x7,%eax
   14000761f:	4a 8d 04 f0          	lea    (%rax,%r14,8),%rax
   140007623:	0f 10 84 06 f0 04 00 	movups 0x4f0(%rsi,%rax,1),%xmm0
   14000762a:	00 
   14000762b:	0f 10 8c 06 00 05 00 	movups 0x500(%rsi,%rax,1),%xmm1
   140007632:	00 
   140007633:	0f 10 94 06 10 05 00 	movups 0x510(%rsi,%rax,1),%xmm2
   14000763a:	00 
   14000763b:	0f 10 9c 06 20 05 00 	movups 0x520(%rsi,%rax,1),%xmm3
   140007642:	00 
   140007643:	41 0f 11 5c 24 38    	movups %xmm3,0x38(%r12)
   140007649:	41 0f 11 54 24 28    	movups %xmm2,0x28(%r12)
   14000764f:	41 0f 11 4c 24 18    	movups %xmm1,0x18(%r12)
   140007655:	41 0f 11 44 24 08    	movups %xmm0,0x8(%r12)
   14000765b:	40 80 cd 40          	or     $0x40,%bpl
   14000765f:	42 88 ac 3f 88 15 00 	mov    %bpl,0x1588(%rdi,%r15,1)
   140007666:	00 
   140007667:	49 89 5c 24 48       	mov    %rbx,0x48(%r12)
   14000766c:	41 c6 04 24 09       	movb   $0x9,(%r12)
   140007671:	e9 c7 00 00 00       	jmp    0x14000773d
   140007676:	4c 89 c3             	mov    %r8,%rbx
   140007679:	44 8b 84 87 00 14 00 	mov    0x1400(%rdi,%rax,4),%r8d
   140007680:	00 
   140007681:	4e 8d 14 07          	lea    (%rdi,%r8,1),%r10
   140007685:	45 31 db             	xor    %r11d,%r11d
   140007688:	41 be ff ff ff ff    	mov    $0xffffffff,%r14d
   14000768e:	4d 39 f0             	cmp    %r14,%r8
   140007691:	4d 0f 45 da          	cmovne %r10,%r11
   140007695:	4c 89 1c 0f          	mov    %r11,(%rdi,%rcx,1)
   140007699:	44 89 bc 87 00 14 00 	mov    %r15d,0x1400(%rdi,%rax,4)
   1400076a0:	00 
   1400076a1:	41 80 c9 80          	or     $0x80,%r9b
   1400076a5:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   1400076ac:	00 
   1400076ad:	eb 37                	jmp    0x1400076e6
   1400076af:	4c 89 c3             	mov    %r8,%rbx
   1400076b2:	44 8b 84 87 00 14 00 	mov    0x1400(%rdi,%rax,4),%r8d
   1400076b9:	00 
   1400076ba:	4e 8d 14 07          	lea    (%rdi,%r8,1),%r10
   1400076be:	45 31 db             	xor    %r11d,%r11d
   1400076c1:	41 be ff ff ff ff    	mov    $0xffffffff,%r14d
   1400076c7:	4d 39 f0             	cmp    %r14,%r8
   1400076ca:	4d 0f 45 da          	cmovne %r10,%r11
   1400076ce:	4c 89 1c 0f          	mov    %r11,(%rdi,%rcx,1)
   1400076d2:	44 89 bc 87 00 14 00 	mov    %r15d,0x1400(%rdi,%rax,4)
   1400076d9:	00 
   1400076da:	41 80 c9 80          	or     $0x80,%r9b
   1400076de:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   1400076e5:	00 
   1400076e6:	8b 4f 14             	mov    0x14(%rdi),%ecx
   1400076e9:	85 c9                	test   %ecx,%ecx
   1400076eb:	74 6b                	je     0x140007758
   1400076ed:	ff c9                	dec    %ecx
   1400076ef:	89 4f 14             	mov    %ecx,0x14(%rdi)
   1400076f2:	41 83 f8 ff          	cmp    $0xffffffff,%r8d
   1400076f6:	75 16                	jne    0x14000770e
   1400076f8:	44 8b 47 70          	mov    0x70(%rdi),%r8d
   1400076fc:	48 8b 8e e8 03 00 00 	mov    0x3e8(%rsi),%rcx
   140007703:	48 89 c2             	mov    %rax,%rdx
   140007706:	e8 f5 23 00 00       	call   0x140009b00
   14000770b:	8b 4f 14             	mov    0x14(%rdi),%ecx
   14000770e:	85 c9                	test   %ecx,%ecx
   140007710:	49 89 d8             	mov    %rbx,%r8
   140007713:	75 28                	jne    0x14000773d
   140007715:	48 3b be a8 03 00 00 	cmp    0x3a8(%rsi),%rdi
   14000771c:	74 1f                	je     0x14000773d
   14000771e:	83 7f 18 00          	cmpl   $0x0,0x18(%rdi)
   140007722:	75 19                	jne    0x14000773d
   140007724:	80 7f 1c 01          	cmpb   $0x1,0x1c(%rdi)
   140007728:	75 13                	jne    0x14000773d
   14000772a:	48 89 f1             	mov    %rsi,%rcx
   14000772d:	48 89 fa             	mov    %rdi,%rdx
   140007730:	4c 89 c6             	mov    %r8,%rsi
   140007733:	e8 38 24 00 00       	call   0x140009b70
   140007738:	e9 ad fd ff ff       	jmp    0x1400074ea
   14000773d:	4c 89 44 24 28       	mov    %r8,0x28(%rsp)
   140007742:	48 8d 44 24 28       	lea    0x28(%rsp),%rax
   140007747:	48 83 c4 38          	add    $0x38,%rsp
   14000774b:	5b                   	pop    %rbx
   14000774c:	5d                   	pop    %rbp
   14000774d:	5f                   	pop    %rdi
   14000774e:	5e                   	pop    %rsi
   14000774f:	41 5c                	pop    %r12
   140007751:	41 5d                	pop    %r13
   140007753:	41 5e                	pop    %r14
   140007755:	41 5f                	pop    %r15
   140007757:	c3                   	ret
   140007758:	48 8d 0d 49 8a 03 00 	lea    0x38a49(%rip),%rcx        # 0x1400401a8
   14000775f:	ba 3a 00 00 00       	mov    $0x3a,%edx
   140007764:	e8 c7 53 03 00       	call   0x14003cb30
   140007769:	0f 0b                	ud2
   14000776b:	4c 8d 05 16 8b 03 00 	lea    0x38b16(%rip),%r8        # 0x140040288
   140007772:	ba 31 00 00 00       	mov    $0x31,%edx
   140007777:	4c 89 f1             	mov    %r14,%rcx
   14000777a:	e8 d4 26 03 00       	call   0x140039e53
   14000777f:	0f 0b                	ud2
   140007781:	4c 8d 05 18 8b 03 00 	lea    0x38b18(%rip),%r8        # 0x1400402a0
   140007788:	ba 10 00 00 00       	mov    $0x10,%edx
   14000778d:	e8 c1 26 03 00       	call   0x140039e53
   140007792:	0f 0b                	ud2
