   140006340:	48 83 ec 28          	sub    $0x28,%rsp
   140006344:	48 89 c8             	mov    %rcx,%rax
   140006347:	8b 0d 83 18 06 00    	mov    0x61883(%rip),%ecx        # 0x140067bd0
   14000634d:	65 4c 8b 0c 25 58 00 	mov    %gs:0x58,%r9
   140006354:	00 00 
   140006356:	49 8b 0c c9          	mov    (%r9,%rcx,8),%rcx
   14000635a:	48 8b 89 38 00 00 00 	mov    0x38(%rcx),%rcx
   140006361:	4c 8d 49 ff          	lea    -0x1(%rcx),%r9
   140006365:	49 83 f9 fd          	cmp    $0xfffffffffffffffd,%r9
   140006369:	0f 87 13 01 00 00    	ja     0x140006482
   14000636f:	49 83 f8 11          	cmp    $0x11,%r8
   140006373:	41 b9 10 00 00 00    	mov    $0x10,%r9d
   140006379:	4d 0f 43 c8          	cmovae %r8,%r9
   14000637d:	48 83 fa 11          	cmp    $0x11,%rdx
   140006381:	73 7f                	jae    0x140006402
   140006383:	49 ff c9             	dec    %r9
   140006386:	49 81 f9 bf f2 03 00 	cmp    $0x3f2bf,%r9
   14000638d:	0f 87 dd 00 00 00    	ja     0x140006470
   140006393:	49 c1 e9 04          	shr    $0x4,%r9
   140006397:	48 8d 05 ba ac 03 00 	lea    0x3acba(%rip),%rax        # 0x140041058
   14000639e:	45 0f b6 0c 01       	movzbl (%r9,%rax,1),%r9d
   1400063a3:	49 83 f9 30          	cmp    $0x30,%r9
   1400063a7:	0f 87 e1 00 00 00    	ja     0x14000648e
   1400063ad:	44 89 c8             	mov    %r9d,%eax
   1400063b0:	c1 e0 07             	shl    $0x7,%eax
   1400063b3:	4e 8d 14 c8          	lea    (%rax,%r9,8),%r10
   1400063b7:	42 0f b6 84 11 a8 04 	movzbl 0x4a8(%rcx,%r10,1),%eax
   1400063be:	00 00 
   1400063c0:	48 85 c0             	test   %rax,%rax
   1400063c3:	0f 84 b0 00 00 00    	je     0x140006479
   1400063c9:	4c 01 d1             	add    %r10,%rcx
   1400063cc:	48 81 c1 a8 04 00 00 	add    $0x4a8,%rcx
   1400063d3:	48 ff c8             	dec    %rax
   1400063d6:	88 01                	mov    %al,(%rcx)
   1400063d8:	83 e0 0f             	and    $0xf,%eax
   1400063db:	48 8b 44 c1 08       	mov    0x8(%rcx,%rax,8),%rax
   1400063e0:	89 c1                	mov    %eax,%ecx
   1400063e2:	48 89 c2             	mov    %rax,%rdx
   1400063e5:	48 81 e2 00 00 c0 ff 	and    $0xffffffffffc00000,%rdx
   1400063ec:	c1 e9 04             	shr    $0x4,%ecx
   1400063ef:	81 e1 ff ff 03 00    	and    $0x3ffff,%ecx
   1400063f5:	80 a4 0a 88 15 00 00 	andb   $0x3f,0x1588(%rdx,%rcx,1)
   1400063fc:	3f 
   1400063fd:	48 83 c4 28          	add    $0x28,%rsp
   140006401:	c3                   	ret
   140006402:	49 39 d1             	cmp    %rdx,%r9
   140006405:	4c 0f 46 ca          	cmovbe %rdx,%r9
   140006409:	49 ff c9             	dec    %r9
   14000640c:	49 81 f9 bf f2 03 00 	cmp    $0x3f2bf,%r9
   140006413:	77 5b                	ja     0x140006470
   140006415:	49 c1 e9 04          	shr    $0x4,%r9
   140006419:	48 8d 05 90 ff 03 00 	lea    0x3ff90(%rip),%rax        # 0x1400463b0
   140006420:	45 0f b6 8c 01 94 01 	movzbl 0x194(%r9,%rax,1),%r9d
   140006427:	00 00 
   140006429:	41 80 f9 30          	cmp    $0x30,%r9b
   14000642d:	77 41                	ja     0x140006470
   14000642f:	4c 8d 52 ff          	lea    -0x1(%rdx),%r10
   140006433:	66 66 66 66 2e 0f 1f 	data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   14000643a:	84 00 00 00 00 00 
   140006440:	45 0f b6 c9          	movzbl %r9b,%r9d
   140006444:	4e 8b 1c c8          	mov    (%rax,%r9,8),%r11
   140006448:	4d 85 d3             	test   %r10,%r11
   14000644b:	0f 84 5c ff ff ff    	je     0x1400063ad
   140006451:	4d 09 d3             	or     %r10,%r11
   140006454:	49 81 fb bf f2 03 00 	cmp    $0x3f2bf,%r11
   14000645b:	77 13                	ja     0x140006470
   14000645d:	49 c1 eb 04          	shr    $0x4,%r11
   140006461:	45 0f b6 8c 03 94 01 	movzbl 0x194(%r11,%rax,1),%r9d
   140006468:	00 00 
   14000646a:	41 80 f9 31          	cmp    $0x31,%r9b
   14000646e:	72 d0                	jb     0x140006440
   140006470:	48 83 c4 28          	add    $0x28,%rsp
   140006474:	e9 47 68 03 00       	jmp    0x14003ccc0
   140006479:	48 83 c4 28          	add    $0x28,%rsp
   14000647d:	e9 7e 68 03 00       	jmp    0x14003cd00
   140006482:	48 89 c1             	mov    %rax,%rcx
   140006485:	48 83 c4 28          	add    $0x28,%rsp
   140006489:	e9 32 66 03 00       	jmp    0x14003cac0
   14000648e:	4c 8d 05 4b 9e 03 00 	lea    0x39e4b(%rip),%r8        # 0x1400402e0
   140006495:	ba 31 00 00 00       	mov    $0x31,%edx
   14000649a:	4c 89 c9             	mov    %r9,%rcx
   14000649d:	e8 31 38 03 00       	call   0x140039cd3
   1400064a2:	0f 0b                	ud2
