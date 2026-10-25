   140007010:	41 57                	push   %r15
   140007012:	41 56                	push   %r14
   140007014:	56                   	push   %rsi
   140007015:	57                   	push   %rdi
   140007016:	55                   	push   %rbp
   140007017:	53                   	push   %rbx
   140007018:	48 83 ec 38          	sub    $0x38,%rsp
   14000701c:	8b 05 ae 0b 06 00    	mov    0x60bae(%rip),%eax        # 0x140067bd0
   140007022:	65 4c 8b 0c 25 58 00 	mov    %gs:0x58,%r9
   140007029:	00 00 
   14000702b:	49 8b 04 c1          	mov    (%r9,%rax,8),%rax
   14000702f:	48 8b b0 38 00 00 00 	mov    0x38(%rax),%rsi
   140007036:	48 8d 46 ff          	lea    -0x1(%rsi),%rax
   14000703a:	48 83 f8 fd          	cmp    $0xfffffffffffffffd,%rax
   14000703e:	0f 87 bc 03 00 00    	ja     0x140007400
   140007044:	48 89 c8             	mov    %rcx,%rax
   140007047:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   14000704d:	48 39 86 30 01 00 00 	cmp    %rax,0x130(%rsi)
   140007054:	75 15                	jne    0x14000706b
   140007056:	48 8b be 38 01 00 00 	mov    0x138(%rsi),%rdi
   14000705d:	0f b6 5f 1c          	movzbl 0x1c(%rdi),%ebx
   140007061:	83 fb 02             	cmp    $0x2,%ebx
   140007064:	74 43                	je     0x1400070a9
   140007066:	e9 f8 00 00 00       	jmp    0x140007163
   14000706b:	49 89 c2             	mov    %rax,%r10
   14000706e:	49 c1 ea 16          	shr    $0x16,%r10
   140007072:	45 89 d1             	mov    %r10d,%r9d
   140007075:	41 83 e1 0f          	and    $0xf,%r9d
   140007079:	41 c1 e1 04          	shl    $0x4,%r9d
   14000707d:	4a 39 44 0e 30       	cmp    %rax,0x30(%rsi,%r9,1)
   140007082:	75 3a                	jne    0x1400070be
   140007084:	4a 8b 7c 0e 38       	mov    0x38(%rsi,%r9,1),%rdi
   140007089:	48 39 c7             	cmp    %rax,%rdi
   14000708c:	75 0e                	jne    0x14000709c
   14000708e:	48 89 86 30 01 00 00 	mov    %rax,0x130(%rsi)
   140007095:	48 89 be 38 01 00 00 	mov    %rdi,0x138(%rsi)
   14000709c:	0f b6 5f 1c          	movzbl 0x1c(%rdi),%ebx
   1400070a0:	83 fb 02             	cmp    $0x2,%ebx
   1400070a3:	0f 85 ba 00 00 00    	jne    0x140007163
   1400070a9:	49 89 cb             	mov    %rcx,%r11
   1400070ac:	49 29 fb             	sub    %rdi,%r11
   1400070af:	4c 3b 5f 38          	cmp    0x38(%rdi),%r11
   1400070b3:	0f 82 b3 00 00 00    	jb     0x14000716c
   1400070b9:	e9 42 03 00 00       	jmp    0x140007400
   1400070be:	49 01 f1             	add    %rsi,%r9
   1400070c1:	49 83 c1 30          	add    $0x30,%r9
   1400070c5:	49 89 c3             	mov    %rax,%r11
   1400070c8:	49 c1 eb 23          	shr    $0x23,%r11
   1400070cc:	48 89 c7             	mov    %rax,%rdi
   1400070cf:	48 c1 ef 30          	shr    $0x30,%rdi
   1400070d3:	41 31 fb             	xor    %edi,%r11d
   1400070d6:	45 31 da             	xor    %r11d,%r10d
   1400070d9:	41 81 e2 ff 1f 00 00 	and    $0x1fff,%r10d
   1400070e0:	4c 8b 9e 40 01 00 00 	mov    0x140(%rsi),%r11
   1400070e7:	4c 89 d3             	mov    %r10,%rbx
   1400070ea:	66 0f 1f 44 00 00    	nopw   0x0(%rax,%rax,1)
   1400070f0:	49 8b 3c db          	mov    (%r11,%rbx,8),%rdi
   1400070f4:	48 85 ff             	test   %rdi,%rdi
   1400070f7:	0f 84 03 03 00 00    	je     0x140007400
   1400070fd:	49 89 fe             	mov    %rdi,%r14
   140007100:	49 81 e6 00 00 c0 ff 	and    $0xffffffffffc00000,%r14
   140007107:	49 39 c6             	cmp    %rax,%r14
   14000710a:	74 12                	je     0x14000711e
   14000710c:	ff c3                	inc    %ebx
   14000710e:	81 e3 ff 1f 00 00    	and    $0x1fff,%ebx
   140007114:	4c 39 d3             	cmp    %r10,%rbx
   140007117:	75 d7                	jne    0x1400070f0
   140007119:	e9 e2 02 00 00       	jmp    0x140007400
   14000711e:	49 89 fa             	mov    %rdi,%r10
   140007121:	49 81 e2 ff ff 3f 00 	and    $0x3fffff,%r10
   140007128:	74 25                	je     0x14000714f
   14000712a:	4d 8d 5a ff          	lea    -0x1(%r10),%r11
   14000712e:	8b be 58 01 00 00    	mov    0x158(%rsi),%edi
   140007134:	49 39 fb             	cmp    %rdi,%r11
   140007137:	0f 83 c3 02 00 00    	jae    0x140007400
   14000713d:	4c 8b 5e 20          	mov    0x20(%rsi),%r11
   140007141:	4b 8b 7c d3 f8       	mov    -0x8(%r11,%r10,8),%rdi
   140007146:	48 85 ff             	test   %rdi,%rdi
   140007149:	0f 84 b1 02 00 00    	je     0x140007400
   14000714f:	49 89 01             	mov    %rax,(%r9)
   140007152:	49 89 79 08          	mov    %rdi,0x8(%r9)
   140007156:	0f b6 5f 1c          	movzbl 0x1c(%rdi),%ebx
   14000715a:	83 fb 02             	cmp    $0x2,%ebx
   14000715d:	0f 84 46 ff ff ff    	je     0x1400070a9
   140007163:	81 e1 ff ff 3f 00    	and    $0x3fffff,%ecx
   140007169:	49 89 cb             	mov    %rcx,%r11
   14000716c:	4a 8d 04 1f          	lea    (%rdi,%r11,1),%rax
   140007170:	49 83 f8 11          	cmp    $0x11,%r8
   140007174:	b9 10 00 00 00       	mov    $0x10,%ecx
   140007179:	49 0f 43 c8          	cmovae %r8,%rcx
   14000717d:	48 83 fa 11          	cmp    $0x11,%rdx
   140007181:	73 71                	jae    0x1400071f4
   140007183:	4c 8d 49 ff          	lea    -0x1(%rcx),%r9
   140007187:	49 81 f9 bf f2 03 00 	cmp    $0x3f2bf,%r9
   14000718e:	0f 87 cc 00 00 00    	ja     0x140007260
   140007194:	49 c1 e9 04          	shr    $0x4,%r9
   140007198:	48 8d 0d 39 9e 03 00 	lea    0x39e39(%rip),%rcx        # 0x140040fd8
   14000719f:	45 0f b6 14 09       	movzbl (%r9,%rcx,1),%r10d
   1400071a4:	49 83 fa 31          	cmp    $0x31,%r10
   1400071a8:	0f 83 34 04 00 00    	jae    0x1400075e2
   1400071ae:	44 89 d1             	mov    %r10d,%ecx
   1400071b1:	c1 e1 07             	shl    $0x7,%ecx
   1400071b4:	4a 8d 0c d1          	lea    (%rcx,%r10,8),%rcx
   1400071b8:	44 0f b6 8c 0e a8 04 	movzbl 0x4a8(%rsi,%rcx,1),%r9d
   1400071bf:	00 00 
   1400071c1:	84 db                	test   %bl,%bl
   1400071c3:	0f 84 82 02 00 00    	je     0x14000744b
   1400071c9:	0f b6 db             	movzbl %bl,%ebx
   1400071cc:	83 fb 02             	cmp    $0x2,%ebx
   1400071cf:	0f 84 3b 02 00 00    	je     0x140007410
   1400071d5:	83 fb 01             	cmp    $0x1,%ebx
   1400071d8:	0f 85 d3 02 00 00    	jne    0x1400074b1
   1400071de:	bb 00 20 04 00       	mov    $0x42000,%ebx
   1400071e3:	44 89 da             	mov    %r11d,%edx
   1400071e6:	48 39 da             	cmp    %rbx,%rdx
   1400071e9:	0f 83 69 02 00 00    	jae    0x140007458
   1400071ef:	e9 bd 02 00 00       	jmp    0x1400074b1
   1400071f4:	48 39 d1             	cmp    %rdx,%rcx
   1400071f7:	49 89 d2             	mov    %rdx,%r10
   1400071fa:	4c 0f 47 d1          	cmova  %rcx,%r10
   1400071fe:	49 ff ca             	dec    %r10
   140007201:	49 81 fa bf f2 03 00 	cmp    $0x3f2bf,%r10
   140007208:	77 56                	ja     0x140007260
   14000720a:	49 c1 ea 04          	shr    $0x4,%r10
   14000720e:	4c 8d 0d 33 f1 03 00 	lea    0x3f133(%rip),%r9        # 0x140046348
   140007215:	47 0f b6 94 0a 94 01 	movzbl 0x194(%r10,%r9,1),%r10d
   14000721c:	00 00 
   14000721e:	41 80 fa 30          	cmp    $0x30,%r10b
   140007222:	77 3c                	ja     0x140007260
   140007224:	4c 8d 72 ff          	lea    -0x1(%rdx),%r14
   140007228:	0f 1f 84 00 00 00 00 	nopl   0x0(%rax,%rax,1)
   14000722f:	00 
   140007230:	45 0f b6 d2          	movzbl %r10b,%r10d
   140007234:	4f 8b 3c d1          	mov    (%r9,%r10,8),%r15
   140007238:	4d 85 f7             	test   %r14,%r15
   14000723b:	0f 84 6d ff ff ff    	je     0x1400071ae
   140007241:	4d 09 f7             	or     %r14,%r15
   140007244:	49 81 ff bf f2 03 00 	cmp    $0x3f2bf,%r15
   14000724b:	77 13                	ja     0x140007260
   14000724d:	49 c1 ef 04          	shr    $0x4,%r15
   140007251:	47 0f b6 94 0f 94 01 	movzbl 0x194(%r15,%r9,1),%r10d
   140007258:	00 00 
   14000725a:	41 80 fa 31          	cmp    $0x31,%r10b
   14000725e:	72 d0                	jb     0x140007230
   140007260:	83 fb 02             	cmp    $0x2,%ebx
   140007263:	73 2c                	jae    0x140007291
   140007265:	48 83 fa 11          	cmp    $0x11,%rdx
   140007269:	73 45                	jae    0x1400072b0
   14000726b:	48 ff c9             	dec    %rcx
   14000726e:	48 81 f9 bf f2 03 00 	cmp    $0x3f2bf,%rcx
   140007275:	0f 87 96 00 00 00    	ja     0x140007311
   14000727b:	48 c1 e9 04          	shr    $0x4,%rcx
   14000727f:	48 8d 05 52 9d 03 00 	lea    0x39d52(%rip),%rax        # 0x140040fd8
   140007286:	0f b6 04 01          	movzbl (%rcx,%rax,1),%eax
   14000728a:	b1 01                	mov    $0x1,%cl
   14000728c:	e9 82 00 00 00       	jmp    0x140007313
   140007291:	0f 85 1a 02 00 00    	jne    0x1400074b1
   140007297:	48 89 f1             	mov    %rsi,%rcx
   14000729a:	48 89 c2             	mov    %rax,%rdx
   14000729d:	4c 89 c6             	mov    %r8,%rsi
   1400072a0:	49 89 f8             	mov    %rdi,%r8
   1400072a3:	e8 58 3d 03 00       	call   0x14003b000
   1400072a8:	49 89 f0             	mov    %rsi,%r8
   1400072ab:	e9 01 02 00 00       	jmp    0x1400074b1
   1400072b0:	48 39 d1             	cmp    %rdx,%rcx
   1400072b3:	48 0f 46 ca          	cmovbe %rdx,%rcx
   1400072b7:	48 ff c9             	dec    %rcx
   1400072ba:	48 81 f9 bf f2 03 00 	cmp    $0x3f2bf,%rcx
   1400072c1:	77 4e                	ja     0x140007311
   1400072c3:	48 c1 e9 04          	shr    $0x4,%rcx
   1400072c7:	4c 8d 0d 7a f0 03 00 	lea    0x3f07a(%rip),%r9        # 0x140046348
   1400072ce:	46 0f b6 94 09 94 01 	movzbl 0x194(%rcx,%r9,1),%r10d
   1400072d5:	00 00 
   1400072d7:	41 80 fa 30          	cmp    $0x30,%r10b
   1400072db:	77 34                	ja     0x140007311
   1400072dd:	48 ff ca             	dec    %rdx
   1400072e0:	41 0f b6 c2          	movzbl %r10b,%eax
   1400072e4:	4d 8b 14 c1          	mov    (%r9,%rax,8),%r10
   1400072e8:	49 85 d2             	test   %rdx,%r10
   1400072eb:	0f 94 c1             	sete   %cl
   1400072ee:	74 23                	je     0x140007313
   1400072f0:	49 09 d2             	or     %rdx,%r10
   1400072f3:	49 81 fa bf f2 03 00 	cmp    $0x3f2bf,%r10
   1400072fa:	77 17                	ja     0x140007313
   1400072fc:	49 c1 ea 04          	shr    $0x4,%r10
   140007300:	47 0f b6 94 0a 94 01 	movzbl 0x194(%r10,%r9,1),%r10d
   140007307:	00 00 
   140007309:	41 80 fa 31          	cmp    $0x31,%r10b
   14000730d:	72 d1                	jb     0x1400072e0
   14000730f:	eb 02                	jmp    0x140007313
   140007311:	31 c9                	xor    %ecx,%ecx
   140007313:	83 fb 01             	cmp    $0x1,%ebx
   140007316:	74 6b                	je     0x140007383
   140007318:	85 db                	test   %ebx,%ebx
   14000731a:	0f 85 91 01 00 00    	jne    0x1400074b1
   140007320:	84 c9                	test   %cl,%cl
   140007322:	0f 84 89 01 00 00    	je     0x1400074b1
   140007328:	44 89 d9             	mov    %r11d,%ecx
   14000732b:	48 81 f9 00 f0 05 00 	cmp    $0x5f000,%rcx
   140007332:	0f 82 79 01 00 00    	jb     0x1400074b1
   140007338:	48 3b 0f             	cmp    (%rdi),%rcx
   14000733b:	0f 83 70 01 00 00    	jae    0x1400074b1
   140007341:	41 f6 c3 0f          	test   $0xf,%r11b
   140007345:	0f 85 66 01 00 00    	jne    0x1400074b1
   14000734b:	44 89 da             	mov    %r11d,%edx
   14000734e:	c1 ea 04             	shr    $0x4,%edx
   140007351:	44 8d 48 01          	lea    0x1(%rax),%r9d
   140007355:	44 38 8c 17 88 15 00 	cmp    %r9b,0x1588(%rdi,%rdx,1)
   14000735c:	00 
   14000735d:	0f 85 4e 01 00 00    	jne    0x1400074b1
   140007363:	48 83 f8 30          	cmp    $0x30,%rax
   140007367:	0f 86 5b 01 00 00    	jbe    0x1400074c8
   14000736d:	48 c7 04 0f 00 00 00 	movq   $0x0,(%rdi,%rcx,1)
   140007374:	00 
   140007375:	41 80 c9 80          	or     $0x80,%r9b
   140007379:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   140007380:	00 
   140007381:	eb 65                	jmp    0x1400073e8
   140007383:	84 c9                	test   %cl,%cl
   140007385:	0f 84 26 01 00 00    	je     0x1400074b1
   14000738b:	44 89 d9             	mov    %r11d,%ecx
   14000738e:	48 81 f9 00 20 04 00 	cmp    $0x42000,%rcx
   140007395:	0f 82 16 01 00 00    	jb     0x1400074b1
   14000739b:	48 3b 0f             	cmp    (%rdi),%rcx
   14000739e:	0f 83 0d 01 00 00    	jae    0x1400074b1
   1400073a4:	41 f6 c3 0f          	test   $0xf,%r11b
   1400073a8:	0f 85 03 01 00 00    	jne    0x1400074b1
   1400073ae:	44 89 da             	mov    %r11d,%edx
   1400073b1:	c1 ea 04             	shr    $0x4,%edx
   1400073b4:	44 0f b6 8c 17 88 15 	movzbl 0x1588(%rdi,%rdx,1),%r9d
   1400073bb:	00 00 
   1400073bd:	44 8d 50 01          	lea    0x1(%rax),%r10d
   1400073c1:	45 38 d1             	cmp    %r10b,%r9b
   1400073c4:	0f 85 e7 00 00 00    	jne    0x1400074b1
   1400073ca:	48 83 f8 30          	cmp    $0x30,%rax
   1400073ce:	0f 86 2d 01 00 00    	jbe    0x140007501
   1400073d4:	48 c7 04 0f 00 00 00 	movq   $0x0,(%rdi,%rcx,1)
   1400073db:	00 
   1400073dc:	41 80 c9 80          	or     $0x80,%r9b
   1400073e0:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   1400073e7:	00 
   1400073e8:	8b 4f 14             	mov    0x14(%rdi),%ecx
   1400073eb:	85 c9                	test   %ecx,%ecx
   1400073ed:	0f 84 dc 01 00 00    	je     0x1400075cf
   1400073f3:	4c 89 c3             	mov    %r8,%rbx
   1400073f6:	ff c9                	dec    %ecx
   1400073f8:	89 4f 14             	mov    %ecx,0x14(%rdi)
   1400073fb:	e9 4e 01 00 00       	jmp    0x14000754e
   140007400:	4c 89 c6             	mov    %r8,%rsi
   140007403:	e8 88 2c 03 00       	call   0x14003a090
   140007408:	49 89 f0             	mov    %rsi,%r8
   14000740b:	e9 a1 00 00 00       	jmp    0x1400074b1
   140007410:	48 8b 4f 30          	mov    0x30(%rdi),%rcx
   140007414:	4c 8b 4f 08          	mov    0x8(%rdi),%r9
   140007418:	48 01 f9             	add    %rdi,%rcx
   14000741b:	0f 82 90 00 00 00    	jb     0x1400074b1
   140007421:	48 39 c1             	cmp    %rax,%rcx
   140007424:	0f 85 87 00 00 00    	jne    0x1400074b1
   14000742a:	48 89 7c 24 20       	mov    %rdi,0x20(%rsp)
   14000742f:	48 89 f1             	mov    %rsi,%rcx
   140007432:	49 89 d1             	mov    %rdx,%r9
   140007435:	48 89 c2             	mov    %rax,%rdx
   140007438:	4c 89 c6             	mov    %r8,%rsi
   14000743b:	4d 89 c8             	mov    %r9,%r8
   14000743e:	49 89 f1             	mov    %rsi,%r9
   140007441:	e8 9a 52 03 00       	call   0x14003c6e0
   140007446:	49 89 f0             	mov    %rsi,%r8
   140007449:	eb 66                	jmp    0x1400074b1
   14000744b:	bb 00 f0 05 00       	mov    $0x5f000,%ebx
   140007450:	44 89 da             	mov    %r11d,%edx
   140007453:	48 39 da             	cmp    %rbx,%rdx
   140007456:	72 59                	jb     0x1400074b1
   140007458:	41 f6 c3 0f          	test   $0xf,%r11b
   14000745c:	75 53                	jne    0x1400074b1
   14000745e:	41 c1 eb 04          	shr    $0x4,%r11d
   140007462:	42 0f b6 9c 1f 88 15 	movzbl 0x1588(%rdi,%r11,1),%ebx
   140007469:	00 00 
   14000746b:	41 8d 6a 01          	lea    0x1(%r10),%ebp
   14000746f:	40 38 eb             	cmp    %bpl,%bl
   140007472:	75 3d                	jne    0x1400074b1
   140007474:	48 3b 17             	cmp    (%rdi),%rdx
   140007477:	73 38                	jae    0x1400074b1
   140007479:	48 8d 15 85 da 03 00 	lea    0x3da85(%rip),%rdx        # 0x140044f05
   140007480:	45 3a 0c 12          	cmp    (%r10,%rdx,1),%r9b
   140007484:	0f 83 1c 01 00 00    	jae    0x1400075a6
   14000748a:	48 01 f1             	add    %rsi,%rcx
   14000748d:	48 81 c1 a8 04 00 00 	add    $0x4a8,%rcx
   140007494:	41 0f b6 d1          	movzbl %r9b,%edx
   140007498:	80 cb 40             	or     $0x40,%bl
   14000749b:	42 88 9c 1f 88 15 00 	mov    %bl,0x1588(%rdi,%r11,1)
   1400074a2:	00 
   1400074a3:	83 e2 0f             	and    $0xf,%edx
   1400074a6:	48 89 44 d1 08       	mov    %rax,0x8(%rcx,%rdx,8)
   1400074ab:	41 fe c1             	inc    %r9b
   1400074ae:	44 88 09             	mov    %r9b,(%rcx)
   1400074b1:	4c 89 44 24 30       	mov    %r8,0x30(%rsp)
   1400074b6:	48 8d 44 24 30       	lea    0x30(%rsp),%rax
   1400074bb:	48 83 c4 38          	add    $0x38,%rsp
   1400074bf:	5b                   	pop    %rbx
   1400074c0:	5d                   	pop    %rbp
   1400074c1:	5f                   	pop    %rdi
   1400074c2:	5e                   	pop    %rsi
   1400074c3:	41 5e                	pop    %r14
   1400074c5:	41 5f                	pop    %r15
   1400074c7:	c3                   	ret
   1400074c8:	4c 89 c3             	mov    %r8,%rbx
   1400074cb:	44 8b 84 87 00 14 00 	mov    0x1400(%rdi,%rax,4),%r8d
   1400074d2:	00 
   1400074d3:	4e 8d 14 07          	lea    (%rdi,%r8,1),%r10
   1400074d7:	45 31 f6             	xor    %r14d,%r14d
   1400074da:	41 bf ff ff ff ff    	mov    $0xffffffff,%r15d
   1400074e0:	4d 39 f8             	cmp    %r15,%r8
   1400074e3:	4d 0f 45 f2          	cmovne %r10,%r14
   1400074e7:	4c 89 34 0f          	mov    %r14,(%rdi,%rcx,1)
   1400074eb:	44 89 9c 87 00 14 00 	mov    %r11d,0x1400(%rdi,%rax,4)
   1400074f2:	00 
   1400074f3:	41 80 c9 80          	or     $0x80,%r9b
   1400074f7:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   1400074fe:	00 
   1400074ff:	eb 37                	jmp    0x140007538
   140007501:	4c 89 c3             	mov    %r8,%rbx
   140007504:	44 8b 84 87 00 14 00 	mov    0x1400(%rdi,%rax,4),%r8d
   14000750b:	00 
   14000750c:	4e 8d 14 07          	lea    (%rdi,%r8,1),%r10
   140007510:	45 31 f6             	xor    %r14d,%r14d
   140007513:	41 bf ff ff ff ff    	mov    $0xffffffff,%r15d
   140007519:	4d 39 f8             	cmp    %r15,%r8
   14000751c:	4d 0f 45 f2          	cmovne %r10,%r14
   140007520:	4c 89 34 0f          	mov    %r14,(%rdi,%rcx,1)
   140007524:	44 89 9c 87 00 14 00 	mov    %r11d,0x1400(%rdi,%rax,4)
   14000752b:	00 
   14000752c:	41 80 c9 80          	or     $0x80,%r9b
   140007530:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   140007537:	00 
   140007538:	8b 4f 14             	mov    0x14(%rdi),%ecx
   14000753b:	85 c9                	test   %ecx,%ecx
   14000753d:	0f 84 8c 00 00 00    	je     0x1400075cf
   140007543:	ff c9                	dec    %ecx
   140007545:	89 4f 14             	mov    %ecx,0x14(%rdi)
   140007548:	41 83 f8 ff          	cmp    $0xffffffff,%r8d
   14000754c:	75 16                	jne    0x140007564
   14000754e:	44 8b 47 70          	mov    0x70(%rdi),%r8d
   140007552:	48 8b 8e e8 03 00 00 	mov    0x3e8(%rsi),%rcx
   140007559:	48 89 c2             	mov    %rax,%rdx
   14000755c:	e8 5f 1f 00 00       	call   0x1400094c0
   140007561:	8b 4f 14             	mov    0x14(%rdi),%ecx
   140007564:	85 c9                	test   %ecx,%ecx
   140007566:	49 89 d8             	mov    %rbx,%r8
   140007569:	0f 85 42 ff ff ff    	jne    0x1400074b1
   14000756f:	48 3b be a8 03 00 00 	cmp    0x3a8(%rsi),%rdi
   140007576:	0f 84 35 ff ff ff    	je     0x1400074b1
   14000757c:	83 7f 18 00          	cmpl   $0x0,0x18(%rdi)
   140007580:	0f 85 2b ff ff ff    	jne    0x1400074b1
   140007586:	80 7f 1c 01          	cmpb   $0x1,0x1c(%rdi)
   14000758a:	0f 85 21 ff ff ff    	jne    0x1400074b1
   140007590:	48 89 f1             	mov    %rsi,%rcx
   140007593:	48 89 fa             	mov    %rdi,%rdx
   140007596:	4c 89 c6             	mov    %r8,%rsi
   140007599:	e8 92 1f 00 00       	call   0x140009530
   14000759e:	49 89 f0             	mov    %rsi,%r8
   1400075a1:	e9 0b ff ff ff       	jmp    0x1400074b1
   1400075a6:	4a 8d 0c 1f          	lea    (%rdi,%r11,1),%rcx
   1400075aa:	48 81 c1 88 15 00 00 	add    $0x1588,%rcx
   1400075b1:	48 89 4c 24 20       	mov    %rcx,0x20(%rsp)
   1400075b6:	48 89 f1             	mov    %rsi,%rcx
   1400075b9:	48 89 c2             	mov    %rax,%rdx
   1400075bc:	4c 89 c6             	mov    %r8,%rsi
   1400075bf:	4d 89 d0             	mov    %r10,%r8
   1400075c2:	e8 f9 3c 03 00       	call   0x14003b2c0
   1400075c7:	49 89 f0             	mov    %rsi,%r8
   1400075ca:	e9 e2 fe ff ff       	jmp    0x1400074b1
   1400075cf:	48 8d 0d 6a 8b 03 00 	lea    0x38b6a(%rip),%rcx        # 0x140040140
   1400075d6:	ba 3a 00 00 00       	mov    $0x3a,%edx
   1400075db:	e8 40 50 03 00       	call   0x14003c620
   1400075e0:	0f 0b                	ud2
   1400075e2:	4c 8d 05 37 8c 03 00 	lea    0x38c37(%rip),%r8        # 0x140040220
   1400075e9:	ba 31 00 00 00       	mov    $0x31,%edx
   1400075ee:	4c 89 d1             	mov    %r10,%rcx
   1400075f1:	e8 dd 22 03 00       	call   0x1400398d3
   1400075f6:	0f 0b                	ud2
